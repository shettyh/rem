import { describe, expect, it } from 'vitest'
import { CUSTOM_STUDY_PRESETS, parseCustomStudyRequest } from './customStudy'

describe('parseCustomStudyRequest', () => {
  it('parses a valid mode and positive integer amount', () => {
    expect(parseCustomStudyRequest(new URLSearchParams('custom=study-ahead&amount=7')))
      .toEqual({ mode: 'study-ahead', amount: 7 })
  })

  it('uses the mode default when amount is missing and rejects unknown modes', () => {
    expect(parseCustomStudyRequest(new URLSearchParams('custom=increase-new')))
      .toEqual({ mode: 'increase-new', amount: 10 })
    expect(parseCustomStudyRequest(new URLSearchParams('custom=other&amount=2'))).toBeNull()
  })

  it('clamps the amount to the UI range', () => {
    expect(parseCustomStudyRequest(new URLSearchParams('custom=preview-new&amount=0'))?.amount).toBe(1)
    expect(parseCustomStudyRequest(new URLSearchParams('custom=preview-new&amount=5000'))?.amount).toBe(999)
  })

  it('defines day and card units for the four presets', () => {
    expect(CUSTOM_STUDY_PRESETS.map((p) => [p.mode, p.unit])).toEqual([
      ['study-ahead', 'days'],
      ['increase-new', 'cards'],
      ['review-forgotten', 'days'],
      ['preview-new', 'days'],
    ])
  })
})
