import type { DeckSettings, FSRSState, Grade } from '../models'
import type { DeckFsrsParams } from './Scheduler'
import { getScheduler, MS_PER_DAY } from './index'
import { parseStepsMs } from './steps'

/** Per-deck FSRS params from the deck's settings. */
export function settingsToParams(s: DeckSettings): DeckFsrsParams {
  return {
    desiredRetention: s.desiredRetention,
    maximumInterval: s.maximumInterval,
    weights: s.fsrsWeights,
  }
}

/** A pure learning/relearning step transition: keep memory, move state/step/due. */
function stepTo(base: FSRSState, state: number, step: number, dueMs: number, now: number): FSRSState {
  return { ...base, state, step, due: now + dueMs, lastReview: now }
}

/**
 * Anki-standard Hard delay on a learning/relearning step:
 * - On step 0 with ≥2 steps: average of step 0 and step 1 (e.g. `1m 10m` → `5.5m`).
 * - On step 0 with 1 step: 1.5× step 0, capped at `+1d` (e.g. `10m` → `15m`).
 * - On later steps: repeat the current step.
 */
function hardStepDelay(steps: number[], index: number): number {
  if (index === 0) {
    if (steps.length >= 2) return Math.floor((steps[0] + steps[1]) / 2)
    return steps[0] + Math.min(Math.floor(steps[0] / 2), MS_PER_DAY)
  }
  return steps[index]
}

/** Clamp an FSRS due date to [minDays, maxDays] whole days from now (floor 1d, matching rem-core). */
function clampDays(state: FSRSState, now: number, minDays: number, maxDays: number): FSRSState {
  const max = Math.max(1, Math.floor(Number.isFinite(maxDays) ? maxDays : 1))
  const min = Math.min(max, Math.max(1, Math.floor(Number.isFinite(minDays) ? minDays : 1)))
  const days = Math.min(Math.max(Math.round((state.due - now) / MS_PER_DAY), min), max)
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
      hard: stepTo(scheduling, 1, li, hardStepDelay(L, li), now),
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
      hard: stepTo(scheduling, 3, ri, hardStepDelay(R, ri), now),
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
