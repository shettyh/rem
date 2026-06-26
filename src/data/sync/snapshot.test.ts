import { describe, it, expect } from 'vitest'
import {
  serializeSnapshot,
  deserializeSnapshot,
  EMPTY_SNAPSHOT,
  type RepoSnapshot,
} from './snapshot'

const sample: RepoSnapshot = {
  decks: [{ id: 'd1', name: 'Spanish', createdAt: 1, schedulerKind: 'sm2' }],
  cards: [
    {
      id: 'c1',
      deckId: 'd1',
      front: 'hola',
      back: 'hello',
      createdAt: 2,
      updatedAt: 3,
      scheduling: { kind: 'sm2', repetitions: 0, intervalDays: 0, easeFactor: 2.5, due: 4 },
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
})
