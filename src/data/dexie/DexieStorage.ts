import type { Card, Deck, ID, SchedulerKind } from '../../domain/models'
import type { Scheduler } from '../../domain/scheduler'
import type { CardPatch, ImportResult, Storage } from '../Storage'
import { planImport, type DeckBackup } from '../backup'
import type { RemDB } from './db'

/** IndexedDB-backed {@link Storage}, using Dexie. */
export class DexieStorage implements Storage {
  constructor(
    private readonly db: RemDB,
    private readonly scheduler: Scheduler,
  ) {}

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
    await this.db.transaction('rw', this.db.decks, this.db.cards, async () => {
      await this.db.cards.where('deckId').equals(id).delete()
      await this.db.decks.delete(id)
    })
  }

  async createCard(deckId: ID, front: string, back: string): Promise<Card> {
    const now = Date.now()
    const card: Card = {
      id: crypto.randomUUID(),
      deckId,
      front,
      back,
      createdAt: now,
      updatedAt: now,
      scheduling: this.scheduler.initial(now),
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
    await this.db.cards.delete(id)
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
}
