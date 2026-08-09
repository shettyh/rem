import type { Asset, Card, Deck, ID, ReviewLog, SchedulerKind } from '../../domain/models'
import { DEFAULT_DECK_SETTINGS } from '../../domain/models'
import { deckColor } from '../../ui/deckColor'
import { hashBytes } from '../assetHash'
import { assetRefs } from '../assetRefs'
import { getScheduler } from '../../domain/scheduler'
import type {
  ApplyMergeResult,
  CardPatch,
  DeckPatch,
  ImportResult,
  ReviewCommit,
  Storage,
  VersionedRepoSnapshot,
} from '../Storage'
import { planImport, type DeckBackup } from '../backup'
import type { DbOps } from '../sync/merge'
import type { RemDB } from './db'

/** IndexedDB-backed {@link Storage}, using Dexie. */
export class DexieStorage implements Storage {
  private readonly listeners = new Set<() => void>()
  private syncRevision = 0

  constructor(readonly db: RemDB) {}

  subscribe(listener: () => void): () => void {
    this.listeners.add(listener)
    return () => this.listeners.delete(listener)
  }

  async createDeck(name: string, kind: SchedulerKind = 'fsrs'): Promise<Deck> {
    const now = Date.now()
    const id = crypto.randomUUID()
    const deck: Deck = {
      id,
      name: name.trim(),
      createdAt: now,
      updatedAt: now,
      color: deckColor(id),
      schedulerKind: kind,
      settings: { ...DEFAULT_DECK_SETTINGS },
    }
    await this.db.decks.add(deck)
    this.notify()
    return deck
  }

  async updateDeck(id: ID, patch: DeckPatch): Promise<void> {
    await this.db.decks.update(id, { ...patch, updatedAt: Date.now() })
    this.notify()
  }

  listDecks(): Promise<Deck[]> {
    return this.db.decks.orderBy('createdAt').toArray()
  }

  getDeck(id: ID): Promise<Deck | undefined> {
    return this.db.decks.get(id)
  }

  async deleteDeck(id: ID): Promise<void> {
    await this.db.transaction(
      'rw',
      this.db.decks, this.db.cards, this.db.tombstones, this.db.dailyStats, this.db.reviewLogs,
      async () => {
        await this.db.cards.where('deckId').equals(id).delete()
        await this.db.dailyStats.where('deckId').equals(id).delete()
        await this.db.reviewLogs.where('deckId').equals(id).delete()
        await this.db.decks.delete(id)
        await this.db.tombstones.put({ id, kind: 'deck', deletedAt: Date.now() })
      },
    )
    this.notify()
  }

  async createCard(deckId: ID, front: string, back: string, tags: string[] = []): Promise<Card> {
    const now = Date.now()
    const card: Card = {
      id: crypto.randomUUID(),
      deckId,
      front,
      back,
      createdAt: now,
      updatedAt: now,
      tags,
      suspended: false,
      lastAgainAt: null,
      scheduling: getScheduler().initial(now),
    }
    await this.db.cards.add(card)
    this.notify()
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
    this.notify()
  }

  async commitReview(commit: ReviewCommit): Promise<ReviewLog | null> {
    const result = await this.db.transaction('rw', this.db.cards, this.db.dailyStats, this.db.reviewLogs, async () => {
      await this.db.cards.update(commit.cardId, { ...commit.patch, updatedAt: commit.reviewedAt })
      if (commit.daily) {
        const id = `${commit.deckId}:${commit.daily.day}`
        const row = await this.db.dailyStats.get(id)
        const base = row ?? {
          id,
          deckId: commit.deckId,
          day: commit.daily.day,
          newIntroduced: 0,
          reviewsDone: 0,
        }
        const field = commit.daily.field
        await this.db.dailyStats.put({ ...base, [field]: base[field] + 1 })
      }
      if (!commit.fsrsGrade) return null
      const log: ReviewLog = {
        id: crypto.randomUUID(),
        deckId: commit.deckId,
        cardId: commit.cardId,
        reviewedAt: commit.reviewedAt,
        grade: commit.fsrsGrade,
      }
      await this.db.reviewLogs.add(log)
      return log
    })
    this.notify()
    return result
  }

  listReviewLogs(deckId: ID): Promise<ReviewLog[]> {
    return this.db.reviewLogs.where('deckId').equals(deckId).sortBy('reviewedAt')
  }

  async deleteCard(id: ID): Promise<void> {
    await this.db.transaction('rw', this.db.cards, this.db.tombstones, this.db.reviewLogs, async () => {
      await this.db.reviewLogs.where('cardId').equals(id).delete()
      await this.db.cards.delete(id)
      await this.db.tombstones.put({ id, kind: 'card', deletedAt: Date.now() })
    })
    this.notify()
  }

  async dueCards(deckId: ID, now: number): Promise<Card[]> {
    const cards = await this.db.cards.where('deckId').equals(deckId).toArray()
    return cards
      .filter((c) => !c.suspended && c.scheduling.due <= now)
      .sort((a, b) => a.scheduling.due - b.scheduling.due)
  }

  async countDue(deckId: ID, now: number): Promise<number> {
    const cards = await this.dueCards(deckId, now)
    return cards.length
  }

  async getDailyStat(deckId: ID, day: string): Promise<{ newIntroduced: number; reviewsDone: number }> {
    const row = await this.db.dailyStats.get(`${deckId}:${day}`)
    return { newIntroduced: row?.newIntroduced ?? 0, reviewsDone: row?.reviewsDone ?? 0 }
  }

  async importDecks(decks: DeckBackup[]): Promise<ImportResult> {
    const incomingNames = decks.map((d) => d.name)
    const result = await this.db.transaction('rw', this.db.decks, this.db.cards, this.db.reviewLogs, async () => {
      const existing = await this.db.decks.toArray()
      const result = planImport(incomingNames, existing.map((d) => d.name))

      const toReplace = new Set(result.replaced)
      const deckIdsToDelete = existing.filter((d) => toReplace.has(d.name)).map((d) => d.id)
      for (const id of deckIdsToDelete) {
        await this.db.cards.where('deckId').equals(id).delete()
        await this.db.reviewLogs.where('deckId').equals(id).delete()
        await this.db.decks.delete(id)
      }

      for (const d of decks) {
        const deckId = crypto.randomUUID()
        await this.db.decks.add({
          id: deckId,
          name: d.name,
          createdAt: d.createdAt,
          updatedAt: Date.now(),
          color: d.color ?? deckColor(deckId),
          schedulerKind: d.schedulerKind,
          settings: d.settings,
        })
        for (const c of d.cards) {
          const cardId = crypto.randomUUID()
          await this.db.cards.add({
            id: cardId,
            deckId,
            front: c.front,
            back: c.back,
            createdAt: c.createdAt,
            updatedAt: c.updatedAt,
            tags: c.tags,
            suspended: c.suspended,
            lastAgainAt: c.lastAgainAt,
            scheduling: c.scheduling,
          })
          for (const review of c.reviews) {
            await this.db.reviewLogs.add({
              id: crypto.randomUUID(),
              deckId,
              cardId,
              reviewedAt: review.reviewedAt,
              grade: review.grade,
            })
          }
        }
      }
      return result
    })
    this.notify()
    return result
  }

  async exportSnapshot(): Promise<VersionedRepoSnapshot> {
    const [decks, cards, reviewLogs, tombstones, assets] = await Promise.all([
      this.db.decks.toArray(),
      this.db.cards.toArray(),
      this.db.reviewLogs.toArray(),
      this.db.tombstones.toArray(),
      this.db.assets.toArray(),
    ])
    return {
      revision: this.syncRevision,
      snapshot: {
        decks,
        cards,
        reviewLogs,
        tombstones,
        assets: assets.map(({ hash, mime, bytes }) => ({ hash, mime, bytes })),
      },
    }
  }

  async applyMerge(ops: DbOps, expectedRevision: number): Promise<ApplyMergeResult> {
    if (expectedRevision !== this.syncRevision) {
      return { status: 'stale', currentRevision: this.syncRevision }
    }
    await this.db.transaction(
      'rw',
      this.db.decks, this.db.cards, this.db.reviewLogs, this.db.tombstones, this.db.assets,
      async () => {
        if (ops.deleteReviewLogIds.length) await this.db.reviewLogs.bulkDelete(ops.deleteReviewLogIds)
        if (ops.deleteCardIds.length) await this.db.cards.bulkDelete(ops.deleteCardIds)
        if (ops.deleteDeckIds.length) await this.db.decks.bulkDelete(ops.deleteDeckIds)
        if (ops.deleteAssetHashes.length) await this.db.assets.bulkDelete(ops.deleteAssetHashes)
        if (ops.upsertDecks.length) await this.db.decks.bulkPut(ops.upsertDecks)
        if (ops.upsertCards.length) await this.db.cards.bulkPut(ops.upsertCards)
        if (ops.upsertReviewLogs.length) await this.db.reviewLogs.bulkPut(ops.upsertReviewLogs)
        if (ops.upsertAssets.length) {
          await this.db.assets.bulkPut(
            ops.upsertAssets.map((a) => ({ ...a, createdAt: Date.now() })),
          )
        }
        if (ops.tombstones.length) await this.db.tombstones.bulkPut(ops.tombstones)
      },
    )
    this.notify()
    return { status: 'applied', revision: this.syncRevision }
  }

  async putAsset(bytes: Uint8Array, mime: string): Promise<Asset> {
    const hash = await hashBytes(bytes)
    const asset = await this.db.transaction('rw', this.db.assets, async () => {
      const existing = await this.db.assets.get(hash)
      if (existing) return existing
      const created: Asset = { hash, mime, bytes, createdAt: Date.now() }
      await this.db.assets.add(created)
      return created
    })
    this.notify()
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
    this.notify()
  }

  private notify(): void {
    this.syncRevision++
    for (const listener of this.listeners) listener()
  }
}
