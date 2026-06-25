import type { Card, Deck, SchedulingState } from '../../domain/models'
import type { Storage } from '../../data/Storage'

/** A card the user has never graded (first-time / brand new). */
export function isNew(s: SchedulingState): boolean {
  return s.kind === 'sm2' ? s.repetitions === 0 : s.reps === 0
}

export interface DeckOverview {
  deck: Deck
  /** Cards due at or before `now`. */
  due: number
  /** First-time cards in the deck (regardless of due date). */
  newCount: number
  /** Total cards in the deck. */
  total: number
}

export interface DueOverview {
  decks: DeckOverview[]
  /** All due cards across every deck, in deck order. */
  queue: Card[]
  totalDue: number
  totalNew: number
  totalReview: number
}

/**
 * Aggregate due counts across all decks for the Today screen, and collect the
 * cross-deck review queue. The Storage seam stays per-deck; this composes it.
 */
export async function loadDueOverview(storage: Storage, now: number): Promise<DueOverview> {
  const allDecks = await storage.listDecks()
  const decks: DeckOverview[] = []
  const queue: Card[] = []

  for (const deck of allDecks) {
    const [cards, dueList] = await Promise.all([
      storage.listCards(deck.id),
      storage.dueCards(deck.id, now),
    ])
    decks.push({
      deck,
      due: dueList.length,
      newCount: cards.filter((c) => isNew(c.scheduling)).length,
      total: cards.length,
    })
    queue.push(...dueList)
  }

  const totalDue = queue.length
  const totalNew = queue.filter((c) => isNew(c.scheduling)).length
  return { decks, queue, totalDue, totalNew, totalReview: totalDue - totalNew }
}

/** Fisher–Yates shuffle into a new array; `rng` is injectable for tests. */
export function shuffle<T>(items: T[], rng: () => number = Math.random): T[] {
  const a = items.slice()
  for (let i = a.length - 1; i > 0; i--) {
    const j = Math.floor(rng() * (i + 1))
    ;[a[i], a[j]] = [a[j], a[i]]
  }
  return a
}
