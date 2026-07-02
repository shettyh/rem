import { describe, it, expect } from 'vitest'
import { parseSteps, parseStepsMs } from './steps'

describe('parseSteps', () => {
  it('splits on whitespace', () => {
    expect(parseSteps('1m 10m 1d')).toEqual(['1m', '10m', '1d'])
  })
  it('trims and drops blanks', () => {
    expect(parseSteps('  10m   1d ')).toEqual(['10m', '1d'])
  })
  it('empty string → []', () => {
    expect(parseSteps('   ')).toEqual([])
  })
})

describe('parseStepsMs', () => {
  it('parses s/m/h/d units', () => {
    expect(parseStepsMs('30s 10m 1h 1d')).toEqual([30_000, 600_000, 3_600_000, 86_400_000])
  })
  it('treats a bare integer as minutes', () => {
    expect(parseStepsMs('1 10')).toEqual([60_000, 600_000])
  })
  it('drops unparseable tokens', () => {
    expect(parseStepsMs('10m foo 1d')).toEqual([600_000, 86_400_000])
  })
  it('empty string → []', () => {
    expect(parseStepsMs('')).toEqual([])
  })
})
