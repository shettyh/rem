import { describe, it, expect } from 'vitest'
import { SM2Scheduler, MS_PER_DAY } from './sm2'

const now = 1_700_000_000_000 // fixed reference time
const scheduler = new SM2Scheduler()

describe('SM2Scheduler.initial', () => {
  it('starts a new card immediately due, ease 2.5, no progress', () => {
    expect(scheduler.initial(now)).toEqual({
      repetitions: 0,
      intervalDays: 0,
      easeFactor: 2.5,
      due: now,
    })
  })
})

describe('SM2Scheduler.next — successful progression', () => {
  it('schedules a new card 1 day out on the first "good"', () => {
    const s = scheduler.next(scheduler.initial(now), 'good', now)
    expect(s.repetitions).toBe(1)
    expect(s.intervalDays).toBe(1)
    expect(s.due).toBe(now + 1 * MS_PER_DAY)
  })

  it('schedules the second "good" 6 days out', () => {
    const first = scheduler.next(scheduler.initial(now), 'good', now)
    const second = scheduler.next(first, 'good', now)
    expect(second.repetitions).toBe(2)
    expect(second.intervalDays).toBe(6)
    expect(second.due).toBe(now + 6 * MS_PER_DAY)
  })

  it('multiplies interval by ease factor from the third "good" on', () => {
    let s = scheduler.initial(now)
    s = scheduler.next(s, 'good', now) // -> 1 day
    s = scheduler.next(s, 'good', now) // -> 6 days
    s = scheduler.next(s, 'good', now) // -> round(6 * 2.5) = 15 days
    expect(s.repetitions).toBe(3)
    expect(s.intervalDays).toBe(15)
  })
})

describe('SM2Scheduler.next — ease factor adjustments', () => {
  it('leaves ease unchanged on "good" (q=4)', () => {
    const s = scheduler.next(scheduler.initial(now), 'good', now)
    expect(s.easeFactor).toBeCloseTo(2.5)
  })

  it('raises ease on "easy" (q=5)', () => {
    const s = scheduler.next(scheduler.initial(now), 'easy', now)
    expect(s.easeFactor).toBeCloseTo(2.6)
  })

  it('lowers ease on "hard" (q=3)', () => {
    const s = scheduler.next(scheduler.initial(now), 'hard', now)
    expect(s.easeFactor).toBeCloseTo(2.36)
  })

  it('never lets ease drop below 1.3', () => {
    let s = scheduler.initial(now)
    for (let i = 0; i < 10; i++) s = scheduler.next(s, 'again', now)
    expect(s.easeFactor).toBe(1.3)
  })
})

describe('SM2Scheduler.next — lapses', () => {
  it('resets repetitions and schedules 1 day out on "again"', () => {
    let s = scheduler.initial(now)
    s = scheduler.next(s, 'good', now) // reps 1
    s = scheduler.next(s, 'good', now) // reps 2, interval 6
    const lapsed = scheduler.next(s, 'again', now)
    expect(lapsed.repetitions).toBe(0)
    expect(lapsed.intervalDays).toBe(1)
    expect(lapsed.due).toBe(now + 1 * MS_PER_DAY)
  })
})
