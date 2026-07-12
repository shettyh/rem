import { describe, it, expect } from 'vitest'
import { localDay } from './day'

describe('localDay', () => {
  it('formats a timestamp as local YYYY-MM-DD', () => {
    expect(localDay(new Date(2026, 6, 12, 9, 30).getTime())).toBe('2026-07-12')
  })

  it('rolls over at local midnight', () => {
    expect(localDay(new Date(2026, 6, 12, 23, 59).getTime())).toBe('2026-07-12')
    expect(localDay(new Date(2026, 6, 13, 0, 1).getTime())).toBe('2026-07-13')
  })
})
