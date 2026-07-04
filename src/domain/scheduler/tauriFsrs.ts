import { invoke } from '@tauri-apps/api/core'
import type { Grade, FSRSState, SchedulingState } from '../models'
import type { DeckFsrsParams, Scheduler } from './Scheduler'

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

  async previewNextStates(
    state: SchedulingState,
    params: DeckFsrsParams,
    now: number,
  ): Promise<Record<Grade, SchedulingState>> {
    const dto = await invoke<NextStatesDto>('fsrs_next_states', { state, now, params })
    return mapNextStates(dto)
  }
}
