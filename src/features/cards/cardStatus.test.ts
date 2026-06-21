import { describe, it, expect } from 'vitest'
import { cardStatus } from './DeckDetailPage'
import { MS_PER_DAY } from '../../domain/scheduler'

const now = 1_000_000_000_000

describe('cardStatus', () => {
  it('marks unreviewed cards new', () => {
    expect(cardStatus({ repetitions: 0, intervalDays: 0, easeFactor: 2.5, due: now }, now).kind).toBe('new')
  })
  it('marks past-due reviewed cards due', () => {
    expect(cardStatus({ repetitions: 2, intervalDays: 1, easeFactor: 2.5, due: now - 1 }, now).kind).toBe('due')
  })
  it('labels future cards with a relative interval', () => {
    const s = cardStatus({ repetitions: 2, intervalDays: 3, easeFactor: 2.5, due: now + 3 * MS_PER_DAY }, now)
    expect(s.kind).toBe('scheduled')
    expect(s.label).toBe('3d')
  })
})
