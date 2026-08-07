import { invoke, isTauri } from '@tauri-apps/api/core'
import type { ReviewLog } from '../models'
import { MS_PER_DAY } from './index'

export interface FsrsReviewHistory {
  reviews: Array<{ reviewedAt: number; rating: number }>
}

export interface FsrsOptimizer {
  optimize(histories: FsrsReviewHistory[], numRelearningSteps: number): Promise<number[]>
}

/** fsrs-rs 6.6.1 FSRS-6 defaults; also the deterministic browser-test result. */
export const DEFAULT_FSRS_WEIGHTS = [
  0.212, 1.2931, 2.3065, 8.2956, 6.4133, 0.8334, 3.0194,
  0.001, 1.8722, 0.1666, 0.796, 1.4835, 0.0614, 0.2629,
  1.6483, 0.6014, 1.8729, 0.5425, 0.0912, 0.0658, 0.1542,
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
