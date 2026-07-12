import { describe, it, expect, afterEach } from 'vitest'
import Dexie from 'dexie'
import { RemDB } from './db'
import { DEFAULT_DECK_SETTINGS } from '../../domain/models'

const NAME = 'rem-migration-test'

afterEach(async () => {
  await Dexie.delete(NAME)
})

describe('learning-step migration (v7)', () => {
  it('backfills step: 0 on pre-v7 card scheduling', async () => {
    const v6 = new Dexie(NAME)
    v6.version(6).stores({
      decks: 'id, createdAt',
      cards: 'id, deckId, createdAt',
      tombstones: 'id, deletedAt',
      assets: 'hash',
    })
    await v6.open()
    await v6.table('cards').add({
      id: 'c1', deckId: 'd1', front: 'q', back: 'a', createdAt: 1, updatedAt: 1,
      scheduling: { kind: 'fsrs', stability: 5, difficulty: 5, reps: 3, lapses: 1, state: 2, lastReview: 100, due: 200 },
    })
    v6.close()

    const db = new RemDB(NAME)
    const card = await db.cards.get('c1')
    expect(card?.scheduling.step).toBe(0)
    db.close()
  })
})

describe('deck settings migration (v6)', () => {
  it('backfills updatedAt, color and default settings on pre-v6 decks', async () => {
    const v5 = new Dexie(NAME)
    v5.version(5).stores({
      decks: 'id, createdAt',
      cards: 'id, deckId, createdAt',
      tombstones: 'id, deletedAt',
      assets: 'hash',
    })
    await v5.open()
    await v5.table('decks').add({ id: 'd1', name: 'Old', createdAt: 1234, schedulerKind: 'fsrs' })
    v5.close()

    const db = new RemDB(NAME)
    const deck = await db.decks.get('d1')
    expect(deck?.updatedAt).toBe(1234)
    expect(deck?.color).toBeTruthy()
    expect(deck?.settings).toEqual(DEFAULT_DECK_SETTINGS)
    db.close()
  })
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
    const fsrs = { kind: 'fsrs', stability: 5, difficulty: 5, reps: 3, lapses: 1, state: 2, step: 0, lastReview: 100, due: 200 }
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

describe('daily-caps migration (v8)', () => {
  it('adds an empty dailyStats table and leaves existing data intact', async () => {
    const v7 = new Dexie(NAME)
    v7.version(7).stores({
      decks: 'id, createdAt',
      cards: 'id, deckId, createdAt',
      tombstones: 'id, deletedAt',
      assets: 'hash',
    })
    await v7.open()
    await v7.table('cards').add({
      id: 'c1', deckId: 'd1', front: 'q', back: 'a', createdAt: 1, updatedAt: 1,
      scheduling: { kind: 'fsrs', stability: 5, difficulty: 5, reps: 3, lapses: 1, state: 2, step: 0, lastReview: 100, due: 200 },
    })
    v7.close()

    const db = new RemDB(NAME)
    const card = await db.cards.get('c1')
    expect(card?.front).toBe('q')
    expect(await db.dailyStats.count()).toBe(0)
    db.close()
  })
})
