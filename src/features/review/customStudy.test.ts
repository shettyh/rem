import { describe, expect, it } from 'vitest'
import type { Card, FSRSState } from '../../domain/models'
import { MS_PER_DAY } from '../../domain/scheduler'
import {
  CUSTOM_STUDY_PRESETS,
  parseCustomStudyRequest,
  selectCustomStudyCards,
} from './customStudy'

const NOW = 10 * MS_PER_DAY

function scheduling(state: number, due = NOW): FSRSState {
  return {
    kind: 'fsrs', stability: state === 0 ? 0 : 5, difficulty: 5,
    reps: state === 0 ? 0 : 2, lapses: 0, state, step: 0, lastReview: null, due,
  }
}

function card(
  id: string,
  state: number,
  options: Partial<Pick<Card, 'createdAt' | 'suspended' | 'lastAgainAt'>> & { due?: number } = {},
): Card {
  return {
    id,
    deckId: 'd1',
    front: id,
    back: id,
    createdAt: options.createdAt ?? 1,
    updatedAt: 1,
    tags: [],
    suspended: options.suspended ?? false,
    lastAgainAt: options.lastAgainAt ?? null,
    scheduling: scheduling(state, options.due),
  }
}

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

describe('selectCustomStudyCards', () => {
  it('selects Review cards due inside the study-ahead window, soonest first', () => {
    const cards = [
      card('later', 2, { due: NOW + 2 * MS_PER_DAY }),
      card('due-now', 2, { due: NOW }),
      card('tomorrow', 2, { due: NOW + MS_PER_DAY }),
      card('new', 0, { due: NOW + MS_PER_DAY }),
      card('too-late', 2, { due: NOW + 3 * MS_PER_DAY }),
    ]
    const selected = selectCustomStudyCards(cards, { mode: 'study-ahead', amount: 2 }, NOW, {
      insertionOrder: 'sequential', normalNewSlots: 0,
    })
    expect(selected.map((c) => c.id)).toEqual(['tomorrow', 'later'])
  })

  it('takes additional due New cards after the normal allowance', () => {
    const cards = [
      card('n3', 0, { createdAt: 3 }),
      card('n1', 0, { createdAt: 1 }),
      card('n4', 0, { createdAt: 4 }),
      card('n2', 0, { createdAt: 2 }),
    ]
    const selected = selectCustomStudyCards(cards, { mode: 'increase-new', amount: 2 }, NOW, {
      insertionOrder: 'sequential', normalNewSlots: 1,
    })
    expect(selected.map((c) => c.id)).toEqual(['n2', 'n3'])
  })

  it('honours random insertion order before skipping normal new slots', () => {
    const cards = [card('n1', 0), card('n2', 0), card('n3', 0)]
    const selected = selectCustomStudyCards(cards, { mode: 'increase-new', amount: 2 }, NOW, {
      insertionOrder: 'random', normalNewSlots: 1, rng: () => 0,
    })
    expect(selected.map((c) => c.id)).toEqual(['n3', 'n1'])
  })

  it('selects cards answered Again inside the recent-day window', () => {
    const cards = [
      card('recent', 2, { lastAgainAt: NOW - 1 }),
      card('boundary', 1, { lastAgainAt: NOW - 2 * MS_PER_DAY }),
      card('old', 2, { lastAgainAt: NOW - 2 * MS_PER_DAY - 1 }),
      card('never', 2),
    ]
    const selected = selectCustomStudyCards(cards, { mode: 'review-forgotten', amount: 2 }, NOW, {
      insertionOrder: 'sequential', normalNewSlots: 0,
    })
    expect(selected.map((c) => c.id)).toEqual(['recent', 'boundary'])
  })

  it('previews only still-New cards created inside the recent-day window', () => {
    const cards = [
      card('newest', 0, { createdAt: NOW }),
      card('boundary', 0, { createdAt: NOW - MS_PER_DAY }),
      card('old', 0, { createdAt: NOW - MS_PER_DAY - 1 }),
      card('reviewed', 2, { createdAt: NOW }),
    ]
    const selected = selectCustomStudyCards(cards, { mode: 'preview-new', amount: 1 }, NOW, {
      insertionOrder: 'sequential', normalNewSlots: 0,
    })
    expect(selected.map((c) => c.id)).toEqual(['newest', 'boundary'])
  })

  it('excludes suspended cards from every mode', () => {
    const cards = [card('s', 2, {
      suspended: true,
      due: NOW + MS_PER_DAY,
      createdAt: NOW,
      lastAgainAt: NOW,
    })]
    for (const preset of CUSTOM_STUDY_PRESETS) {
      expect(selectCustomStudyCards(cards, { mode: preset.mode, amount: 2 }, NOW, {
        insertionOrder: 'sequential', normalNewSlots: 0,
      })).toEqual([])
    }
  })
})
