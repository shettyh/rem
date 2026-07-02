import { invoke } from '@tauri-apps/api/core'
import type { Grade, FSRSState, SchedulingState } from '../models'
import type { Scheduler } from './Scheduler'

export interface DeckFsrsParams {
  desiredRetention: number
  maximumInterval: number
  weights: number[] | null
}

/** Defaults for sub-project #2. Sub-projects #1/#3 replace this with the deck's
 *  stored settings — the command already accepts a DeckFsrsParams argument. */
export const DEFAULT_DECK_FSRS_PARAMS: DeckFsrsParams = {
  desiredRetention: 0.9,
  maximumInterval: 36500,
  weights: null,
}

interface FsrsStateDto {
  stability: number
  difficulty: number
  reps: number
  lapses: number
  state: number
  lastReview: number | null
  due: number
}

interface NextStatesDto {
  again: FsrsStateDto
  hard: FsrsStateDto
  good: FsrsStateDto
  easy: FsrsStateDto
}

function toState(dto: FsrsStateDto): FSRSState {
  return { kind: 'fsrs', step: 0, ...dto }
}

/** Pure DTO → domain mapping (unit-tested without Tauri). */
export function mapNextStates(dto: NextStatesDto): Record<Grade, SchedulingState> {
  return { again: toState(dto.again), hard: toState(dto.hard), good: toState(dto.good), easy: toState(dto.easy) }
}

export class TauriFsrsScheduler implements Scheduler {
  initial(now: number): SchedulingState {
    return { kind: 'fsrs', stability: 0, difficulty: 0, reps: 0, lapses: 0, state: 0, step: 0, lastReview: null, due: now }
  }

  async previewNextStates(state: SchedulingState, now: number): Promise<Record<Grade, SchedulingState>> {
    const dto = await invoke<NextStatesDto>('fsrs_next_states', { state, now, params: DEFAULT_DECK_FSRS_PARAMS })
    return mapNextStates(dto)
  }
}
