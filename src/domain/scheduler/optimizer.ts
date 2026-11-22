import { invoke, isTauri } from '@tauri-apps/api/core'
import type { ReviewLog } from '../models'
import { MS_PER_DAY } from './index'

export interface FsrsReviewHistory {
  reviews: Array<{ reviewedAt: number; rating: number }>
}

export interface FsrsOptimizer {
  optimize(histories: FsrsReviewHistory[], numRelearningSteps: number): Promise<number[]>
}

/** fsrs-rs 7.0.0 FSRS-7 defaults; also the deterministic browser-test result. */
export const DEFAULT_FSRS_WEIGHTS = [
  0.1104, 2.2395, 3.9221, 11.7841, 6.1686, 0.6457, 3.6807, 1.9795, 0.0, 1.3826, 0.7024, 0.5999,
  0.8146, 0.6398, 1.0, 1.3207, 0.6707, 3.8668, 0.4416, 0.0934, 1.8631, 0.6162, 1.0869, 0.1567,
  0.0801, 0.2421, 0.9464, 0.1433, 0.7145, 0.0, 0.5667, 0.3734, 0.5333, 0.3048,
] as const

const RATINGS: Record<ReviewLog['grade'], number> = {
  again: 1,
  hard: 2,
  good: 3,
  easy: 4,
}

/** Convert immutable storage events into deterministic per-card optimizer histories. */
export function buildReviewHistories(logs: ReviewLog[]): FsrsReviewHistory[] {
  const byCard = new Map<string, ReviewLog[]>()
  for (const log of logs) {
    const cardLogs = byCard.get(log.cardId) ?? []
    cardLogs.push(log)
    byCard.set(log.cardId, cardLogs)
  }
  return [...byCard.entries()]
    .sort(([a], [b]) => a.localeCompare(b))
    .map(([, cardLogs]) => ({
      reviews: cardLogs
        .slice()
        .sort((a, b) => a.reviewedAt - b.reviewedAt || a.id.localeCompare(b.id))
        .map((log) => ({ reviewedAt: log.reviewedAt, rating: RATINGS[log.grade] })),
    }))
}

/** Whether Rust can build at least one training item with `delta_t > 0`. */
export function hasDelayedReview(histories: FsrsReviewHistory[]): boolean {
  return histories.some((history) => history.reviews.some((review, index) => (
    index > 0 && review.reviewedAt - history.reviews[index - 1].reviewedAt >= MS_PER_DAY
  )))
}

class TauriFsrsOptimizer implements FsrsOptimizer {
  optimize(histories: FsrsReviewHistory[], numRelearningSteps: number): Promise<number[]> {
    return invoke<number[]>('fsrs_optimize', { histories, numRelearningSteps })
  }
}

class FakeFsrsOptimizer implements FsrsOptimizer {
  async optimize(): Promise<number[]> {
    return [...DEFAULT_FSRS_WEIGHTS]
  }
}

const tauriOptimizer = new TauriFsrsOptimizer()
const fakeOptimizer = new FakeFsrsOptimizer()

/** Native optimizer in the app; deterministic seam in browser tests. */
export function getFsrsOptimizer(): FsrsOptimizer {
  return isTauri() ? tauriOptimizer : fakeOptimizer
}
