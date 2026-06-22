import { describe, it, expect } from 'vitest'
import { FSRSScheduler } from './fsrs'

const now = 1_700_000_000_000
const scheduler = new FSRSScheduler()

describe('FSRSScheduler.initial', () => {
  it('starts a new card immediately due, unreviewed, kind fsrs', () => {
    const s = scheduler.initial(now)
    expect(s.kind).toBe('fsrs')
    expect(s.due).toBe(now)
    if (s.kind !== 'fsrs') throw new Error('expected fsrs')
    expect(s.reps).toBe(0)
    expect(s.lapses).toBe(0)
    expect(s.state).toBe(0)
    expect(s.lastReview).toBeNull()
  })
})

describe('FSRSScheduler.next', () => {
  it('schedules a reviewed card into the future and records the review time', () => {
    const s = scheduler.next(scheduler.initial(now), 'good', now)
    expect(s.kind).toBe('fsrs')
    if (s.kind !== 'fsrs') throw new Error('expected fsrs')
    expect(s.due).toBeGreaterThan(now)
    expect(s.reps).toBe(1)
    expect(s.lastReview).toBe(now)
  })

  it('schedules "easy" further out than "good"', () => {
    const init = scheduler.initial(now)
    const good = scheduler.next(init, 'good', now)
    const easy = scheduler.next(init, 'easy', now)
    expect(easy.due).toBeGreaterThan(good.due)
  })

  it('counts a lapse and reschedules sooner than "good" when failing a learned card', () => {
    let s = scheduler.next(scheduler.initial(now), 'good', now)
    const reviewedAt = s.due
    const again = scheduler.next(s, 'again', reviewedAt)
    const good = scheduler.next(s, 'good', reviewedAt)
    if (again.kind !== 'fsrs' || good.kind !== 'fsrs') throw new Error('expected fsrs')
    expect(again.lapses).toBe(1)
    expect(again.due).toBeLessThan(good.due)
  })

  it('is deterministic (fuzz disabled)', () => {
    const a = scheduler.next(scheduler.initial(now), 'good', now)
    const b = scheduler.next(scheduler.initial(now), 'good', now)
    expect(a).toEqual(b)
  })
})
