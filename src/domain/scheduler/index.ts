import type { SchedulerKind } from '../models'
import type { Scheduler } from './Scheduler'
import { SM2Scheduler } from './sm2'
import { FSRSScheduler } from './fsrs'

export type { Scheduler } from './Scheduler'
export { MS_PER_DAY } from './sm2'

const SCHEDULERS: Record<SchedulerKind, Scheduler> = {
  sm2: new SM2Scheduler(),
  fsrs: new FSRSScheduler(),
}

/** Resolve the scheduling algorithm for a given kind. */
export function getScheduler(kind: SchedulerKind): Scheduler {
  return SCHEDULERS[kind]
}
