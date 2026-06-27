import type { Grade, SchedulingState } from '../models'

/**
 * A spaced-repetition scheduling algorithm.
 *
 * Implementations are pure: given the current state, a grade, and the current
 * time, they return the next state. This is the seam that lets us swap in
 * another algorithm later without touching the rest of the app.
 */
export interface Scheduler {
  /** Scheduling state for a brand-new card (immediately due). */
  initial(now: number): SchedulingState
  /** Compute the next scheduling state after grading a review. */
  next(state: SchedulingState, grade: Grade, now: number): SchedulingState
}
