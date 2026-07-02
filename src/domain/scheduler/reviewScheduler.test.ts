import { describe, it, expect } from 'vitest'
import { DEFAULT_DECK_SETTINGS } from '../models'
import { settingsToParams, nextStates } from './reviewScheduler'
import type { FSRSState } from '../models'

describe('settingsToParams', () => {
  it('maps deck settings to FSRS params with null weights', () => {
    const s = { ...DEFAULT_DECK_SETTINGS, desiredRetention: 0.85, maximumInterval: 1000 }
    expect(settingsToParams(s)).toEqual({ desiredRetention: 0.85, maximumInterval: 1000, weights: null })
  })
})

const DAY = 86_400_000
const S = { ...DEFAULT_DECK_SETTINGS, learnSteps: '1m 10m', relearnSteps: '10m', minimumInterval: 3, maximumInterval: 36500 }
function newCard(now: number): FSRSState {
  return { kind: 'fsrs', stability: 0, difficulty: 0, reps: 0, lapses: 0, state: 0, step: 0, lastReview: null, due: now }
}

describe('nextStates — learning', () => {
  it('new card: again → step 0 in 1m, good → step 1 in 10m (no FSRS, reps stays 0)', async () => {
    const now = 1_000_000
    const ns = await nextStates(newCard(now), S, now)
    expect(ns.again).toMatchObject({ state: 1, step: 0, due: now + 60_000, reps: 0 })
    expect(ns.good).toMatchObject({ state: 1, step: 1, due: now + 600_000, reps: 0 })
  })
  it('hard repeats the current step', async () => {
    const now = 1_000_000
    const ns = await nextStates({ ...newCard(now), state: 1, step: 1 }, S, now)
    expect(ns.hard).toMatchObject({ state: 1, step: 1, due: now + 600_000 })
  })
  it('good on the last step graduates via FSRS (state 2, reps 1)', async () => {
    const now = 1_000_000
    const ns = await nextStates({ ...newCard(now), state: 1, step: 1 }, S, now)
    expect(ns.good).toMatchObject({ state: 2, step: 0, reps: 1 })
    expect(ns.good.due).toBeGreaterThan(now + DAY) // FSRS interval, not a step
  })
  it('easy graduates immediately via FSRS', async () => {
    const now = 1_000_000
    const ns = await nextStates(newCard(now), S, now)
    expect(ns.easy).toMatchObject({ state: 2, step: 0, reps: 1 })
  })
  it('empty learn steps → straight to FSRS review on good', async () => {
    const now = 1_000_000
    const ns = await nextStates(newCard(now), { ...S, learnSteps: '' }, now)
    expect(ns.good).toMatchObject({ state: 2, step: 0, reps: 1 })
  })
  it('new card with runtime-undefined step (pre-#3a ingestion) does not go NaN or skip learning steps', async () => {
    const now = 1_000_000
    const card = {
      kind: 'fsrs', stability: 0, difficulty: 0, reps: 0, lapses: 0, state: 0, lastReview: null, due: now,
    } as unknown as FSRSState
    const ns = await nextStates(card, S, now)
    expect(ns.again.due).toBe(now + 60_000)
    expect(ns.good.due).toBe(now + 600_000)
  })
})

describe('nextStates — review lapse + relearning', () => {
  const review: FSRSState = { kind: 'fsrs', stability: 10, difficulty: 5, reps: 4, lapses: 0, state: 2, step: 0, lastReview: 0, due: 0 }
  it('again on a review card records a lapse and enters relearning', async () => {
    const now = 5_000_000
    const ns = await nextStates(review, S, now)
    expect(ns.again).toMatchObject({ state: 3, step: 0, due: now + 600_000, lapses: 1 })
  })
  it('good/easy stay in review (FSRS long-term), unclamped', async () => {
    const now = 5_000_000
    const ns = await nextStates(review, { ...S, minimumInterval: 5 }, now)
    expect(ns.good).toMatchObject({ state: 2, step: 0 })
    expect(ns.easy).toMatchObject({ state: 2, step: 0 })
    expect(ns.good.due).toBe(now + 3 * DAY) // fake good = +3d, NOT clamped up to minimumInterval 5
    expect(ns.easy.due).toBe(now + 7 * DAY) // fake easy = +7d, unaffected by minimumInterval
  })
  it('relearning good on the last step graduates, clamped to minimumInterval', async () => {
    const now = 5_000_000
    const relearn: FSRSState = { ...review, state: 3, step: 0 }
    const ns = await nextStates(relearn, S, now) // relearnSteps '10m' → single step, so good graduates
    expect(ns.good).toMatchObject({ state: 2, step: 0 })
    expect(ns.good.due).toBe(now + 3 * DAY) // fake good = +3d, minimumInterval 3 → clamp keeps 3d
  })
  it('relearning good graduation respects a higher minimumInterval', async () => {
    const now = 5_000_000
    const relearn: FSRSState = { ...review, state: 3, step: 0 }
    const ns = await nextStates(relearn, { ...S, minimumInterval: 5 }, now)
    expect(ns.good.due).toBe(now + 5 * DAY) // fake good = +3d, clamped up to 5d
  })
  it('relearning card with empty relearnSteps: every outcome is a clamped FSRS state', async () => {
    const now = 5_000_000
    const relearn: FSRSState = { ...review, state: 3, step: 0 }
    const ns = await nextStates(relearn, { ...S, relearnSteps: '', minimumInterval: 3 }, now)
    expect(ns.again.due).toBe(now + 3 * DAY) // fake again = +0d, clamped up to min 3
    expect(ns.good.due).toBe(now + 3 * DAY)
    expect(ns.easy.due).toBe(now + 7 * DAY)
    expect(ns.again.state).toBe(2)
  })
  it('review card with empty relearnSteps: again stays in review as clamped FSRS again', async () => {
    const now = 5_000_000
    const ns = await nextStates(review, { ...S, relearnSteps: '' }, now)
    expect(ns.again.state).toBe(2)
    expect(ns.again.due).toBe(now + 3 * DAY) // fake again = +0d, clamped up to min 3
    expect(ns.again.lapses).toBe(1)
  })
})
