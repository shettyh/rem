import { describe, expect, it } from 'vitest'
import type { Deck, Grade, ReviewLog } from '../../domain/models'
import { DEFAULT_DECK_SETTINGS } from '../../domain/models'
import { buildStats } from './stats'

const NOW = new Date(2026, 7, 7, 12).getTime()

function atDaysAgo(days: number, hour = 10): number {
  const date = new Date(NOW)
  date.setDate(date.getDate() - days)
  date.setHours(hour, 0, 0, 0)
  return date.getTime()
}

function deck(id: string, name: string): Deck {
  return {
    id, name, createdAt: 0, updatedAt: 0, color: '#7e6cff',
    schedulerKind: 'fsrs', settings: DEFAULT_DECK_SETTINGS,
  }
}

function log(id: string, deckId: string, daysAgo: number, grade: Grade, hour = 10): ReviewLog {
  return { id, cardId: `c-${id}`, deckId, reviewedAt: atDaysAgo(daysAgo, hour), grade }
}

const DECKS = [deck('a', 'Alpha'), deck('b', 'Beta')]

describe('buildStats', () => {
  it('aggregates reviews, recall, grades, active days, and zero-filled daily buckets', () => {
    const summary = buildStats([
      log('1', 'a', 0, 'good'),
      log('2', 'a', 0, 'again'),
      log('3', 'b', 2, 'easy'),
      log('old', 'b', 7, 'hard'), // outside a 7-day inclusive range
    ], DECKS, NOW, 7, null)

    expect(summary.totalReviews).toBe(3)
    expect(summary.recallRate).toBeCloseTo(2 / 3)
    expect(summary.activeDays).toBe(2)
    expect(summary.grades).toEqual({ again: 1, hard: 0, good: 1, easy: 1 })
    expect(summary.daily).toHaveLength(7)
    expect(summary.daily.map((day) => day.count)).toEqual([0, 0, 0, 0, 1, 0, 2])
  })

  it('includes the first local midnight and excludes future events', () => {
    const first = new Date(NOW)
    first.setDate(first.getDate() - 6)
    first.setHours(0, 0, 0, 0)
    const logs: ReviewLog[] = [
      { id: 'start', deckId: 'a', cardId: 'c1', reviewedAt: first.getTime(), grade: 'good' },
      { id: 'future', deckId: 'a', cardId: 'c2', reviewedAt: NOW + 1, grade: 'good' },
    ]
    expect(buildStats(logs, DECKS, NOW, 7, null).totalReviews).toBe(1)
  })

  it('computes an active streak through today or yesterday and stops at a gap', () => {
    const today = buildStats([
      log('1', 'a', 0, 'good'), log('2', 'a', 1, 'good'), log('3', 'a', 2, 'good'),
      log('4', 'a', 4, 'good'),
    ], DECKS, NOW, 7, null)
    expect(today.currentStreak).toBe(3)

    const throughYesterday = buildStats([
      log('1', 'a', 1, 'good'), log('2', 'a', 2, 'good'),
    ], DECKS, NOW, 7, null)
    expect(throughYesterday.currentStreak).toBe(2)

    expect(buildStats([log('1', 'a', 2, 'good')], DECKS, NOW, 7, null).currentStreak).toBe(0)
  })

  it('applies a deck filter to metrics, streak, and breakdown', () => {
    const summary = buildStats([
      log('1', 'a', 0, 'again'),
      log('2', 'b', 0, 'good'),
      log('3', 'b', 1, 'easy'),
    ], DECKS, NOW, 30, 'b')
    expect(summary.totalReviews).toBe(2)
    expect(summary.recallRate).toBe(1)
    expect(summary.currentStreak).toBe(2)
    expect(summary.byDeck.map((item) => item.deck.id)).toEqual(['b'])
  })

  it('orders active decks by review count then name and calculates each recall rate', () => {
    const summary = buildStats([
      log('1', 'a', 0, 'again'),
      log('2', 'a', 1, 'good'),
      log('3', 'b', 0, 'good'),
    ], DECKS, NOW, 30, null)
    expect(summary.byDeck.map((item) => item.deck.name)).toEqual(['Alpha', 'Beta'])
    expect(summary.byDeck[0]).toMatchObject({ reviews: 2, recallRate: 0.5 })
    expect(summary.byDeck[1]).toMatchObject({ reviews: 1, recallRate: 1 })
  })

  it('distinguishes no history from history outside the selected range', () => {
    expect(buildStats([], DECKS, NOW, 7, null).historyCount).toBe(0)
    const old = buildStats([log('old', 'a', 20, 'good')], DECKS, NOW, 7, null)
    expect(old.historyCount).toBe(1)
    expect(old.totalReviews).toBe(0)
    expect(old.recallRate).toBeNull()
  })
})
