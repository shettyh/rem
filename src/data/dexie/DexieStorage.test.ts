import { describe, it, expect, beforeEach, afterEach } from 'vitest'
import Dexie from 'dexie'
import { RemDB } from './db'
import { DexieStorage } from './DexieStorage'
import { MS_PER_DAY } from '../../domain/scheduler'

const DB_NAME = 'rem-test'
let db: RemDB
let storage: DexieStorage

beforeEach(async () => {
  await Dexie.delete(DB_NAME)
  db = new RemDB(DB_NAME)
  storage = new DexieStorage(db)
})

afterEach(() => {
  db.close()
})

describe('decks', () => {
  it('creates and lists a deck', async () => {
    const deck = await storage.createDeck('Spanish')
    expect(deck.id).toBeTruthy()
    expect(deck.name).toBe('Spanish')

    const decks = await storage.listDecks()
    expect(decks).toHaveLength(1)
    expect(decks[0].name).toBe('Spanish')
  })

  it('deletes a deck and cascades its cards', async () => {
    const deck = await storage.createDeck('Temp')
    await storage.createCard(deck.id, 'q', 'a')

    await storage.deleteDeck(deck.id)

    expect(await storage.getDeck(deck.id)).toBeUndefined()
    expect(await storage.listCards(deck.id)).toHaveLength(0)
  })
})

describe('cards', () => {
  it('creates a card with initial scheduling (due now, ease 2.5)', async () => {
    const deck = await storage.createDeck('Deck')
    const before = Date.now()
    const card = await storage.createCard(deck.id, 'front', 'back')

    expect(card.front).toBe('front')
    expect(card.scheduling).toMatchObject({ kind: 'sm2', easeFactor: 2.5, repetitions: 0 })
    expect(card.scheduling.due).toBeGreaterThanOrEqual(before)
  })

  it('updates card content', async () => {
    const deck = await storage.createDeck('Deck')
    const card = await storage.createCard(deck.id, 'q', 'a')

    await storage.updateCard(card.id, { front: 'q2', back: 'a2' })

    const updated = await storage.getCard(card.id)
    expect(updated?.front).toBe('q2')
    expect(updated?.back).toBe('a2')
  })

  it('deletes a single card', async () => {
    const deck = await storage.createDeck('Deck')
    const card = await storage.createCard(deck.id, 'q', 'a')

    await storage.deleteCard(card.id)

    expect(await storage.getCard(card.id)).toBeUndefined()
  })

  it('creates FSRS-scheduled cards in an FSRS deck', async () => {
    const deck = await storage.createDeck('Algo', 'fsrs')
    const card = await storage.createCard(deck.id, 'q', 'a')
    expect(card.scheduling.kind).toBe('fsrs')
  })
})

describe('due queue', () => {
  it('returns only cards due at or before now, soonest first', async () => {
    const deck = await storage.createDeck('Deck')
    const a = await storage.createCard(deck.id, 'a', 'a')
    const b = await storage.createCard(deck.id, 'b', 'b')

    const now = Date.now()
    // Push card A into the future so it is no longer due.
    await storage.updateCard(a.id, {
      scheduling: { kind: 'sm2', repetitions: 1, intervalDays: 1, easeFactor: 2.5, due: now + MS_PER_DAY },
    })

    const due = await storage.dueCards(deck.id, now)
    expect(due.map((c) => c.id)).toEqual([b.id])
    expect(await storage.countDue(deck.id, now)).toBe(1)
  })

  it('scopes the due queue to a single deck', async () => {
    const d1 = await storage.createDeck('One')
    const d2 = await storage.createDeck('Two')
    await storage.createCard(d1.id, 'a', 'a')
    await storage.createCard(d2.id, 'b', 'b')

    const now = Date.now() + 1000
    expect(await storage.countDue(d1.id, now)).toBe(1)
    expect(await storage.countDue(d2.id, now)).toBe(1)
  })
})

describe('sync storage', () => {
  it('exportSnapshot returns decks, cards, and tombstones', async () => {
    const deck = await storage.createDeck('S')
    await storage.createCard(deck.id, 'q', 'a')
    const snap = await storage.exportSnapshot()
    expect(snap.decks).toHaveLength(1)
    expect(snap.cards).toHaveLength(1)
    expect(snap.tombstones).toHaveLength(0)
  })

  it('deleteCard writes a card tombstone', async () => {
    const deck = await storage.createDeck('S')
    const c = await storage.createCard(deck.id, 'q', 'a')
    await storage.deleteCard(c.id)
    const snap = await storage.exportSnapshot()
    expect(snap.tombstones).toEqual([
      expect.objectContaining({ id: c.id, kind: 'card' }),
    ])
  })

  it('deleteDeck writes a deck tombstone', async () => {
    const deck = await storage.createDeck('S')
    await storage.deleteDeck(deck.id)
    const snap = await storage.exportSnapshot()
    expect(snap.tombstones).toEqual([
      expect.objectContaining({ id: deck.id, kind: 'deck' }),
    ])
  })

  it('applyMerge upserts and deletes records', async () => {
    const deck = await storage.createDeck('S')
    const stale = await storage.createCard(deck.id, 'old', 'old')
    await storage.applyMerge({
      upsertDecks: [{ id: deck.id, name: 'S', createdAt: deck.createdAt, schedulerKind: 'sm2' }],
      upsertCards: [{
        id: 'new', deckId: deck.id, front: 'new', back: 'new', createdAt: 1, updatedAt: 2,
        scheduling: { kind: 'sm2', repetitions: 0, intervalDays: 0, easeFactor: 2.5, due: 0 },
      }],
      deleteDeckIds: [],
      deleteCardIds: [stale.id],
      tombstones: [{ id: stale.id, kind: 'card', deletedAt: 5 }],
    })
    expect(await storage.getCard(stale.id)).toBeUndefined()
    expect(await storage.getCard('new')).toBeTruthy()
    expect((await storage.exportSnapshot()).tombstones).toHaveLength(1)
  })
})

describe('importDecks', () => {
  it('adds brand-new decks with their cards', async () => {
    const result = await storage.importDecks([
      {
        name: 'Spanish',
        createdAt: 5,
        schedulerKind: 'sm2',
        cards: [
          { front: 'hola', back: 'hello', createdAt: 6, updatedAt: 7, scheduling: { kind: 'sm2', repetitions: 2, intervalDays: 4, easeFactor: 2.7, due: 8 } },
        ],
      },
    ])

    expect(result).toEqual({ added: ['Spanish'], replaced: [] })
    const decks = await storage.listDecks()
    expect(decks).toHaveLength(1)
    const cards = await storage.listCards(decks[0].id)
    expect(cards).toHaveLength(1)
    expect(cards[0].front).toBe('hola')
    expect(cards[0].scheduling).toEqual({ kind: 'sm2', repetitions: 2, intervalDays: 4, easeFactor: 2.7, due: 8 })
    expect(cards[0].createdAt).toBe(6)
    expect(cards[0].updatedAt).toBe(7)
  })

  it('replaces a same-named deck, dropping its old cards', async () => {
    const old = await storage.createDeck('Spanish')
    const oldCard = await storage.createCard(old.id, 'old-front', 'old-back')

    const result = await storage.importDecks([
      { name: 'Spanish', createdAt: 5, schedulerKind: 'sm2', cards: [
        { front: 'new', back: 'new', createdAt: 6, updatedAt: 7, scheduling: { kind: 'sm2', repetitions: 0, intervalDays: 0, easeFactor: 2.5, due: 8 } },
      ] },
    ])

    expect(result).toEqual({ added: [], replaced: ['Spanish'] })
    const decks = await storage.listDecks()
    expect(decks).toHaveLength(1)
    expect(decks[0].id).not.toBe(old.id) // fresh id
    const cards = await storage.listCards(decks[0].id)
    expect(cards.map((c) => c.front)).toEqual(['new'])
    expect(await storage.getCard(oldCard.id)).toBeUndefined()
  })

  it('removes every existing deck sharing an incoming name', async () => {
    await storage.createDeck('Dup')
    await storage.createDeck('Dup')

    await storage.importDecks([{ name: 'Dup', createdAt: 1, schedulerKind: 'sm2', cards: [] }])

    const decks = await storage.listDecks()
    expect(decks.filter((d) => d.name === 'Dup')).toHaveLength(1)
  })
})

describe('assets', () => {
  it('stores an asset and reads it back by hash', async () => {
    const asset = await storage.putAsset(new Uint8Array([1, 2, 3]), 'image/png')
    expect(asset.hash).toHaveLength(64)
    expect(asset.mime).toBe('image/png')
    const got = await storage.getAsset(asset.hash)
    expect(got?.bytes).toEqual(new Uint8Array([1, 2, 3]))
  })

  it('dedupes identical bytes to one record', async () => {
    const a = await storage.putAsset(new Uint8Array([9, 9]), 'image/png')
    const b = await storage.putAsset(new Uint8Array([9, 9]), 'image/png')
    expect(b.hash).toBe(a.hash)
    expect(await storage.db.assets.count()).toBe(1)
  })
})
