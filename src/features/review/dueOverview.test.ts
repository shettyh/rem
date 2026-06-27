import { describe, it, expect } from 'vitest'
import type { Card, Deck, ID, SchedulingState } from '../../domain/models'
import type { Storage } from '../../data/Storage'
import { isNew, loadDueOverview, shuffle } from './dueOverview'

function fsrs(reps: number): SchedulingState {
  return { kind: 'fsrs', stability: 1, difficulty: 5, reps, lapses: 0, state: 2, lastReview: null, due: 0 }
}
function card(id: ID, deckId: ID, scheduling: SchedulingState): Card {
  return { id, deckId, front: id, back: id, createdAt: 0, updatedAt: 0, scheduling }
}
function deck(id: ID): Deck {
  return { id, name: id, createdAt: 0, schedulerKind: 'fsrs' }
}

/** Minimal in-memory Storage exposing only what loadDueOverview reads. */
function fakeStorage(decks: Deck[], cards: Card[], dueIds: Set<ID>): Storage {
  return {
    listDecks: async () => decks,
    listCards: async (deckId: ID) => cards.filter((c) => c.deckId === deckId),
    dueCards: async (deckId: ID) => cards.filter((c) => c.deckId === deckId && dueIds.has(c.id)),
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
    expect(ov.queue.map((c) => c.id)).toEqual(['a1', 'a2'])

    const a = ov.decks.find((d) => d.deck.id === 'a')!
    const b = ov.decks.find((d) => d.deck.id === 'b')!
    expect(a).toMatchObject({ due: 2, newCount: 1, total: 2 })
    expect(b).toMatchObject({ due: 0, newCount: 1, total: 1 })
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
