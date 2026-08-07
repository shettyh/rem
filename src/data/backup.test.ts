import { describe, it, expect } from 'vitest'
import {
  collectBackup,
  serializeBackup,
  parseBackup,
  planImport,
  type DeckBackup,
} from './backup'
import type { Storage } from './Storage'
import type { Card, Deck } from '../domain/models'
import { DEFAULT_DECK_SETTINGS } from '../domain/models'
import { getScheduler } from '../domain/scheduler'

const NOW = 1_700_000_000_000
const sched = { kind: 'fsrs' as const, stability: 5, difficulty: 5, reps: 1, lapses: 0, state: 2, step: 0, lastReview: 1, due: 999 }

function fakeStorage(decks: Deck[], cardsByDeck: Record<string, Card[]>): Storage {
  return {
    listDecks: async () => decks,
    listCards: async (id: string) => cardsByDeck[id] ?? [],
  } as unknown as Storage
}

const deckA: Deck = { id: 'a', name: 'Spanish', createdAt: 10, updatedAt: 10, color: '#7e6cff', schedulerKind: 'fsrs', settings: DEFAULT_DECK_SETTINGS }
const cardA: Card = {
  id: 'c1', deckId: 'a', front: 'hola', back: 'hello',
  createdAt: 11, updatedAt: 12, tags: [], suspended: false, scheduling: sched,
}

describe('collectBackup', () => {
  it('collects selected decks with their cards, dropping ids', async () => {
    const storage = fakeStorage([deckA], { a: [cardA] })
    const out = await collectBackup(storage, ['a'])
    expect(out).toEqual([
      {
        name: 'Spanish',
        createdAt: 10,
        schedulerKind: 'fsrs',
        color: '#7e6cff',
        settings: DEFAULT_DECK_SETTINGS,
        cards: [{ front: 'hola', back: 'hello', createdAt: 11, updatedAt: 12, tags: [], suspended: false, scheduling: sched }],
      },
    ])
  })

  it('skips deck ids that do not exist', async () => {
    const storage = fakeStorage([deckA], { a: [cardA] })
    expect(await collectBackup(storage, ['missing'])).toEqual([])
  })
})

describe('serializeBackup', () => {
  it('emits the format/version/exportedAt envelope', () => {
    const decks: DeckBackup[] = [{ name: 'Spanish', createdAt: 10, schedulerKind: 'fsrs', settings: DEFAULT_DECK_SETTINGS, cards: [] }]
    const parsed = JSON.parse(serializeBackup(decks, 1234))
    expect(parsed.format).toBe('rem-backup')
    expect(parsed.version).toBe(1)
    expect(parsed.exportedAt).toBe(1234)
    expect(parsed.decks).toEqual(decks)
  })
})

describe('parseBackup', () => {
  const valid = serializeBackup(
    [{ name: 'Spanish', createdAt: 10, schedulerKind: 'fsrs', settings: DEFAULT_DECK_SETTINGS, cards: [{ front: 'hola', back: 'hello', createdAt: 11, updatedAt: 12, tags: [], suspended: false, scheduling: sched }] }],
    1234,
  )

  it('round-trips valid FSRS input', () => {
    expect(parseBackup(valid, NOW)).toEqual([
      { name: 'Spanish', createdAt: 10, schedulerKind: 'fsrs', settings: DEFAULT_DECK_SETTINGS, cards: [{ front: 'hola', back: 'hello', createdAt: 11, updatedAt: 12, tags: [], suspended: false, scheduling: sched }] },
    ])
  })

  it('round-trips leech tags and suspension', () => {
    const json = serializeBackup(
      [{ name: 'D', createdAt: 1, schedulerKind: 'fsrs', settings: DEFAULT_DECK_SETTINGS, cards: [
        { front: 'q', back: 'a', createdAt: 2, updatedAt: 3, tags: ['leech'], suspended: true, scheduling: sched },
      ] }],
      NOW,
    )
    expect(parseBackup(json, NOW)[0].cards[0]).toMatchObject({ tags: ['leech'], suspended: true })
  })

  it('rejects non-JSON', () => {
    expect(() => parseBackup('not json', NOW)).toThrow(/valid JSON/i)
  })

  it('rejects a wrong format tag', () => {
    expect(() => parseBackup(JSON.stringify({ format: 'other', version: 1, decks: [] }), NOW)).toThrow(/rem backup/i)
  })

  it('rejects an unsupported version', () => {
    expect(() => parseBackup(JSON.stringify({ format: 'rem-backup', version: 2, decks: [] }), NOW)).toThrow(/version/i)
  })

  it('rejects a malformed deck', () => {
    expect(() => parseBackup(JSON.stringify({ format: 'rem-backup', version: 1, decks: [{ name: 5, createdAt: 1, cards: [] }] }), NOW)).toThrow(/malformed/i)
  })

  it('rejects a malformed card', () => {
    const bad = { format: 'rem-backup', version: 1, decks: [{ name: 'x', createdAt: 1, cards: [{ front: 'a' }] }] }
    expect(() => parseBackup(JSON.stringify(bad), NOW)).toThrow(/malformed/i)
  })

  it('resets legacy SM-2 scheduling (no kind) to a fresh FSRS card', () => {
    const legacy = JSON.stringify({
      format: 'rem-backup', version: 1, exportedAt: 1,
      decks: [{ name: 'X', createdAt: 1, cards: [
        { front: 'a', back: 'b', createdAt: 1, updatedAt: 1, scheduling: { repetitions: 1, intervalDays: 3, easeFactor: 2.6, due: 999 } },
      ] }],
    })
    expect(parseBackup(legacy, NOW)[0].cards[0].scheduling).toEqual(getScheduler().initial(NOW))
  })

  it('resets explicit SM-2 scheduling to a fresh FSRS card', () => {
    const legacy = JSON.stringify({
      format: 'rem-backup', version: 1, exportedAt: 1,
      decks: [{ name: 'X', createdAt: 1, schedulerKind: 'sm2', cards: [
        { front: 'a', back: 'b', createdAt: 1, updatedAt: 1, scheduling: { kind: 'sm2', repetitions: 4, intervalDays: 30, easeFactor: 2.6, due: 50 } },
      ] }],
    })
    const deck = parseBackup(legacy, NOW)[0]
    expect(deck.schedulerKind).toBe('fsrs')
    expect(deck.cards[0].scheduling).toEqual(getScheduler().initial(NOW))
  })

  it('accepts FSRS scheduling unchanged', () => {
    const f = { kind: 'fsrs', stability: 5, difficulty: 5, reps: 1, lapses: 0, state: 2, step: 0, lastReview: 1, due: 2 }
    const file = JSON.stringify({
      format: 'rem-backup', version: 1, exportedAt: 1,
      decks: [{ name: 'X', createdAt: 1, schedulerKind: 'fsrs', cards: [{ front: 'a', back: 'b', createdAt: 1, updatedAt: 1, scheduling: f }] }],
    })
    const parsed = parseBackup(file, NOW)[0]
    expect(parsed.schedulerKind).toBe('fsrs')
    expect(parsed.cards[0].scheduling).toEqual(f)
  })

  it('backfills a missing step to 0 for fsrs scheduling (pre-#3a backup)', () => {
    const f = { kind: 'fsrs', stability: 5, difficulty: 5, reps: 1, lapses: 0, state: 2, lastReview: 1, due: 2 } // no step
    const file = JSON.stringify({
      format: 'rem-backup', version: 1, exportedAt: 1,
      decks: [{ name: 'X', createdAt: 1, schedulerKind: 'fsrs', cards: [{ front: 'a', back: 'b', createdAt: 1, updatedAt: 1, scheduling: f }] }],
    })
    const parsed = parseBackup(file, NOW)[0]
    expect(parsed.cards[0].scheduling).toMatchObject({ step: 0 })
  })

  it('defaults a deck without schedulerKind to fsrs', () => {
    const legacy = JSON.stringify({
      format: 'rem-backup', version: 1, exportedAt: 1,
      decks: [{ name: 'X', createdAt: 1, cards: [] }],
    })
    expect(parseBackup(legacy, NOW)[0].schedulerKind).toBe('fsrs')
  })
})

describe('planImport', () => {
  it('splits incoming names into added vs replaced and de-dupes', () => {
    expect(planImport(['Spanish', 'French', 'Spanish'], ['Spanish', 'German'])).toEqual({
      added: ['French'],
      replaced: ['Spanish'],
    })
  })
})

describe('settings round-trip', () => {
  it('preserves custom settings through serialize -> parse', () => {
    const custom = { ...DEFAULT_DECK_SETTINGS, newPerDay: 50, leechAction: 'tag' as const }
    const json = serializeBackup(
      [{ name: 'D', createdAt: 1, schedulerKind: 'fsrs', color: '#2fa86b', settings: custom, cards: [] }],
      NOW,
    )
    const parsed = parseBackup(json, NOW)
    expect(parsed[0].settings).toEqual(custom)
    expect(parsed[0].color).toBe('#2fa86b')
  })

  it('defaults settings to DEFAULT_DECK_SETTINGS for an old backup without them', () => {
    const oldFile = JSON.stringify({
      format: 'rem-backup',
      version: 1,
      exportedAt: NOW,
      decks: [{ name: 'Legacy', createdAt: 1, cards: [] }],
    })
    const parsed = parseBackup(oldFile, NOW)
    expect(parsed[0].settings).toEqual(DEFAULT_DECK_SETTINGS)
    expect(parsed[0].color).toBeUndefined()
  })
})
