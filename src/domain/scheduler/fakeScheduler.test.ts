import { describe, it, expect } from 'vitest'
import type { SchedulingState } from '../models'
import { FakeScheduler } from './fakeScheduler'

const now = 1_700_000_000_000
const s = new FakeScheduler()
const PARAMS = { desiredRetention: 0.9, maximumInterval: 36500, weights: null }

describe('FakeScheduler.initial', () => {
  it('makes a new card due now, unreviewed, kind fsrs', () => {
    const c = s.initial(now)
    expect(c).toEqual({ kind: 'fsrs', stability: 0, difficulty: 0, reps: 0, lapses: 0, state: 0, step: 0, lastReview: null, due: now })
  })
})

describe('FakeScheduler.previewNextStates', () => {
  it('returns four ascending future states and bumps reps', async () => {
    const n = await s.previewNextStates(s.initial(now), PARAMS, now)
    expect(n.again.reps).toBe(1)
    expect(n.again.due).toBeLessThan(n.hard.due)
    expect(n.hard.due).toBeLessThan(n.good.due)
    expect(n.good.due).toBeLessThan(n.easy.due)
  })

  it('counts a lapse only when failing a Review-state card', async () => {
    const reviewed: SchedulingState = { ...s.initial(now), reps: 1, state: 2 }
    const fromReview = await s.previewNextStates(reviewed, PARAMS, now)
    expect(fromReview.again.lapses).toBe(1)
    const fromNew = await s.previewNextStates(s.initial(now), PARAMS, now)
    expect(fromNew.again.lapses).toBe(0)
  })

  it('is deterministic', async () => {
    const a = await s.previewNextStates(s.initial(now), PARAMS, now)
    const b = await s.previewNextStates(s.initial(now), PARAMS, now)
    expect(a).toEqual(b)
  })
})
