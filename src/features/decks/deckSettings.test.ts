import { describe, it, expect } from 'vitest'
import { parseSteps } from '../../domain/scheduler/steps'

describe('parseSteps', () => {
  it('splits a space-separated steps string into tokens', () => {
    expect(parseSteps('1m 10m 1d')).toEqual(['1m', '10m', '1d'])
  })
  it('collapses extra whitespace and drops empties', () => {
    expect(parseSteps('  10m   1d ')).toEqual(['10m', '1d'])
  })
  it('returns [] for a blank string', () => {
    expect(parseSteps('   ')).toEqual([])
  })
})
