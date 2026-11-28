import type { Card, Deck, SchedulingState } from '../../domain/models'
import type { Storage } from '../../data/Storage'
import { localDay } from './day'
import { buildSessionCards } from './session'

/** A card the user has never studied (still in the New state). */
export function isNew(s: SchedulingState): boolean {
  return s.state === 0
}

/** Local midnight epoch milliseconds for the day containing `now`. */
export function localDayStartMs(now: number): number {
  const d = new Date(now)
  d.setHours(0, 0, 0, 0)
  return d.getTime()
}

/** Build map of `siblingGroupId -> Set<cardId>` for cards reviewed today (by `lastReview` or `reviewLogs`). */
export async function loadReviewedTodaySiblingGroups(
  storage: Storage,
  decks: Deck[],
  now: number,
): Promise<Map<string, Set<string>>> {
  const dayStart = localDayStartMs(now)
  const reviewed = new Map<string, Set<string>>()
  const cardGroupById = new Map<string, string>()

  const perDeck = await Promise.all(
    decks.map(async (deck) => ({
      cards: await storage.listCards(deck.id),
      logs: typeof storage.listReviewLogs === 'function' ? await storage.listReviewLogs(deck.id) : [],
    })),
  )

  for (const { cards } of perDeck) {
    for (const card of cards) {
      if (!card.siblingGroupId) continue
      cardGroupById.set(card.id, card.siblingGroupId)
      const lr = card.scheduling.lastReview
      if (lr !== null && lr >= dayStart && lr <= now) {
        let set = reviewed.get(card.siblingGroupId)
        if (!set) {
          set = new Set()
          reviewed.set(card.siblingGroupId, set)
        }
        set.add(card.id)
      }
    }
  }
  for (const { logs } of perDeck) {
    for (const log of logs) {
      if (log.reviewedAt < dayStart || log.reviewedAt > now) continue
      const groupId = cardGroupById.get(log.cardId)
      if (!groupId) continue
      let set = reviewed.get(groupId)
      if (!set) {
        set = new Set()
        reviewed.set(groupId, set)
      }
      set.add(log.cardId)
    }
  }
  return reviewed
}

export function isSiblingBuriedToday(
  card: Card,
  reviewedTodayByGroup: Map<string, Set<string>>,
): boolean {
  if (card.scheduling.state === 1 || card.scheduling.state === 3) return false
  if (!card.siblingGroupId) return false
  const reviewedIds = reviewedTodayByGroup.get(card.siblingGroupId)
  if (!reviewedIds) return false
  for (const otherId of reviewedIds) {
    if (otherId !== card.id) return true
  }
  return false
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
  /** All due cards across every deck, in deck order. */
  queue: Card[]
  totalDue: number
  totalNew: number
  totalReview: number
  /** Total cards introduced or reviewed today across all decks. */
  reviewedToday: number
}

/**
 * Aggregate due counts across all decks for the Today screen, and collect the
 * cross-deck review queue. When the native `getTodaySummary` IPC is present, uses
 * the shared Rust `compute_today_summary` for exact buried-sibling + cap parity.
 */
export async function loadDueOverview(storage: Storage, now: number): Promise<DueOverview> {
  const allDecks = await storage.listDecks()

  if (storage.getTodaySummary) {
    const [summary, perDeckCards] = await Promise.all([
      storage.getTodaySummary(now),
      Promise.all(allDecks.map((d) => storage.listCards(d.id))),
    ])
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
      queue: [],
      totalDue: summary.totalDue,
      totalNew: summary.newDue,
      totalReview: summary.reviewDue,
      reviewedToday: summary.reviewedToday,
    }
  }

  const day = localDay(now)
  const reviewedTodayByGroup = await loadReviewedTodaySiblingGroups(storage, allDecks, now)
  const decks: DeckOverview[] = []
  const queue: Card[] = []
  let reviewedToday = 0
  let totalDue = 0
  let totalNew = 0

  for (const deck of allDecks) {
    const [cards, dueList, stat] = await Promise.all([
      storage.listCards(deck.id),
      storage.dueCards(deck.id, now),
      storage.getDailyStat(deck.id, day),
    ])
    reviewedToday += stat.newIntroduced + stat.reviewsDone
    const filteredDue = deck.settings.buryRelated
      ? dueList.filter((card) => !isSiblingBuriedToday(card, reviewedTodayByGroup))
      : dueList
    const caps = {
      newSlots: deck.settings.newPerDay - stat.newIntroduced,
      reviewSlots: deck.settings.maxReviews - stat.reviewsDone,
    }
    const capped = buildSessionCards(
      filteredDue.map((card) => ({ card, settings: deck.settings })),
      deck.settings.insertionOrder,
      caps,
      deck.settings.buryRelated,
    ).map((c) => c.card)

    // Count distinct studyable cards today when buryRelated is enabled.
    const countDistinct = (subset: Card[]) => {
      if (!deck.settings.buryRelated) return subset.length
      const seen = new Set<string>()
      let n = 0
      for (const c of subset) {
        if (c.scheduling.state === 1 || c.scheduling.state === 3 || !c.siblingGroupId) {
          n += 1
        } else if (!seen.has(c.siblingGroupId)) {
          seen.add(c.siblingGroupId)
          n += 1
        }
      }
      return n
    }

    const deckNewDue = countDistinct(capped.filter((c) => isNew(c.scheduling)))
    const deckReviewDue = countDistinct(capped.filter((c) => !isNew(c.scheduling)))
    const deckDue = deckNewDue + deckReviewDue
    totalDue += deckDue
    totalNew += deckNewDue

    decks.push({
      deck,
      due: deckDue,
      newCount: deckNewDue,
      total: cards.length,
      studied: cards.filter((c) => !isNew(c.scheduling)).length,
    })
    queue.push(...capped)
  }

  return { decks, queue, totalDue, totalNew, totalReview: totalDue - totalNew, reviewedToday }
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
