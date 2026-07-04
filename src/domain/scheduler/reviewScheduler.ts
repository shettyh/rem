import type { DeckSettings, FSRSState, Grade } from '../models'
import type { DeckFsrsParams } from './Scheduler'
import { getScheduler, MS_PER_DAY } from './index'
import { parseStepsMs } from './steps'

/** Per-deck FSRS params from the deck's settings. Weights stay null until #5. */
export function settingsToParams(s: DeckSettings): DeckFsrsParams {
  return { desiredRetention: s.desiredRetention, maximumInterval: s.maximumInterval, weights: null }
}

/** A pure learning/relearning step transition: keep memory, move state/step/due. */
function stepTo(base: FSRSState, state: number, step: number, dueMs: number, now: number): FSRSState {
  return { ...base, state, step, due: now + dueMs, lastReview: now }
}

/** Clamp an FSRS due date to [minDays, maxDays] whole days from now. */
function clampDays(state: FSRSState, now: number, minDays: number, maxDays: number): FSRSState {
  const days = Math.min(Math.max(Math.round((state.due - now) / MS_PER_DAY), minDays), maxDays)
  return { ...state, due: now + days * MS_PER_DAY }
}

/**
 * All four grade outcomes for a card, honouring classic learning/relearning
 * steps in TS and delegating long-term intervals to FSRS (via the seam).
 */
export async function nextStates(
  scheduling: FSRSState,
  settings: DeckSettings,
  now: number,
): Promise<Record<Grade, FSRSState>> {
  const fsrs = await getScheduler().previewNextStates(scheduling, settingsToParams(settings), now)
  const i = scheduling.step ?? 0
  const min = settings.minimumInterval
  const max = settings.maximumInterval

  // New / Learning
  if (scheduling.state === 0 || scheduling.state === 1) {
    const L = parseStepsMs(settings.learnSteps)
    if (L.length === 0) return fsrs
    const li = Math.min(i, L.length - 1)
    return {
      again: stepTo(scheduling, 1, 0, L[0], now),
      hard: stepTo(scheduling, 1, li, L[li], now),
      good: i + 1 < L.length ? stepTo(scheduling, 1, i + 1, L[i + 1], now) : fsrs.good,
      easy: fsrs.easy,
    }
  }

  // Relearning
  if (scheduling.state === 3) {
    const R = parseStepsMs(settings.relearnSteps)
    if (R.length === 0) {
      return {
        again: clampDays(fsrs.again, now, min, max),
        hard: clampDays(fsrs.hard, now, min, max),
        good: clampDays(fsrs.good, now, min, max),
        easy: clampDays(fsrs.easy, now, min, max),
      }
    }
    const ri = Math.min(i, R.length - 1)
    return {
      again: stepTo(scheduling, 3, 0, R[0], now),
      hard: stepTo(scheduling, 3, ri, R[ri], now),
      good: i + 1 < R.length ? stepTo(scheduling, 3, i + 1, R[i + 1], now) : clampDays(fsrs.good, now, min, max),
      easy: clampDays(fsrs.easy, now, min, max),
    }
  }

  // Review (state 2)
  const R = parseStepsMs(settings.relearnSteps)
  const againBase = fsrs.again // memory updated + lapses++ by the FSRS seam
  const again = R.length === 0
    ? clampDays(againBase, now, min, max)
    : stepTo(againBase, 3, 0, R[0], now)
  return { again, hard: fsrs.hard, good: fsrs.good, easy: fsrs.easy }
}
