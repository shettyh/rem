import { isTauri } from '@tauri-apps/api/core'
import type { Scheduler } from './Scheduler'
import { TauriFsrsScheduler } from './tauriFsrs'
import { FakeScheduler } from './fakeScheduler'

export type { Scheduler } from './Scheduler'

export const MS_PER_DAY = 86_400_000

const tauriScheduler = new TauriFsrsScheduler()
const fakeScheduler = new FakeScheduler()

/** The active scheduler: real fsrs-rs over Tauri in the app, deterministic fake
 *  in tests / non-Tauri dev. */
export function getScheduler(): Scheduler {
  return isTauri() ? tauriScheduler : fakeScheduler
}
