import { describe, it, expect, beforeEach, afterEach } from 'vitest'
import Dexie from 'dexie'
import { RemDB } from './db'
import { DexieStorage } from './DexieStorage'
import { MS_PER_DAY } from '../../domain/scheduler'
import { DEFAULT_DECK_SETTINGS } from '../../domain/models'
import type { DbOps } from '../sync/merge'

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

async function currentSnapshot() {
  return (await storage.exportSnapshot()).snapshot
}

async function applyMerge(ops: DbOps) {
  const { revision } = await storage.exportSnapshot()
  return storage.applyMerge(ops, revision)
}

describe('decks', () => {
  it('creates and lists a deck', async () => {
    const deck = await storage.createDeck('Spanish')
    expect(deck.id).toBeTruthy()
    expect(deck.name).toBe('Spanish')

    const decks = await storage.listDecks()
    expect(decks).toHaveLength(1)
    expect(decks[0].name).toBe('Spanish')
  })

  it('seeds a new deck with defaults: updatedAt, color, default settings', async () => {
    const before = Date.now()
    const deck = await storage.createDeck('Spanish')
    expect(deck.updatedAt).toBeGreaterThanOrEqual(before)
    expect(deck.color).toBeTruthy()
    expect(deck.settings).toEqual(DEFAULT_DECK_SETTINGS)
  })

  it('updateDeck patches fields and bumps updatedAt', async () => {
    const deck = await storage.createDeck('Spanish')
    const next = { ...DEFAULT_DECK_SETTINGS, newPerDay: 35 }
    await storage.updateDeck(deck.id, { name: 'Español', color: '#2fa86b', settings: next })

    const updated = await storage.getDeck(deck.id)
    expect(updated?.name).toBe('Español')
    expect(updated?.color).toBe('#2fa86b')
    expect(updated?.settings.newPerDay).toBe(35)
    expect(updated!.updatedAt).toBeGreaterThanOrEqual(deck.updatedAt)
  })

  it('deletes a deck and cascades its cards and local drafts', async () => {
    const deck = await storage.createDeck('Temp')
    await storage.createCard(deck.id, 'q', 'a')
    await storage.proposeDrafts(
      deck.id,
      [{ front: 'draft', back: 'answer', tags: [], rationale: null, sources: [] }],
      { proposedBy: null },
    )

    await storage.deleteDeck(deck.id)

    expect(await storage.getDeck(deck.id)).toBeUndefined()
    expect(await storage.listCards(deck.id)).toHaveLength(0)
    expect(await storage.listDrafts()).toHaveLength(0)
  })
})

describe('cards', () => {
  it('creates a card with initial FSRS scheduling (due now, unreviewed)', async () => {
    const deck = await storage.createDeck('Deck')
    const before = Date.now()
    const card = await storage.createCard(deck.id, 'front', 'back')

    expect(card.front).toBe('front')
    expect(card.tags).toEqual([])
    expect(card.suspended).toBe(false)
    expect(card.lastAgainAt).toBeNull()
    expect(card.scheduling).toMatchObject({ kind: 'fsrs', reps: 0 })
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

describe('drafts', () => {
  it('keeps proposals local until atomic acceptance creates an edited due card', async () => {
    const deck = await storage.createDeck('Rust')
    const before = await storage.exportSnapshot()
    const proposed = await storage.proposeDrafts(
      deck.id,
      [{
        front: 'Original question',
        back: 'Original answer',
        tags: [' Rust ', 'rust'],
        rationale: ' Durable invariant ',
        sources: [{ locator: ' src/lib.rs:1-5 ', label: ' Core ' }],
      }],
      { proposedBy: ' pi ' },
    )

    expect(proposed.outcomes[0].status).toBe('created')
    if (proposed.outcomes[0].status !== 'created') throw new Error('expected created draft')
    const draft = proposed.outcomes[0].value
    expect(draft).toMatchObject({
      deckId: deck.id,
      tags: ['Rust'],
      rationale: 'Durable invariant',
      sources: [{ locator: 'src/lib.rs:1-5', label: 'Core' }],
      proposedBy: 'pi',
      revision: 0,
    })
    expect(await storage.listDrafts()).toEqual([draft])
    expect(await storage.listCards(deck.id)).toEqual([])
    expect((await storage.exportSnapshot()).revision).toBe(before.revision)

    const accepted = await storage.resolveDraft(draft.id, draft.revision, {
      decision: 'accept',
      deckId: deck.id,
      card: { front: 'Edited question', back: 'Edited answer', tags: ['accepted'] },
    })

    expect(accepted.status).toBe('accepted')
    if (accepted.status !== 'accepted') throw new Error('expected accepted card')
    expect(accepted.value).toMatchObject({
      deckId: deck.id,
      front: 'Edited question',
      back: 'Edited answer',
      tags: ['accepted'],
    })
    expect(accepted.value.scheduling.due).toBeGreaterThanOrEqual(draft.createdAt)
    expect(await storage.listDrafts()).toEqual([])
    expect(await storage.listCards(deck.id)).toEqual([accepted.value])
    expect((await storage.exportSnapshot()).revision).toBe(before.revision + 1)
  })

  it('keeps a stale draft decision pending and rejects it without advancing sync', async () => {
    const deck = await storage.createDeck('Rust')
    const proposed = await storage.proposeDrafts(
      deck.id,
      [{ front: 'Question', back: 'Answer', tags: [], rationale: null, sources: [] }],
      { proposedBy: null },
    )
    if (proposed.outcomes[0].status !== 'created') throw new Error('expected created draft')
    const draft = proposed.outcomes[0].value
    const revision = (await storage.exportSnapshot()).revision

    await expect(storage.resolveDraft(draft.id, 1, { decision: 'reject' }))
      .rejects.toThrow(`draft changed: ${draft.id}`)
    expect(await storage.listDrafts()).toEqual([draft])

    await expect(storage.resolveDraft(draft.id, 0, { decision: 'reject' }))
      .resolves.toEqual({ status: 'rejected' })
    expect(await storage.listDrafts()).toEqual([])
    expect((await storage.exportSnapshot()).revision).toBe(revision)
  })
})

describe('review commits', () => {
  it('atomically updates scheduling, bumps the daily counter, and appends an FSRS log', async () => {
    const deck = await storage.createDeck('History')
    const card = await storage.createCard(deck.id, 'q', 'a')
    const reviewedAt = Date.now()
    const scheduling = { ...card.scheduling, reps: 1, state: 2, lastReview: reviewedAt, due: reviewedAt + MS_PER_DAY }

    const log = await storage.commitReview({
      cardId: card.id,
      deckId: deck.id,
      patch: { scheduling },
      reviewedAt,
      fsrsGrade: 'good',
      daily: { day: '2026-08-07', field: 'newIntroduced' },
    })

    expect(await storage.getCard(card.id)).toMatchObject({ scheduling })
    expect(await storage.getDailyStat(deck.id, '2026-08-07')).toEqual({ newIntroduced: 1, reviewsDone: 0 })
    expect(log).toMatchObject({ deckId: deck.id, cardId: card.id, reviewedAt, grade: 'good' })
    expect(await storage.listReviewLogs(deck.id)).toEqual([log])
  })

  it('commits a fixed step without writing an optimizer log', async () => {
    const deck = await storage.createDeck('Steps')
    const card = await storage.createCard(deck.id, 'q', 'a')
    const scheduling = { ...card.scheduling, state: 1, step: 1, due: Date.now() + 60_000 }

    expect(await storage.commitReview({
      cardId: card.id,
      deckId: deck.id,
      patch: { scheduling },
      reviewedAt: Date.now(),
    })).toBeNull()
    expect(await storage.listReviewLogs(deck.id)).toEqual([])
  })

  it('orders logs chronologically and removes them with their card', async () => {
    const deck = await storage.createDeck('History')
    const card = await storage.createCard(deck.id, 'q', 'a')
    for (const [reviewedAt, fsrsGrade] of [[20, 'good'], [10, 'again']] as const) {
      await storage.commitReview({ cardId: card.id, deckId: deck.id, patch: {}, reviewedAt, fsrsGrade })
    }
    expect((await storage.listReviewLogs(deck.id)).map((log) => log.reviewedAt)).toEqual([10, 20])

    await storage.deleteCard(card.id)
    expect(await storage.listReviewLogs(deck.id)).toEqual([])
  })

  it('removes logs when their deck is deleted', async () => {
    const deck = await storage.createDeck('History')
    const card = await storage.createCard(deck.id, 'q', 'a')
    await storage.commitReview({ cardId: card.id, deckId: deck.id, patch: {}, reviewedAt: 10, fsrsGrade: 'good' })
    await storage.deleteDeck(deck.id)
    expect(await storage.listReviewLogs(deck.id)).toEqual([])
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
      scheduling: { kind: 'fsrs', stability: 1, difficulty: 5, reps: 1, lapses: 0, state: 2, step: 0, lastReview: now, due: now + MS_PER_DAY },
    })

    const due = await storage.dueCards(deck.id, now)
    expect(due.map((c) => c.id)).toEqual([b.id])
    expect(await storage.countDue(deck.id, now)).toBe(1)
  })

  it('keeps suspended cards editable but excludes them from the due queue', async () => {
    const deck = await storage.createDeck('Deck')
    const card = await storage.createCard(deck.id, 'q', 'a')
    await storage.updateCard(card.id, { tags: ['leech'], suspended: true })

    expect(await storage.getCard(card.id)).toMatchObject({ tags: ['leech'], suspended: true })
    expect(await storage.dueCards(deck.id, Date.now() + 1000)).toEqual([])
    expect(await storage.countDue(deck.id, Date.now() + 1000)).toBe(0)
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
    const snap = await currentSnapshot()
    expect(snap.decks).toHaveLength(1)
    expect(snap.cards).toHaveLength(1)
    expect(snap.tombstones).toHaveLength(0)
  })

  it('deleteCard writes a card tombstone', async () => {
    const deck = await storage.createDeck('S')
    const c = await storage.createCard(deck.id, 'q', 'a')
    await storage.deleteCard(c.id)
    const snap = await currentSnapshot()
    expect(snap.tombstones).toEqual([
      expect.objectContaining({ id: c.id, kind: 'card' }),
    ])
  })

  it('deleteDeck writes a deck tombstone', async () => {
    const deck = await storage.createDeck('S')
    await storage.deleteDeck(deck.id)
    const snap = await currentSnapshot()
    expect(snap.tombstones).toEqual([
      expect.objectContaining({ id: deck.id, kind: 'deck' }),
    ])
  })

  it('applyMerge upserts and deletes records', async () => {
    const deck = await storage.createDeck('S')
    const stale = await storage.createCard(deck.id, 'old', 'old')
    await applyMerge({
      upsertDecks: [{ id: deck.id, name: 'S', createdAt: deck.createdAt, updatedAt: deck.updatedAt, color: deck.color, schedulerKind: 'fsrs', settings: DEFAULT_DECK_SETTINGS }],
      upsertCards: [{
        id: 'new', deckId: deck.id, front: 'new', back: 'new', createdAt: 1, updatedAt: 2,
        tags: [], suspended: false, lastAgainAt: null,
        scheduling: { kind: 'fsrs', stability: 0, difficulty: 0, reps: 0, lapses: 0, state: 0, step: 0, lastReview: null, due: 0 },
      }],
      upsertReviewLogs: [],
      deleteReviewLogIds: [],
      deleteDeckIds: [],
      deleteCardIds: [stale.id],
      tombstones: [{ id: stale.id, kind: 'card', deletedAt: 5 }],
      upsertAssets: [],
      deleteAssetHashes: [],
    })
    expect(await storage.getCard(stale.id)).toBeUndefined()
    expect(await storage.getCard('new')).toBeTruthy()
    expect((await currentSnapshot()).tombstones).toHaveLength(1)
  })
})

describe('importDecks', () => {
  it('adds brand-new decks with their cards', async () => {
    const result = await storage.importDecks([
      {
        name: 'Spanish',
        createdAt: 5,
        schedulerKind: 'fsrs',
        settings: DEFAULT_DECK_SETTINGS,
        cards: [
          {
            front: 'hola', back: 'hello', createdAt: 6, updatedAt: 7, tags: ['leech'], suspended: true,
            lastAgainAt: 6,
            scheduling: { kind: 'fsrs', stability: 4, difficulty: 5, reps: 2, lapses: 0, state: 2, step: 0, lastReview: 7, due: 8 },
            reviews: [{ reviewedAt: 7, grade: 'good' }],
          },
        ],
      },
    ])

    expect(result).toEqual({ added: ['Spanish'], replaced: [] })
    const decks = await storage.listDecks()
    expect(decks).toHaveLength(1)
    const cards = await storage.listCards(decks[0].id)
    expect(cards).toHaveLength(1)
    expect(cards[0].front).toBe('hola')
    expect(cards[0].scheduling).toEqual({ kind: 'fsrs', stability: 4, difficulty: 5, reps: 2, lapses: 0, state: 2, step: 0, lastReview: 7, due: 8 })
    expect(cards[0].createdAt).toBe(6)
    expect(cards[0].updatedAt).toBe(7)
    expect(cards[0]).toMatchObject({ tags: ['leech'], suspended: true, lastAgainAt: 6 })
    expect(await storage.listReviewLogs(decks[0].id)).toEqual([
      expect.objectContaining({ cardId: cards[0].id, deckId: decks[0].id, reviewedAt: 7, grade: 'good' }),
    ])
  })

  it('replaces a same-named deck, dropping its old cards', async () => {
    const old = await storage.createDeck('Spanish')
    const oldCard = await storage.createCard(old.id, 'old-front', 'old-back')

    const result = await storage.importDecks([
      { name: 'Spanish', createdAt: 5, schedulerKind: 'fsrs', settings: DEFAULT_DECK_SETTINGS, cards: [
        { front: 'new', back: 'new', createdAt: 6, updatedAt: 7, tags: [], suspended: false, lastAgainAt: null, scheduling: { kind: 'fsrs', stability: 0, difficulty: 0, reps: 0, lapses: 0, state: 0, step: 0, lastReview: null, due: 8 }, reviews: [] },
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

    await storage.importDecks([{ name: 'Dup', createdAt: 1, schedulerKind: 'fsrs', settings: DEFAULT_DECK_SETTINGS, cards: [] }])

    const decks = await storage.listDecks()
    expect(decks.filter((d) => d.name === 'Dup')).toHaveLength(1)
  })

  it('imports decks with settings, a color, and a fresh updatedAt', async () => {
    const before = Date.now()
    await storage.importDecks([
      { name: 'Imported', createdAt: 5, schedulerKind: 'fsrs', settings: { ...DEFAULT_DECK_SETTINGS, newPerDay: 7 }, cards: [] },
    ])
    const deck = (await storage.listDecks()).find((d) => d.name === 'Imported')
    expect(deck?.settings.newPerDay).toBe(7)
    expect(deck?.color).toBeTruthy()
    expect(deck!.updatedAt).toBeGreaterThanOrEqual(before)
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

  it('sweeps assets not referenced by any card', async () => {
    const deck = await storage.createDeck('D')
    const used = await storage.putAsset(new Uint8Array([1]), 'image/png')
    const orphan = await storage.putAsset(new Uint8Array([2]), 'image/png')
    await storage.createCard(deck.id, `![x](asset:${used.hash})`, 'back')

    await storage.sweepOrphanAssets()

    expect(await storage.getAsset(used.hash)).toBeDefined()
    expect(await storage.getAsset(orphan.hash)).toBeUndefined()
  })
})

describe('snapshot assets', () => {
  it('exports stored assets in the snapshot', async () => {
    const a = await storage.putAsset(new Uint8Array([7]), 'image/gif')
    const snap = await currentSnapshot()
    expect(snap.assets.map((x) => x.hash)).toEqual([a.hash])
    expect(snap.assets[0].mime).toBe('image/gif')
  })

  it('applyMerge applies a synced deck color/settings change', async () => {
    const deck = await storage.createDeck('Spanish')
    const incoming = {
      id: deck.id,
      name: 'Spanish',
      createdAt: deck.createdAt,
      updatedAt: deck.updatedAt + 1000,
      color: '#e8638c',
      schedulerKind: 'fsrs' as const,
      settings: { ...DEFAULT_DECK_SETTINGS, newPerDay: 99 },
    }
    await applyMerge({
      upsertDecks: [incoming], upsertCards: [], upsertReviewLogs: [], deleteReviewLogIds: [],
      deleteDeckIds: [], deleteCardIds: [], tombstones: [], upsertAssets: [], deleteAssetHashes: [],
    })
    const after = await storage.getDeck(deck.id)
    expect(after?.color).toBe('#e8638c')
    expect(after?.settings.newPerDay).toBe(99)
  })

  it('applyMerge upserts new assets and deletes by hash', async () => {
    const keep = 'c'.repeat(64)
    await applyMerge({
      upsertDecks: [], upsertCards: [], upsertReviewLogs: [], deleteReviewLogIds: [],
      deleteDeckIds: [], deleteCardIds: [], tombstones: [],
      upsertAssets: [{ hash: keep, mime: 'image/png', bytes: new Uint8Array([5]) }],
      deleteAssetHashes: [],
    })
    expect((await storage.getAsset(keep))?.bytes).toEqual(new Uint8Array([5]))

    await applyMerge({
      upsertDecks: [], upsertCards: [], upsertReviewLogs: [], deleteReviewLogIds: [],
      deleteDeckIds: [], deleteCardIds: [], tombstones: [],
      upsertAssets: [], deleteAssetHashes: [keep],
    })
    expect(await storage.getAsset(keep)).toBeUndefined()
  })
})

describe('daily stats', () => {
  async function count(
    deckId: string,
    cardId: string,
    day: string,
    field: 'newIntroduced' | 'reviewsDone',
  ) {
    await storage.commitReview({
      cardId,
      deckId,
      patch: {},
      reviewedAt: Date.now(),
      daily: { day, field },
    })
  }

  it('returns zeros when no row exists', async () => {
    expect(await storage.getDailyStat('d1', '2026-07-12')).toEqual({ newIntroduced: 0, reviewsDone: 0 })
  })

  it('bumps and accumulates counters per (deck, day)', async () => {
    const deck = await storage.createDeck('S')
    const card = await storage.createCard(deck.id, 'q', 'a')
    await count(deck.id, card.id, '2026-07-12', 'newIntroduced')
    await count(deck.id, card.id, '2026-07-12', 'newIntroduced')
    await count(deck.id, card.id, '2026-07-12', 'reviewsDone')
    expect(await storage.getDailyStat(deck.id, '2026-07-12')).toEqual({ newIntroduced: 2, reviewsDone: 1 })
  })

  it('keeps different days separate', async () => {
    const deck = await storage.createDeck('S')
    const card = await storage.createCard(deck.id, 'q', 'a')
    await count(deck.id, card.id, '2026-07-12', 'reviewsDone')
    expect(await storage.getDailyStat(deck.id, '2026-07-13')).toEqual({ newIntroduced: 0, reviewsDone: 0 })
  })

  it('deleteDeck removes the deck stats', async () => {
    const deck = await storage.createDeck('S')
    const card = await storage.createCard(deck.id, 'q', 'a')
    await count(deck.id, card.id, '2026-07-12', 'newIntroduced')
    await storage.deleteDeck(deck.id)
    expect(await storage.getDailyStat(deck.id, '2026-07-12')).toEqual({ newIntroduced: 0, reviewsDone: 0 })
  })
})
