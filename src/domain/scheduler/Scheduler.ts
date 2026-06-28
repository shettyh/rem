import type { Grade, SchedulingState } from '../models'

/**
 * A spaced-repetition scheduling algorithm.
 *
 * `initial` is pure and synchronous (a brand-new card needs no algorithm).
 * `previewNextStates` returns all four grade outcomes at once — the real
 * implementation crosses into Rust, so it is async.
 */
export interface Scheduler {
  /** Scheduling state for a brand-new card (immediately due). */
  initial(now: number): SchedulingState
  /** All four grade outcomes for the next review. */
  previewNextStates(state: SchedulingState, now: number): Promise<Record<Grade, SchedulingState>>
}
