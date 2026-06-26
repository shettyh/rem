import type { Asset, Card, Deck, ID, SchedulerKind } from '../../domain/models'
import { hashBytes } from '../assetHash'
import { assetRefs } from '../assetRefs'
import { getScheduler } from '../../domain/scheduler'
import type { CardPatch, ImportResult, Storage } from '../Storage'
import { planImport, type DeckBackup } from '../backup'
import type { RepoSnapshot } from '../sync/snapshot'
import type { DbOps } from '../sync/merge'
import type { RemDB } from './db'

/** IndexedDB-backed {@link Storage}, using Dexie. */
export class DexieStorage implements Storage {
  constructor(readonly db: RemDB) {}

  async createDeck(name: string, kind: SchedulerKind = 'sm2'): Promise<Deck> {
    const deck: Deck = {
      id: crypto.randomUUID(),
      name: name.trim(),
      createdAt: Date.now(),
      schedulerKind: kind,
    }
    await this.db.decks.add(deck)
    return deck
  }

  listDecks(): Promise<Deck[]> {
    return this.db.decks.orderBy('createdAt').toArray()
  }

  getDeck(id: ID): Promise<Deck | undefined> {
    return this.db.decks.get(id)
  }

  async deleteDeck(id: ID): Promise<void> {
    await this.db.transaction('rw', this.db.decks, this.db.cards, this.db.tombstones, async () => {
      await this.db.cards.where('deckId').equals(id).delete()
      await this.db.decks.delete(id)
      await this.db.tombstones.put({ id, kind: 'deck', deletedAt: Date.now() })
    })
  }

  async createCard(deckId: ID, front: string, back: string): Promise<Card> {
    const now = Date.now()
    const deck = await this.db.decks.get(deckId)
    const kind = deck?.schedulerKind ?? 'sm2'
    const card: Card = {
      id: crypto.randomUUID(),
      deckId,
      front,
      back,
      createdAt: now,
      updatedAt: now,
      scheduling: getScheduler(kind).initial(now),
    }
    await this.db.cards.add(card)
    return card
  }

  getCard(id: ID): Promise<Card | undefined> {
    return this.db.cards.get(id)
  }

  listCards(deckId: ID): Promise<Card[]> {
    return this.db.cards.where('deckId').equals(deckId).sortBy('createdAt')
  }

  async updateCard(id: ID, patch: CardPatch): Promise<void> {
    await this.db.cards.update(id, { ...patch, updatedAt: Date.now() })
  }

  async deleteCard(id: ID): Promise<void> {
    await this.db.transaction('rw', this.db.cards, this.db.tombstones, async () => {
      await this.db.cards.delete(id)
      await this.db.tombstones.put({ id, kind: 'card', deletedAt: Date.now() })
    })
  }

  async dueCards(deckId: ID, now: number): Promise<Card[]> {
    const cards = await this.db.cards.where('deckId').equals(deckId).toArray()
    return cards
      .filter((c) => c.scheduling.due <= now)
      .sort((a, b) => a.scheduling.due - b.scheduling.due)
  }

  async countDue(deckId: ID, now: number): Promise<number> {
    const cards = await this.dueCards(deckId, now)
    return cards.length
  }

  async importDecks(decks: DeckBackup[]): Promise<ImportResult> {
    const incomingNames = decks.map((d) => d.name)
    return this.db.transaction('rw', this.db.decks, this.db.cards, async () => {
      const existing = await this.db.decks.toArray()
      const result = planImport(incomingNames, existing.map((d) => d.name))

      const toReplace = new Set(result.replaced)
      const deckIdsToDelete = existing.filter((d) => toReplace.has(d.name)).map((d) => d.id)
      for (const id of deckIdsToDelete) {
        await this.db.cards.where('deckId').equals(id).delete()
        await this.db.decks.delete(id)
      }

      for (const d of decks) {
        const deckId = crypto.randomUUID()
        await this.db.decks.add({ id: deckId, name: d.name, createdAt: d.createdAt, schedulerKind: d.schedulerKind })
        for (const c of d.cards) {
          await this.db.cards.add({
            id: crypto.randomUUID(),
            deckId,
            front: c.front,
            back: c.back,
            createdAt: c.createdAt,
            updatedAt: c.updatedAt,
            scheduling: c.scheduling,
          })
        }
      }
      return result
    })
  }

  async exportSnapshot(): Promise<RepoSnapshot> {
    const [decks, cards, tombstones] = await Promise.all([
      this.db.decks.toArray(),
      this.db.cards.toArray(),
      this.db.tombstones.toArray(),
    ])
    return { decks, cards, tombstones, assets: [] }
  }

  async applyMerge(ops: DbOps): Promise<void> {
    await this.db.transaction('rw', this.db.decks, this.db.cards, this.db.tombstones, async () => {
      if (ops.deleteCardIds.length) await this.db.cards.bulkDelete(ops.deleteCardIds)
      if (ops.deleteDeckIds.length) await this.db.decks.bulkDelete(ops.deleteDeckIds)
      if (ops.upsertDecks.length) await this.db.decks.bulkPut(ops.upsertDecks)
      if (ops.upsertCards.length) await this.db.cards.bulkPut(ops.upsertCards)
      if (ops.tombstones.length) await this.db.tombstones.bulkPut(ops.tombstones)
    })
  }

  async putAsset(bytes: Uint8Array, mime: string): Promise<Asset> {
    const hash = await hashBytes(bytes)
    const existing = await this.db.assets.get(hash)
    if (existing) return existing
    const asset: Asset = { hash, mime, bytes, createdAt: Date.now() }
    await this.db.assets.add(asset)
    return asset
  }

  getAsset(hash: ID): Promise<Asset | undefined> {
    return this.db.assets.get(hash)
  }

  async sweepOrphanAssets(): Promise<void> {
    const [cards, assets] = await Promise.all([this.db.cards.toArray(), this.db.assets.toArray()])
    const referenced = new Set(cards.flatMap((c) => [...assetRefs(c.front), ...assetRefs(c.back)]))
    const orphans = assets.filter((a) => !referenced.has(a.hash)).map((a) => a.hash)
    if (orphans.length) await this.db.assets.bulkDelete(orphans)
  }
}
