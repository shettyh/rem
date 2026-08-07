import { describe, expect, it } from 'vitest'
import type { ReviewLog } from '../models'
import { MS_PER_DAY } from './index'
import {
  DEFAULT_FSRS_WEIGHTS,
  buildReviewHistories,
  hasDelayedReview,
} from './optimizer'

function log(
  id: string,
  cardId: string,
  reviewedAt: number,
  grade: ReviewLog['grade'],
): ReviewLog {
  return { id, deckId: 'd1', cardId, reviewedAt, grade }
}

describe('buildReviewHistories', () => {
  it('groups by card, orders events, and maps grades to ratings', () => {
    const histories = buildReviewHistories([
      log('3', 'b', 30, 'easy'),
      log('2', 'a', 20, 'hard'),
      log('1', 'a', 10, 'again'),
      log('4', 'a', 40, 'good'),
    ])
    expect(histories).toEqual([
      { reviews: [
        { reviewedAt: 10, rating: 1 },
        { reviewedAt: 20, rating: 2 },
        { reviewedAt: 40, rating: 3 },
      ] },
      { reviews: [{ reviewedAt: 30, rating: 4 }] },
    ])
  })
})

describe('hasDelayedReview', () => {
  it('requires consecutive reviews at least one whole day apart', () => {
    expect(hasDelayedReview(buildReviewHistories([
      log('1', 'a', 0, 'good'),
      log('2', 'a', MS_PER_DAY, 'good'),
    ]))).toBe(true)
    expect(hasDelayedReview(buildReviewHistories([
      log('1', 'a', 0, 'good'),
      log('2', 'a', MS_PER_DAY - 1, 'good'),
      log('3', 'a', 2 * MS_PER_DAY - 2, 'good'),
    ]))).toBe(false)
  })

  it('does not combine reviews from different cards', () => {
    expect(hasDelayedReview(buildReviewHistories([
      log('1', 'a', 0, 'good'),
      log('2', 'b', 2 * MS_PER_DAY, 'good'),
    ]))).toBe(false)
  })
})

it('keeps a complete copy of the pinned FSRS-6 defaults for browser tests', () => {
  expect(DEFAULT_FSRS_WEIGHTS).toHaveLength(21)
  expect(DEFAULT_FSRS_WEIGHTS.every(Number.isFinite)).toBe(true)
})
