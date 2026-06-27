import type { SchedulerKind } from '../models'
import type { Scheduler } from './Scheduler'
import { FSRSScheduler } from './fsrs'

export type { Scheduler } from './Scheduler'

export const MS_PER_DAY = 86_400_000

const SCHEDULERS: Record<SchedulerKind, Scheduler> = {
  fsrs: new FSRSScheduler(),
}

/** Resolve the scheduling algorithm for a given kind. */
export function getScheduler(kind: SchedulerKind): Scheduler {
  return SCHEDULERS[kind]
}
