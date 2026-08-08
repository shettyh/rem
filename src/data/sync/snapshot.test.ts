import { describe, it, expect } from 'vitest'
import {
  serializeSnapshot,
  deserializeSnapshot,
  EMPTY_SNAPSHOT,
  type RepoSnapshot,
} from './snapshot'
import { DEFAULT_DECK_SETTINGS } from '../../domain/models'
import { deckColor } from '../../ui/deckColor'

const sample: RepoSnapshot = {
  decks: [{
    id: 'd1', name: 'Spanish', createdAt: 1, updatedAt: 1, color: '#7e6cff', schedulerKind: 'fsrs',
    settings: { ...DEFAULT_DECK_SETTINGS, fsrsWeights: [0.2, 1.3] },
  }],
  cards: [
    {
      id: 'c1',
      deckId: 'd1',
      front: 'hola',
      back: 'hello',
      createdAt: 2,
      updatedAt: 3,
      tags: ['leech'],
      suspended: true,
      lastAgainAt: 2,
      scheduling: { kind: 'fsrs', stability: 0, difficulty: 0, reps: 0, lapses: 0, state: 0, step: 0, lastReview: null, due: 4 },
    },
  ],
  reviewLogs: [{ id: 'r1', deckId: 'd1', cardId: 'c1', reviewedAt: 4, grade: 'good' }],
  tombstones: [{ id: 'c9', kind: 'card', deletedAt: 5 }],
  assets: [],
}

describe('snapshot', () => {
  it('round-trips a snapshot through files', () => {
    const files = serializeSnapshot(sample)
    expect(Object.keys(files)).toContain('rem.json')
    expect(Object.keys(files)).toContain('decks/d1.json')
    expect(deserializeSnapshot(files)).toEqual(sample)
  })

  it('deserializes an empty file map to the empty snapshot', () => {
    expect(deserializeSnapshot({})).toEqual(EMPTY_SNAPSHOT)
  })

  it('groups cards under their deck file', () => {
    const files = serializeSnapshot(sample)
    expect(files['decks/d1.json']).toContain('hola')
  })

  it('rejects a deck ID that does not match its safe filename', () => {
    const files = {
      'decks/safe.json': JSON.stringify({
        deck: { ...sample.decks[0], id: '../../outside' },
        cards: [],
      }),
    }

    expect(() => deserializeSnapshot(files)).toThrow('Invalid deck ID in decks/safe.json.')
  })

  it('rejects malformed card IDs from a synced deck', () => {
    const files = serializeSnapshot(sample)
    const payload = JSON.parse(files['decks/d1.json'])
    payload.cards[0].id = '../../outside'
    files['decks/d1.json'] = JSON.stringify(payload)

    expect(() => deserializeSnapshot(files)).toThrow('Invalid card in decks/d1.json.')
  })

  it('normalizes a deck file missing the v6 fields to defaults', () => {
    const files = {
      'rem.json': JSON.stringify({ format: 'rem-sync', version: 1 }),
      'decks/d1.json': JSON.stringify({ deck: { id: 'd1', name: 'Old', createdAt: 7, schedulerKind: 'fsrs' }, cards: [] }),
      'tombstones.json': '[]',
    }
    const snap = deserializeSnapshot(files)
    expect(snap.decks[0].updatedAt).toBe(7)
    expect(snap.decks[0].color).toBe(deckColor('d1'))
    expect(snap.decks[0].settings).toEqual(DEFAULT_DECK_SETTINGS)
  })

  it('normalizes a card scheduling missing step (pre-#3a snapshot) to step 0', () => {
    const files = {
      'rem.json': JSON.stringify({ format: 'rem-sync', version: 1 }),
      'decks/d1.json': JSON.stringify({
        deck: sample.decks[0],
        cards: [
          {
            id: 'c1',
            deckId: 'd1',
            front: 'hola',
            back: 'hello',
            createdAt: 2,
            updatedAt: 3,
            scheduling: { kind: 'fsrs', stability: 0, difficulty: 0, reps: 0, lapses: 0, state: 0, lastReview: null, due: 4 },
          },
        ],
      }),
      'tombstones.json': '[]',
    }
    const snap = deserializeSnapshot(files)
    expect(snap.cards[0].scheduling).toMatchObject({ step: 0 })
    expect(snap.cards[0]).toMatchObject({ tags: [], suspended: false, lastAgainAt: null })
    expect(snap.reviewLogs).toEqual([])
  })

  it('normalizes missing personalized weights in an old deck settings payload', () => {
    const { fsrsWeights: _missing, ...oldSettings } = DEFAULT_DECK_SETTINGS
    const files = {
      'rem.json': JSON.stringify({ format: 'rem-sync', version: 1 }),
      'decks/d1.json': JSON.stringify({
        deck: { ...sample.decks[0], settings: oldSettings },
        cards: [],
      }),
      'tombstones.json': '[]',
    }
    expect(deserializeSnapshot(files).decks[0].settings.fsrsWeights).toBeNull()
  })
})
