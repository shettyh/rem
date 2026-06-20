import { SM2Scheduler } from './sm2'

export type { Scheduler } from './Scheduler'
export { MS_PER_DAY } from './sm2'

/** The app-wide scheduling algorithm. Swap this line to change algorithms. */
export const scheduler = new SM2Scheduler()
