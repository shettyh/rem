import { describe, it, expect } from 'vitest'
import type { Card, DeckSettings, FSRSState } from '../../domain/models'
import { DEFAULT_DECK_SETTINGS } from '../../domain/models'
import { leechEffect } from './leech'

function scheduling(lapses: number, state = 2): FSRSState {
  return {
    kind: 'fsrs', stability: 2, difficulty: 7, reps: 3, lapses,
    state, step: 0, lastReview: 1, due: 2,
  }
}

function card(lapses: number, tags: string[] = [], suspended = false): Card {
  return {
    id: 'c1', deckId: 'd1', front: 'q', back: 'a', createdAt: 1, updatedAt: 1,
    tags, suspended, lastAgainAt: null, scheduling: scheduling(lapses),
  }
}

function settings(action: DeckSettings['leechAction'], threshold: number): DeckSettings {
  return { ...DEFAULT_DECK_SETTINGS, leechAction: action, leechThreshold: threshold }
}

describe('leechEffect', () => {
  it('tags a Review card when Again reaches the threshold', () => {
    expect(leechEffect(card(2), settings('tag', 3), 'again', scheduling(3))).toEqual({
      action: 'tag',
      tags: ['leech'],
      suspended: false,
    })
  })

  it('tags and suspends when the deck action is suspend', () => {
    expect(leechEffect(card(7), settings('suspend', 8), 'again', scheduling(8))).toEqual({
      action: 'suspend',
      tags: ['leech'],
      suspended: true,
    })
  })

  it.each([
    ['below threshold', card(1), 'again' as const, scheduling(2), settings('tag', 3)],
    ['non-Again grade', card(2), 'good' as const, scheduling(3), settings('tag', 3)],
    ['non-Review state', { ...card(2), scheduling: scheduling(2, 3) }, 'again' as const, scheduling(3, 3), settings('tag', 3)],
    ['lapse did not increase', card(3), 'again' as const, scheduling(3), settings('tag', 3)],
  ])('does nothing when %s', (_name, input, grade, next, deckSettings) => {
    expect(leechEffect(input, deckSettings, grade, next)).toBeNull()
  })

  it('is one-shot and does not mutate existing tags', () => {
    const tags = ['leech', 'hard']
    expect(leechEffect(card(8, tags), settings('suspend', 8), 'again', scheduling(9))).toBeNull()
    expect(tags).toEqual(['leech', 'hard'])

    const existing = ['hard']
    expect(leechEffect(card(7, existing), settings('tag', 8), 'again', scheduling(8))?.tags).toEqual(['hard', 'leech'])
    expect(existing).toEqual(['hard'])
  })
})
