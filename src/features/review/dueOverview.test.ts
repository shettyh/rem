import { describe, it, expect } from 'vitest'
import type { Card, Deck, ID, SchedulingState } from '../../domain/models'
import { DEFAULT_DECK_SETTINGS } from '../../domain/models'
import type { Storage } from '../../data/Storage'
import { isNew, loadDueOverview, shuffle } from './dueOverview'

function fsrs(reps: number): SchedulingState {
  return { kind: 'fsrs', stability: 1, difficulty: 5, reps, lapses: 0, state: reps === 0 ? 0 : 2, step: 0, lastReview: null, due: 0 }
}
function card(id: ID, deckId: ID, scheduling: SchedulingState): Card {
  return { id, deckId, front: id, back: id, createdAt: 0, updatedAt: 0, tags: [], suspended: false, lastAgainAt: null, scheduling }
}
function deck(id: ID): Deck {
  return { id, name: id, createdAt: 0, updatedAt: 0, color: '#7e6cff', schedulerKind: 'fsrs', settings: DEFAULT_DECK_SETTINGS }
}

/** Minimal in-memory Storage exposing only what loadDueOverview reads. */
function fakeStorage(
  decks: Deck[],
  cards: Card[],
  dueIds: Set<ID>,
  stats: Map<ID, { newIntroduced: number; reviewsDone: number }> = new Map(),
): Storage {
  return {
    listDecks: async () => decks,
    listCards: async (deckId: ID) => cards.filter((c) => c.deckId === deckId),
    dueCards: async (deckId: ID) => cards.filter((c) => c.deckId === deckId && dueIds.has(c.id)),
    getDailyStat: async (deckId: ID) => stats.get(deckId) ?? { newIntroduced: 0, reviewsDone: 0 },
  } as unknown as Storage
}

describe('isNew', () => {
  it('is true only for first-time cards', () => {
    expect(isNew(fsrs(0))).toBe(true)
    expect(isNew(fsrs(2))).toBe(false)
  })
})

describe('loadDueOverview', () => {
  it('aggregates due / new / review across decks', async () => {
    const decks = [deck('a'), deck('b')]
    const cards = [
      card('a1', 'a', fsrs(0)), // new + due
      card('a2', 'a', fsrs(4)), // review + due
      card('b1', 'b', fsrs(0)), // new, NOT due
    ]
    const due = new Set(['a1', 'a2'])
    const ov = await loadDueOverview(fakeStorage(decks, cards, due), Date.now())

    expect(ov.totalDue).toBe(2)
    expect(ov.totalNew).toBe(1)
    expect(ov.totalReview).toBe(1)
    // buildSessionCards orders reviews/in-progress before new cards.
    expect(ov.queue.map((c) => c.id)).toEqual(['a2', 'a1'])

    const a = ov.decks.find((d) => d.deck.id === 'a')!
    const b = ov.decks.find((d) => d.deck.id === 'b')!
    expect(a).toMatchObject({ due: 2, newCount: 1, total: 2 })
    // b1 is new but not due, so it's outside the capped/due queue newCount counts from.
    expect(b).toMatchObject({ due: 0, newCount: 0, total: 1 })
  })

  it('reports an empty overview when nothing is due', async () => {
    const decks = [deck('a')]
    const cards = [card('a1', 'a', fsrs(4))]
    const ov = await loadDueOverview(fakeStorage(decks, cards, new Set()), Date.now())
    expect(ov.totalDue).toBe(0)
    expect(ov.totalNew).toBe(0)
    expect(ov.totalReview).toBe(0)
    expect(ov.queue).toEqual([])
  })

  it('caps new and review counts by the deck limits', async () => {
    const d = deck('a')
    d.settings = { ...DEFAULT_DECK_SETTINGS, newPerDay: 1, maxReviews: 1 }
    const cards = [
      card('n1', 'a', fsrs(0)), card('n2', 'a', fsrs(0)),
      card('r1', 'a', fsrs(4)), card('r2', 'a', fsrs(4)),
    ]
    const due = new Set(['n1', 'n2', 'r1', 'r2'])
    const ov = await loadDueOverview(fakeStorage([d], cards, due), Date.now())
    expect(ov.totalNew).toBe(1)
    expect(ov.totalReview).toBe(1)
    expect(ov.totalDue).toBe(2)
    expect(ov.decks[0]).toMatchObject({ due: 2, newCount: 1 })
  })

  it('subtracts allowance already spent today', async () => {
    const d = deck('a')
    d.settings = { ...DEFAULT_DECK_SETTINGS, newPerDay: 2, maxReviews: 5 }
    const cards = [card('n1', 'a', fsrs(0)), card('n2', 'a', fsrs(0))]
    const due = new Set(['n1', 'n2'])
    const stats = new Map([['a', { newIntroduced: 2, reviewsDone: 0 }]])
    const ov = await loadDueOverview(fakeStorage([d], cards, due, stats), Date.now())
    expect(ov.totalNew).toBe(0)
  })
})

describe('shuffle', () => {
  it('preserves all elements without mutating the input', () => {
    const input = [1, 2, 3, 4, 5]
    const out = shuffle(input, () => 0)
    expect([...out].sort((x, y) => x - y)).toEqual([1, 2, 3, 4, 5])
    expect(input).toEqual([1, 2, 3, 4, 5]) // not mutated
  })

  it('is deterministic for a given rng', () => {
    const seq = [0.1, 0.9, 0.3, 0.7]
    let i = 0
    const rng = () => seq[i++ % seq.length]
    const a = shuffle([1, 2, 3, 4, 5], rng)
    i = 0
    const b = shuffle([1, 2, 3, 4, 5], rng)
    expect(a).toEqual(b)
  })
})
