import { describe, it, expect } from 'vitest'
import { cardStatus } from './DeckDetailPage'
import { MS_PER_DAY } from '../../domain/scheduler'

const now = 1_000_000_000_000

describe('cardStatus', () => {
  it('marks unreviewed cards new', () => {
    expect(cardStatus({ kind: 'sm2', repetitions: 0, intervalDays: 0, easeFactor: 2.5, due: now }, now).kind).toBe('new')
  })
  it('marks past-due reviewed cards due', () => {
    expect(cardStatus({ kind: 'sm2', repetitions: 2, intervalDays: 1, easeFactor: 2.5, due: now - 1 }, now).kind).toBe('due')
  })
  it('labels future cards with a relative interval', () => {
    const s = cardStatus({ kind: 'sm2', repetitions: 2, intervalDays: 3, easeFactor: 2.5, due: now + 3 * MS_PER_DAY }, now)
    expect(s.kind).toBe('scheduled')
    expect(s.label).toBe('3d')
  })

  it('marks an unreviewed FSRS card new', () => {
    const s = { kind: 'fsrs' as const, stability: 0, difficulty: 0, reps: 0, lapses: 0, state: 0, lastReview: null, due: now }
    expect(cardStatus(s, now).kind).toBe('new')
  })

  it('marks a reviewed FSRS card due when past its due date', () => {
    const s = { kind: 'fsrs' as const, stability: 5, difficulty: 5, reps: 1, lapses: 0, state: 2, lastReview: now - MS_PER_DAY, due: now - 1 }
    expect(cardStatus(s, now).kind).toBe('due')
  })
})
