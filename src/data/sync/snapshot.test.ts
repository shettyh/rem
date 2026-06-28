import { describe, it, expect } from 'vitest'
import {
  serializeSnapshot,
  deserializeSnapshot,
  EMPTY_SNAPSHOT,
  type RepoSnapshot,
} from './snapshot'
import { DEFAULT_DECK_SETTINGS } from '../../domain/models'

const sample: RepoSnapshot = {
  decks: [{ id: 'd1', name: 'Spanish', createdAt: 1, updatedAt: 1, color: '#7e6cff', schedulerKind: 'fsrs', settings: DEFAULT_DECK_SETTINGS }],
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

  it('normalizes a deck file missing the v6 fields to defaults', () => {
    const files = {
      'rem.json': JSON.stringify({ format: 'rem-sync', version: 1 }),
      'decks/d1.json': JSON.stringify({ deck: { id: 'd1', name: 'Old', createdAt: 7, schedulerKind: 'fsrs' }, cards: [] }),
      'tombstones.json': '[]',
    }
    const snap = deserializeSnapshot(files)
    expect(snap.decks[0].updatedAt).toBe(7)
    expect(snap.decks[0].color).toBeTruthy()
    expect(snap.decks[0].settings).toEqual(DEFAULT_DECK_SETTINGS)
  })
})
