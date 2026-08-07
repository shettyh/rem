import type { Deck, Grade, ReviewLog } from '../../domain/models'
import { localDay } from '../review/day'

export type StatsRange = 7 | 30 | 90

export interface DailyActivity {
  day: string
  timestamp: number
  count: number
}

export interface DeckStats {
  deck: Deck
  reviews: number
  recallRate: number
}

export interface StatsSummary {
  historyCount: number
  totalReviews: number
  recallRate: number | null
  activeDays: number
  currentStreak: number
  grades: Record<Grade, number>
  daily: DailyActivity[]
  byDeck: DeckStats[]
}

/** Build the complete view model for one stats scope and local-calendar range. */
export function buildStats(
  logs: ReviewLog[],
  decks: Deck[],
  now: number,
  range: StatsRange,
  deckId: string | null,
): StatsSummary {
  const deckById = new Map(decks.map((deck) => [deck.id, deck]))
  const scopedHistory = logs.filter((log) => (
    log.reviewedAt <= now &&
    deckById.has(log.deckId) &&
    (deckId === null || log.deckId === deckId)
  ))

  const start = new Date(now)
  start.setHours(0, 0, 0, 0)
  start.setDate(start.getDate() - (range - 1))
  const ranged = scopedHistory.filter((log) => log.reviewedAt >= start.getTime())

  const grades: Record<Grade, number> = { again: 0, hard: 0, good: 0, easy: 0 }
  const activity = new Map<string, number>()
  for (const log of ranged) {
    grades[log.grade] += 1
    const day = localDay(log.reviewedAt)
    activity.set(day, (activity.get(day) ?? 0) + 1)
  }

  const daily = Array.from({ length: range }, (_, index) => {
    const date = new Date(start)
    date.setDate(start.getDate() + index)
    const day = localDay(date.getTime())
    return { day, timestamp: date.getTime(), count: activity.get(day) ?? 0 }
  })

  const totalReviews = ranged.length
  const recalled = grades.hard + grades.good + grades.easy
  const byDeck = decks
    .filter((deck) => deckId === null || deck.id === deckId)
    .map((deck) => {
      const deckLogs = ranged.filter((log) => log.deckId === deck.id)
      const deckRecalled = deckLogs.filter((log) => log.grade !== 'again').length
      return {
        deck,
        reviews: deckLogs.length,
        recallRate: deckLogs.length === 0 ? 0 : deckRecalled / deckLogs.length,
      }
    })
    .filter((item) => item.reviews > 0)
    .sort((a, b) => b.reviews - a.reviews || a.deck.name.localeCompare(b.deck.name))

  return {
    historyCount: scopedHistory.length,
    totalReviews,
    recallRate: totalReviews === 0 ? null : recalled / totalReviews,
    activeDays: activity.size,
    currentStreak: currentStreak(scopedHistory, now),
    grades,
    daily,
    byDeck,
  }
}

function currentStreak(logs: ReviewLog[], now: number): number {
  const active = new Set(logs.map((log) => calendarOrdinal(log.reviewedAt)))
  const today = calendarOrdinal(now)
  let cursor = active.has(today) ? today : active.has(today - 1) ? today - 1 : -1
  let streak = 0
  while (active.has(cursor)) {
    streak += 1
    cursor -= 1
  }
  return streak
}

/** Local calendar date encoded as a DST-independent consecutive integer. */
function calendarOrdinal(ms: number): number {
  const date = new Date(ms)
  return Math.floor(Date.UTC(date.getFullYear(), date.getMonth(), date.getDate()) / 86_400_000)
}
