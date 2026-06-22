import { createEmptyCard, fsrs, generatorParameters, Rating, type Card as FsrsCard, type Grade as FsrsGrade } from 'ts-fsrs'
import type { Grade, FSRSState, SchedulingState } from '../models'
import type { Scheduler } from './Scheduler'

const params = generatorParameters({
  enable_fuzz: false, // deterministic, so scheduling is testable
  enable_short_term: false, // day-granular: skip sub-day learning steps
  request_retention: 0.9,
})

const RATING: Record<Grade, FsrsGrade> = {
  again: Rating.Again,
  hard: Rating.Hard,
  good: Rating.Good,
  easy: Rating.Easy,
}

/** Thin adapter over ts-fsrs, mapping its Date/Card API to our pure
 *  (state, grade, now) numeric-ms contract. Default global weights only. */
export class FSRSScheduler implements Scheduler {
  private readonly f = fsrs(params)

  initial(now: number): SchedulingState {
    return toState(createEmptyCard(new Date(now)))
  }

  next(state: SchedulingState, grade: Grade, now: number): SchedulingState {
    if (state.kind !== 'fsrs') throw new Error('FSRSScheduler received non-FSRS state')
    const { card } = this.f.next(toCard(state), new Date(now), RATING[grade])
    return toState(card)
  }
}

function toCard(s: FSRSState): FsrsCard {
  return {
    due: new Date(s.due),
    stability: s.stability,
    difficulty: s.difficulty,
    elapsed_days: 0, // recomputed by ts-fsrs from last_review + now
    scheduled_days: 0,
    learning_steps: 0,
    reps: s.reps,
    lapses: s.lapses,
    state: s.state,
    last_review: s.lastReview != null ? new Date(s.lastReview) : undefined,
  } as FsrsCard
}

function toState(card: FsrsCard): FSRSState {
  return {
    kind: 'fsrs',
    due: card.due.getTime(),
    stability: card.stability,
    difficulty: card.difficulty,
    reps: card.reps,
    lapses: card.lapses,
    state: card.state,
    lastReview: card.last_review ? card.last_review.getTime() : null,
  }
}
