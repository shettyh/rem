import Dexie, { type EntityTable } from 'dexie'
import type { Card, Deck, Tombstone } from '../../domain/models'

/** IndexedDB schema. Indexed fields are listed; payloads are stored whole. */
export class RemDB extends Dexie {
  decks!: EntityTable<Deck, 'id'>
  cards!: EntityTable<Card, 'id'>
  tombstones!: EntityTable<Tombstone, 'id'>

  constructor(name = 'rem') {
    super(name)
    this.version(1).stores({
      decks: 'id, createdAt',
      cards: 'id, deckId, createdAt',
    })
    // v2: stamp the scheduling-algorithm discriminant onto pre-existing
    // records written before per-deck schedulers existed. Schema unchanged.
    this.version(2)
      .stores({
        decks: 'id, createdAt',
        cards: 'id, deckId, createdAt',
      })
      .upgrade(async (tx) => {
        await tx.table('decks').toCollection().modify((d) => {
          if (!d.schedulerKind) d.schedulerKind = 'sm2'
        })
        await tx.table('cards').toCollection().modify((c) => {
          if (c.scheduling && !c.scheduling.kind) c.scheduling.kind = 'sm2'
        })
      })
    // v3: add the tombstones table for deletion sync. Additive — existing data untouched.
    this.version(3).stores({
      decks: 'id, createdAt',
      cards: 'id, deckId, createdAt',
      tombstones: 'id, deletedAt',
    })
  }
}
