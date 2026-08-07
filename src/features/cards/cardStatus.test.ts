import { describe, it, expect } from 'vitest'
import { cardStatus } from './DeckDetailPage'
import { MS_PER_DAY } from '../../domain/scheduler'
import type { SchedulingState } from '../../domain/models'

const now = 1_000_000_000_000
const card = (scheduling: SchedulingState, tags: string[] = [], suspended = false) => ({
  scheduling,
  tags,
  suspended,
})

describe('cardStatus', () => {
  it('marks an unreviewed FSRS card new', () => {
    const s = { kind: 'fsrs' as const, stability: 0, difficulty: 0, reps: 0, lapses: 0, state: 0, step: 0, lastReview: null, due: now }
    expect(cardStatus(card(s), now).kind).toBe('new')
  })

  it('marks a reviewed FSRS card due when past its due date', () => {
    const s = { kind: 'fsrs' as const, stability: 5, difficulty: 5, reps: 1, lapses: 0, state: 2, step: 0, lastReview: now - MS_PER_DAY, due: now - 1 }
    expect(cardStatus(card(s), now).kind).toBe('due')
  })

  it('labels a future-due FSRS card with a relative interval', () => {
    const s = cardStatus(
      card({ kind: 'fsrs' as const, stability: 5, difficulty: 5, reps: 1, lapses: 0, state: 2, step: 0, lastReview: now, due: now + 3 * MS_PER_DAY }),
      now,
    )
    expect(s.kind).toBe('scheduled')
    expect(s.label).toBe('3d')
  })

  it('shows leech metadata ahead of scheduling status', () => {
    const due = { kind: 'fsrs' as const, stability: 5, difficulty: 5, reps: 1, lapses: 8, state: 2, step: 0, lastReview: now, due: now - 1 }
    expect(cardStatus(card(due, ['leech']), now)).toEqual({ kind: 'leech', label: 'leech' })
    expect(cardStatus(card(due, ['leech'], true), now)).toEqual({ kind: 'suspended', label: 'suspended' })
  })
})
