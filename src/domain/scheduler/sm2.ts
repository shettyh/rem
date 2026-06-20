import type { Grade, SchedulingState } from '../models'
import type { Scheduler } from './Scheduler'

export const MS_PER_DAY = 86_400_000

const INITIAL_EASE = 2.5
const MIN_EASE = 1.3

/** Maps the 4-button UI grades onto SM-2 quality scores (0–5). */
const QUALITY: Record<Grade, number> = {
  again: 0,
  hard: 3,
  good: 4,
  easy: 5,
}

/**
 * The classic SuperMemo-2 algorithm.
 *
 * On a failed recall (quality < 3 → "again") the card lapses: repetitions reset
 * and it is shown again in a day. On success the interval grows (1 → 6 → ×ease).
 * The ease factor is nudged after every review and never falls below 1.3.
 */
export class SM2Scheduler implements Scheduler {
  initial(now: number): SchedulingState {
    return { repetitions: 0, intervalDays: 0, easeFactor: INITIAL_EASE, due: now }
  }

  next(state: SchedulingState, grade: Grade, now: number): SchedulingState {
    const q = QUALITY[grade]
    const easeFactor = Math.max(
      MIN_EASE,
      state.easeFactor + (0.1 - (5 - q) * (0.08 + (5 - q) * 0.02)),
    )

    let repetitions: number
    let intervalDays: number
    if (q < 3) {
      repetitions = 0
      intervalDays = 1
    } else if (state.repetitions === 0) {
      repetitions = 1
      intervalDays = 1
    } else if (state.repetitions === 1) {
      repetitions = 2
      intervalDays = 6
    } else {
      repetitions = state.repetitions + 1
      intervalDays = Math.round(state.intervalDays * easeFactor)
    }

    return { repetitions, intervalDays, easeFactor, due: now + intervalDays * MS_PER_DAY }
  }
}
