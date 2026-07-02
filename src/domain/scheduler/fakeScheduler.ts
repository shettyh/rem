import type { Grade, FSRSState, SchedulingState } from '../models'
import type { DeckFsrsParams, Scheduler } from './Scheduler'

const MS_PER_DAY = 86_400_000
const OFFSET_DAYS: Record<Grade, number> = { again: 0, hard: 1, good: 3, easy: 7 }

function emptyCard(now: number): FSRSState {
  return { kind: 'fsrs', stability: 0, difficulty: 0, reps: 0, lapses: 0, state: 0, step: 0, lastReview: null, due: now }
}

/** Deterministic, non-FSRS stand-in for tests and non-Tauri dev. Real FSRS math
 *  is in Rust (cargo-tested); this only needs to exercise the wiring. */
export class FakeScheduler implements Scheduler {
  initial(now: number): SchedulingState {
    return emptyCard(now)
  }

  async previewNextStates(
    state: SchedulingState,
    _params: DeckFsrsParams,
    now: number,
  ): Promise<Record<Grade, SchedulingState>> {
    if (state.kind !== 'fsrs') throw new Error('expected fsrs state')
    const make = (g: Grade): FSRSState => ({
      kind: 'fsrs',
      stability: state.stability,
      difficulty: state.difficulty,
      reps: state.reps + 1,
      lapses: state.lapses + (g === 'again' && state.state === 2 ? 1 : 0),
      state: 2,
      step: 0,
      lastReview: now,
      due: now + OFFSET_DAYS[g] * MS_PER_DAY,
    })
    return { again: make('again'), hard: make('hard'), good: make('good'), easy: make('easy') }
  }
}
