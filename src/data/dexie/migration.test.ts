import { describe, it, expect, afterEach } from 'vitest'
import Dexie from 'dexie'
import { RemDB } from './db'

const NAME = 'rem-migration-test'

afterEach(async () => {
  await Dexie.delete(NAME)
})

describe('SM-2 removal migration (v5)', () => {
  it('resets legacy v1 cards (no discriminant) to fresh FSRS state', async () => {
    // Write v1-shaped data with a bare v1 Dexie instance.
    const v1 = new Dexie(NAME)
    v1.version(1).stores({ decks: 'id, createdAt', cards: 'id, deckId, createdAt' })
    await v1.open()
    await v1.table('decks').add({ id: 'd1', name: 'Old', createdAt: 1 })
    await v1.table('cards').add({
      id: 'c1', deckId: 'd1', front: 'q', back: 'a', createdAt: 1, updatedAt: 1,
      scheduling: { repetitions: 1, intervalDays: 3, easeFactor: 2.5, due: 9 },
    })
    v1.close()

    // Reopen through RemDB (declares v2..v5) to run the upgrades.
    const before = Date.now()
    const db = new RemDB(NAME)
    const deck = await db.decks.get('d1')
    const card = await db.cards.get('c1')
    expect(deck?.schedulerKind).toBe('fsrs')
    expect(card?.scheduling.kind).toBe('fsrs')
    expect(card?.scheduling.reps).toBe(0)
    expect(card?.scheduling.due).toBeGreaterThanOrEqual(before)
    db.close()
  })

  it('resets stored SM-2 decks/cards to fresh FSRS state', async () => {
    // Seed a v4-shaped DB carrying explicit SM-2 records.
    const v4 = new Dexie(NAME)
    v4.version(4).stores({
      decks: 'id, createdAt',
      cards: 'id, deckId, createdAt',
      tombstones: 'id, deletedAt',
      assets: 'hash',
    })
    await v4.open()
    await v4.table('decks').add({ id: 'd1', name: 'Old', createdAt: 1, schedulerKind: 'sm2' })
    await v4.table('cards').add({
      id: 'c1', deckId: 'd1', front: 'q', back: 'a', createdAt: 1, updatedAt: 1,
      scheduling: { kind: 'sm2', repetitions: 4, intervalDays: 30, easeFactor: 2.6, due: 50 },
    })
    v4.close()

    const before = Date.now()
    const db = new RemDB(NAME)
    const deck = await db.decks.get('d1')
    const card = await db.cards.get('c1')
    expect(deck?.schedulerKind).toBe('fsrs')
    expect(card?.scheduling.kind).toBe('fsrs')
    expect(card?.scheduling.reps).toBe(0)
    expect(card?.scheduling.due).toBeGreaterThanOrEqual(before)
    db.close()
  })

  it('leaves existing FSRS cards untouched', async () => {
    const v4 = new Dexie(NAME)
    v4.version(4).stores({
      decks: 'id, createdAt',
      cards: 'id, deckId, createdAt',
      tombstones: 'id, deletedAt',
      assets: 'hash',
    })
    await v4.open()
    await v4.table('decks').add({ id: 'd1', name: 'Keep', createdAt: 1, schedulerKind: 'fsrs' })
    const fsrs = { kind: 'fsrs', stability: 5, difficulty: 5, reps: 3, lapses: 1, state: 2, lastReview: 100, due: 200 }
    await v4.table('cards').add({
      id: 'c1', deckId: 'd1', front: 'q', back: 'a', createdAt: 1, updatedAt: 1, scheduling: fsrs,
    })
    v4.close()

    const db = new RemDB(NAME)
    const card = await db.cards.get('c1')
    expect(card?.scheduling).toEqual(fsrs)
    db.close()
  })
})
