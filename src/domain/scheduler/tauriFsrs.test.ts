import { describe, it, expect } from 'vitest'
import { mapNextStates } from './tauriFsrs'

describe('mapNextStates', () => {
  it('tags each DTO branch with kind: fsrs and preserves fields', () => {
    const branch = { stability: 3.2, difficulty: 5.1, reps: 2, lapses: 1, state: 2, lastReview: 100, due: 200 }
    const dto = { again: branch, hard: branch, good: branch, easy: branch }
    const out = mapNextStates(dto)
    expect(out.good).toEqual({ kind: 'fsrs', ...branch })
    expect(out.again.kind).toBe('fsrs')
  })
})
