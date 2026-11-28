import type { Deck, SchedulingState } from '../../domain/models'
import type { Storage } from '../../data/Storage'

/** A card the user has never studied (still in the New state). */
export function isNew(s: SchedulingState): boolean {
  return s.state === 0
}

export interface DeckOverview {
  deck: Deck
  /** Cards due at or before `now` (respecting daily caps and sibling burying). */
  due: number
  /** First-time cards among the due/capped set. */
  newCount: number
  /** Total cards in the deck. */
  total: number
  /** Cards studied at least once (not in New state). */
  studied: number
}

export interface DueOverview {
  decks: DeckOverview[]
  totalDue: number
  totalNew: number
  totalReview: number
  /** Total cards introduced or reviewed today across all decks. */
  reviewedToday: number
}

/**
 * Aggregate due counts across all decks for the Today screen using the shared
 * Rust `compute_today_summary` IPC.
 */
export async function loadDueOverview(storage: Storage, now: number): Promise<DueOverview> {
  const [allDecks, summary] = await Promise.all([
    storage.listDecks(),
    storage.getTodaySummary(now),
  ])
  const perDeckCards = await Promise.all(allDecks.map((d) => storage.listCards(d.id)))
  const byId = new Map(summary.decks.map((d) => [d.id, d]))
  const decks: DeckOverview[] = allDecks.map((deck, idx) => {
    const s = byId.get(deck.id)
    const cards = perDeckCards[idx] ?? []
    return {
      deck,
      due: s?.dueCount ?? 0,
      newCount: s?.newDueCount ?? 0,
      total: s?.totalCards ?? cards.length,
      studied: cards.filter((c) => !isNew(c.scheduling)).length,
    }
  })
  return {
    decks,
    totalDue: summary.totalDue,
    totalNew: summary.newDue,
    totalReview: summary.reviewDue,
    reviewedToday: summary.reviewedToday,
  }
}
