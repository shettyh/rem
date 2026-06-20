import Dexie, { type EntityTable } from 'dexie'
import type { Card, Deck } from '../../domain/models'

/** IndexedDB schema (v1). Indexed fields are listed; payloads are stored whole. */
export class RemDB extends Dexie {
  decks!: EntityTable<Deck, 'id'>
  cards!: EntityTable<Card, 'id'>

  constructor(name = 'rem') {
    super(name)
    this.version(1).stores({
      decks: 'id, createdAt',
      cards: 'id, deckId, createdAt',
    })
  }
}
