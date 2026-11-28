import { describe, it, expect } from 'vitest'
import Dexie from 'dexie'
import {
  collectBackup,
  serializeBackup,
  parseBackup,
  planImport,
  type DeckBackup,
} from './backup'
import type { Storage } from './Storage'
import { RemDB } from './dexie/db'
import { DexieStorage } from './dexie/DexieStorage'
import type { Card, Deck, Grade, ReviewLog } from '../domain/models'
import { DEFAULT_DECK_SETTINGS } from '../domain/models'
import { getScheduler } from '../domain/scheduler'
import { forAll, forAllAsync, pick, randBool, randInt, randString } from '../test/prop'

const NOW = 1_700_000_000_000
const sched = { kind: 'fsrs' as const, stability: 5, difficulty: 5, reps: 1, lapses: 0, state: 2, step: 0, lastReview: 1, due: 999 }

function fakeStorage(
  decks: Deck[],
  cardsByDeck: Record<string, Card[]>,
  logsByDeck: Record<string, ReviewLog[]> = {},
): Storage {
  return {
    listDecks: async () => decks,
    listCards: async (id: string) => cardsByDeck[id] ?? [],
    listReviewLogs: async (id: string) => logsByDeck[id] ?? [],
  } as unknown as Storage
}

const deckA: Deck = { id: 'a', name: 'Spanish', createdAt: 10, updatedAt: 10, color: '#7e6cff', schedulerKind: 'fsrs', settings: DEFAULT_DECK_SETTINGS }
const cardA: Card = {
  id: 'c1', deckId: 'a', front: 'hola', back: 'hello',
  createdAt: 11, updatedAt: 12, tags: [], suspended: false, lastAgainAt: 9, scheduling: sched,
}

describe('collectBackup', () => {
  it('collects selected decks with their cards, dropping ids', async () => {
    const storage = fakeStorage([deckA], { a: [cardA] }, {
      a: [{ id: 'r1', deckId: 'a', cardId: 'c1', reviewedAt: 13, grade: 'good' }],
    })
    const out = await collectBackup(storage, ['a'])
    expect(out).toEqual([
      {
        name: 'Spanish',
        createdAt: 10,
        schedulerKind: 'fsrs',
        color: '#7e6cff',
        settings: DEFAULT_DECK_SETTINGS,
        cards: [{
          front: 'hola', back: 'hello', createdAt: 11, updatedAt: 12, tags: [], suspended: false,
          lastAgainAt: 9, scheduling: sched, reviews: [{ reviewedAt: 13, grade: 'good' }],
        }],
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
    [{ name: 'Spanish', createdAt: 10, schedulerKind: 'fsrs', settings: DEFAULT_DECK_SETTINGS, cards: [{ front: 'hola', back: 'hello', createdAt: 11, updatedAt: 12, tags: [], suspended: false, lastAgainAt: 9, scheduling: sched, reviews: [] }] }],
    1234,
  )

  it('round-trips valid FSRS input', () => {
    expect(parseBackup(valid, NOW)).toEqual([
      { name: 'Spanish', createdAt: 10, schedulerKind: 'fsrs', settings: DEFAULT_DECK_SETTINGS, cards: [{ front: 'hola', back: 'hello', createdAt: 11, updatedAt: 12, tags: [], suspended: false, lastAgainAt: 9, scheduling: sched, reviews: [] }] },
    ])
  })

  it('round-trips review history nested under its card', () => {
    const json = serializeBackup(
      [{ name: 'D', createdAt: 1, schedulerKind: 'fsrs', settings: DEFAULT_DECK_SETTINGS, cards: [
        {
          front: 'q', back: 'a', createdAt: 2, updatedAt: 3, tags: [], suspended: false,
          lastAgainAt: null, scheduling: sched,
          reviews: [{ reviewedAt: 4, grade: 'again' }, { reviewedAt: 5, grade: 'good' }],
        },
      ] }],
      NOW,
    )
    expect(parseBackup(json, NOW)[0].cards[0].reviews).toEqual([
      { reviewedAt: 4, grade: 'again' },
      { reviewedAt: 5, grade: 'good' },
    ])
  })

  it('round-trips leech tags and suspension', () => {
    const json = serializeBackup(
      [{ name: 'D', createdAt: 1, schedulerKind: 'fsrs', settings: DEFAULT_DECK_SETTINGS, cards: [
        { front: 'q', back: 'a', createdAt: 2, updatedAt: 3, tags: ['leech'], suspended: true, lastAgainAt: 2, scheduling: sched, reviews: [] },
      ] }],
      NOW,
    )
    expect(parseBackup(json, NOW)[0].cards[0]).toMatchObject({ tags: ['leech'], suspended: true, lastAgainAt: 2 })
  })

  it('defaults lastAgainAt for a backup written before custom study', () => {
    const old = JSON.parse(valid)
    delete old.decks[0].cards[0].lastAgainAt
    expect(parseBackup(JSON.stringify(old), NOW)[0].cards[0]).toMatchObject({ lastAgainAt: null, reviews: [] })
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

  it('round-trips FSRS-7 stabilityFast and strips null stabilityFast', () => {
    const v7 = { kind: 'fsrs', stability: 5, stabilityFast: 4, difficulty: 5, reps: 1, lapses: 0, state: 2, step: 0, lastReview: 1, due: 2 }
    const withNull = { kind: 'fsrs', stability: 5, stabilityFast: null, difficulty: 5, reps: 1, lapses: 0, state: 2, step: 0, lastReview: 1, due: 2 }
    const file = JSON.stringify({
      format: 'rem-backup', version: 1, exportedAt: 1,
      decks: [{
        name: 'X',
        createdAt: 1,
        schedulerKind: 'fsrs',
        cards: [
          { front: 'a', back: 'b', createdAt: 1, updatedAt: 1, scheduling: v7 },
          { front: 'c', back: 'd', createdAt: 1, updatedAt: 1, scheduling: withNull },
        ],
      }],
    })
    const parsed = parseBackup(file, NOW)[0]
    expect(parsed.cards[0].scheduling).toEqual(v7)
    expect(parsed.cards[1].scheduling).not.toHaveProperty('stabilityFast')
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
    const custom = {
      ...DEFAULT_DECK_SETTINGS,
      newPerDay: 50,
      leechAction: 'tag' as const,
      fsrsWeights: [0.2, 1.3],
    }
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

describe('backup validation & property/fuzz invariants', () => {
  const GRADES: readonly Grade[] = ['again', 'hard', 'good', 'easy']
  const SEG_ALPHA = 'abcdefghijklmnopqrstuvwxyz0123456789'

  function validDeckName(rng: () => number, suffix: number): string {
    const depth = randInt(rng, 1, 3)
    const parts = Array.from({ length: depth }, () => randString(rng, SEG_ALPHA, 2, 6))
    parts[0] = `${parts[0]}${suffix}`
    return parts.join('::')
  }

  it('rejects duplicate normalized deck names, blank fronts, invalid scheduling bounds, and clamps settings', () => {
    const baseCard = {
      front: 'Q',
      back: 'A',
      createdAt: 1,
      updatedAt: 2,
      scheduling: { kind: 'fsrs', stability: 4, difficulty: 5, reps: 1, lapses: 0, state: 2, step: 0, lastReview: 1, due: 10 },
    }
    const envelope = (decks: unknown[]) => JSON.stringify({ format: 'rem-backup', version: 1, exportedAt: NOW, decks })

    // Duplicate normalized deck names
    expect(() =>
      parseBackup(envelope([
        { name: 'CS::Rust', createdAt: 1, cards: [] },
        { name: ' CS :: Rust ', createdAt: 2, cards: [] },
      ]), NOW),
    ).toThrow(/duplicate deck name/i)

    // Invalid :: deck segments & blank deck names
    expect(() => parseBackup(envelope([{ name: '   ', createdAt: 1, cards: [] }]), NOW)).toThrow()
    expect(() => parseBackup(envelope([{ name: 'A::::B', createdAt: 1, cards: [] }]), NOW)).toThrow()

    // Blank card front
    expect(() => parseBackup(envelope([{ name: 'D', createdAt: 1, cards: [{ ...baseCard, front: '   ' }] }]), NOW)).toThrow(/malformed/i)

    // Invalid FSRS scheduling invariants: reps > 0 => stability > 0, valid state, integer step
    expect(() => parseBackup(envelope([{
      name: 'D',
      createdAt: 1,
      cards: [{ ...baseCard, scheduling: { ...baseCard.scheduling, reps: 2, stability: 0 } }],
    }]), NOW)).toThrow(/malformed/i)
    expect(() => parseBackup(envelope([{
      name: 'D',
      createdAt: 1,
      cards: [{ ...baseCard, scheduling: { ...baseCard.scheduling, state: 4 } }],
    }]), NOW)).toThrow(/malformed/i)
    expect(() => parseBackup(envelope([{
      name: 'D',
      createdAt: 1,
      cards: [{ ...baseCard, scheduling: { ...baseCard.scheduling, step: 'bad' } }],
    }]), NOW)).toThrow(/malformed/i)

    // Out-of-bounds DeckSettings are safely clamped or defaulted
    const parsed = parseBackup(envelope([{
      name: ' CS :: Rust ',
      createdAt: 1,
      settings: {
        newPerDay: 999_999,
        maxReviews: -5,
        minimumInterval: 0,
        leechThreshold: 500,
        leechAction: 'bogus',
        insertionOrder: 42,
        desiredRetention: 2.5,
        maximumInterval: 999_999,
        buryRelated: 'yes',
        fsrsWeights: [1, null, 'bad'],
      },
      cards: [baseCard],
    }]), NOW)
    expect(parsed[0].name).toBe('CS::Rust')
    expect(parsed[0].settings).toMatchObject({
      newPerDay: 9999,
      maxReviews: DEFAULT_DECK_SETTINGS.maxReviews,
      minimumInterval: DEFAULT_DECK_SETTINGS.minimumInterval,
      leechThreshold: DEFAULT_DECK_SETTINGS.leechThreshold,
      leechAction: DEFAULT_DECK_SETTINGS.leechAction,
      insertionOrder: DEFAULT_DECK_SETTINGS.insertionOrder,
      desiredRetention: DEFAULT_DECK_SETTINGS.desiredRetention,
      maximumInterval: 36500,
      buryRelated: DEFAULT_DECK_SETTINGS.buryRelated,
      fsrsWeights: null,
    })
  })

  it('property: generated valid backups round-trip through serialize -> parse -> DexieStorage.importDecks', async () => {
    await forAllAsync(25, 0xba5e01, async (rng, iter) => {
      const deckCount = randInt(rng, 1, 3)
      const decks: DeckBackup[] = Array.from({ length: deckCount }, (_, dIdx) => {
        const cardCount = randInt(rng, 0, 4)
        const groupKey = `grp-${iter}-${dIdx}`
        return {
          name: validDeckName(rng, dIdx),
          createdAt: randInt(rng, 1, NOW),
          schedulerKind: 'fsrs',
          color: pick(rng, ['#7e6cff', '#2fa86b', '#e8638c']),
          settings: {
            ...DEFAULT_DECK_SETTINGS,
            newPerDay: randInt(rng, 0, 200),
            maxReviews: randInt(rng, 0, 500),
            learnSteps: pick(rng, ['', '1m', '1m 10m']),
            relearnSteps: pick(rng, ['', '10m', '5m 15m']),
            insertionOrder: pick(rng, ['sequential', 'random'] as const),
            minimumInterval: randInt(rng, 1, 30),
            leechThreshold: randInt(rng, 1, 20),
            leechAction: pick(rng, ['tag', 'suspend'] as const),
            buryRelated: randBool(rng),
            showTimer: randBool(rng),
            desiredRetention: Number((0.7 + rng() * 0.29).toFixed(2)),
            maximumInterval: randInt(rng, 1, 36500),
            fsrsWeights: randBool(rng) ? null : [0.2, 1.3, 2.4],
          },
          cards: Array.from({ length: cardCount }, (_, cIdx) => {
            const reps = randInt(rng, 0, 10)
            const state = reps === 0 ? 0 : pick(rng, [1, 2, 3])
            const isCloze = randBool(rng, 0.4)
            return {
              front: `Prompt ${dIdx}-${cIdx} ${randString(rng, SEG_ALPHA, 1, 8)}`,
              back: `Answer ${cIdx}`,
              createdAt: randInt(rng, 1, NOW),
              updatedAt: randInt(rng, 1, NOW),
              tags: randBool(rng) ? ['leech', 'topic'] : ['topic'],
              suspended: randBool(rng, 0.2),
              lastAgainAt: randBool(rng, 0.3) ? randInt(rng, 1, NOW) : null,
              scheduling: {
                kind: 'fsrs' as const,
                stability: reps === 0 ? 0 : Number((0.5 + rng() * 20).toFixed(3)),
                ...(reps > 0 && randBool(rng) ? { stabilityFast: Number((0.3 + rng() * 15).toFixed(3)) } : {}),
                difficulty: reps === 0 ? 0 : Number((1 + rng() * 9).toFixed(3)),
                reps,
                lapses: randInt(rng, 0, reps),
                state,
                step: state === 1 || state === 3 ? randInt(rng, 0, 2) : 0,
                lastReview: reps === 0 ? null : randInt(rng, 1, NOW),
                due: randInt(rng, 1, NOW + 30 * 86_400_000),
              },
              reviews: Array.from({ length: randInt(rng, 0, 3) }, (_, rIdx) => ({
                reviewedAt: 1000 + rIdx * 86_400_000,
                grade: pick(rng, GRADES),
              })),
              ...(isCloze ? { siblingGroupId: groupKey, clozeIndex: cIdx + 1 } : {}),
            }
          }),
        }
      })

      const reparsed = parseBackup(serializeBackup(decks, NOW), NOW)
      expect(reparsed).toEqual(decks)

      const dbName = `backup-prop-${iter}`
      await Dexie.delete(dbName)
      const db = new RemDB(dbName)
      const storage = new DexieStorage(db)
      try {
        const res = await storage.importDecks(reparsed)
        expect(res.added).toEqual(decks.map((d) => d.name))
        expect(res.replaced).toEqual([])
        const storedDecks = await storage.listDecks()
        expect(storedDecks).toHaveLength(decks.length)
      } finally {
        db.close()
        await Dexie.delete(dbName)
      }
    })
  })

  it('fuzz: arbitrary mutated JSON never yields out-of-bounds DeckSettings/FSRSState or non-Error throws', () => {
    const weirdValues: unknown[] = [
      null, undefined, true, false, 0, -1, -0.5, 1e20, 36501, 999999,
      '', '   ', 'A::::B', 'bad', [], {}, [null], ['x'], [1, 2],
    ]
    forAll(150, 0xfeedface, (rng) => {
      const payload = {
        format: pick(rng, ['rem-backup', 'rem-backup', 'other', 123]),
        version: pick(rng, [1, 1, 2, '1']),
        decks: randBool(rng, 0.15) ? pick(rng, weirdValues) : [
          {
            name: pick(rng, ['Valid', ' A :: B ', '   ', 'X::::Y', 99]),
            createdAt: pick(rng, [1, NOW, -1e17, 1e17, '1', null]),
            settings: randBool(rng, 0.8) ? {
              newPerDay: pick(rng, weirdValues),
              maxReviews: pick(rng, weirdValues),
              minimumInterval: pick(rng, weirdValues),
              leechThreshold: pick(rng, weirdValues),
              leechAction: pick(rng, ['tag', 'suspend', ...weirdValues]),
              insertionOrder: pick(rng, ['sequential', 'random', ...weirdValues]),
              desiredRetention: pick(rng, [0.7, 0.9, 0.99, 0.5, 1.2, ...weirdValues]),
              maximumInterval: pick(rng, [1, 36500, 36501, 0, -1, ...weirdValues]),
              fsrsWeights: pick(rng, [null, [0.2, 1.3], [], ['a'], ...weirdValues]),
            } : pick(rng, weirdValues),
            cards: randBool(rng, 0.15) ? pick(rng, weirdValues) : [
              {
                front: pick(rng, ['Q', '  ', 123, null]),
                back: pick(rng, ['A', '', 456]),
                createdAt: pick(rng, [1, NOW, 1e17, 'x']),
                updatedAt: pick(rng, [1, NOW, 1e17, 'x']),
                lastAgainAt: pick(rng, [null, undefined, 10, 1e17, 'bad']),
                scheduling: {
                  kind: pick(rng, ['fsrs', 'sm2', 'other']),
                  stability: pick(rng, [0, 1.5, -1, 'x']),
                  stabilityFast: pick(rng, [undefined, null, 1.0, -2, 'x']),
                  difficulty: pick(rng, [0, 5, -1, 'x']),
                  reps: pick(rng, [0, 1, 2, -1, 1.5, 'x']),
                  lapses: pick(rng, [0, 1, -1, 'x']),
                  state: pick(rng, [0, 1, 2, 3, 4, -1, 'x']),
                  step: pick(rng, [undefined, null, 0, 1, -1, 1.2, 'x']),
                  lastReview: pick(rng, [null, 1, 1e17, 'x']),
                  due: pick(rng, [1, NOW, 1e17, 'x']),
                },
                reviews: pick(rng, [undefined, [], [{ reviewedAt: 1, grade: 'good' }], [{ reviewedAt: 1, grade: 'bad' }], 'x']),
              },
            ],
          },
        ],
      }

      let parsed: DeckBackup[]
      try {
        parsed = parseBackup(JSON.stringify(payload), NOW)
      } catch (err) {
        expect(err).toBeInstanceOf(Error)
        return
      }

      for (const d of parsed) {
        expect(d.name.trim().length).toBeGreaterThan(0)
        expect(d.settings.newPerDay).toBeGreaterThanOrEqual(0)
        expect(d.settings.newPerDay).toBeLessThanOrEqual(9999)
        expect(d.settings.maxReviews).toBeGreaterThanOrEqual(0)
        expect(d.settings.maxReviews).toBeLessThanOrEqual(9999)
        expect(d.settings.minimumInterval).toBeGreaterThanOrEqual(1)
        expect(d.settings.minimumInterval).toBeLessThanOrEqual(365)
        expect(d.settings.leechThreshold).toBeGreaterThanOrEqual(1)
        expect(d.settings.leechThreshold).toBeLessThanOrEqual(99)
        expect(d.settings.desiredRetention).toBeGreaterThanOrEqual(0.7)
        expect(d.settings.desiredRetention).toBeLessThanOrEqual(0.99)
        expect(d.settings.maximumInterval).toBeGreaterThanOrEqual(1)
        expect(d.settings.maximumInterval).toBeLessThanOrEqual(36500)
        for (const c of d.cards) {
          expect(c.front.trim().length).toBeGreaterThan(0)
          expect([0, 1, 2, 3]).toContain(c.scheduling.state)
          expect(Number.isInteger(c.scheduling.step) && c.scheduling.step >= 0).toBe(true)
          expect(Number.isFinite(c.scheduling.stability) && c.scheduling.stability >= 0).toBe(true)
          expect(Number.isFinite(c.scheduling.difficulty) && c.scheduling.difficulty >= 0).toBe(true)
          if (c.scheduling.reps > 0) expect(c.scheduling.stability).toBeGreaterThan(0)
        }
      }
    })
  })

  it('property: planImport normalized names match DexieStorage.importDecks added/replaced sets', async () => {
    await forAllAsync(20, 0x51a701, async (rng, iter) => {
      const dbName = `plan-prop-${iter}`
      await Dexie.delete(dbName)
      const db = new RemDB(dbName)
      const storage = new DexieStorage(db)
      try {
        const pool = ['Alpha', 'Beta::Sub', 'Gamma::One::Two', 'Delta']
        const existingNames: string[] = []
        for (const name of pool) {
          if (randBool(rng)) {
            await storage.createDeck(name)
            existingNames.push(name)
          }
        }
        const incomingDecks: DeckBackup[] = []
        const seen = new Set<string>()
        for (const name of pool) {
          if (!randBool(rng) || seen.has(name)) continue
          seen.add(name)
          const padded = randBool(rng)
            ? `  ${name.split('::').join(' :: ')}  `
            : name
          incomingDecks.push({
            name: padded,
            createdAt: 1,
            schedulerKind: 'fsrs',
            settings: DEFAULT_DECK_SETTINGS,
            cards: [],
          })
        }
        const planned = planImport(
          incomingDecks.map((d) => d.name),
          existingNames,
        )
        const actual = await storage.importDecks(incomingDecks)
        expect(actual).toEqual(planned)
      } finally {
        db.close()
        await Dexie.delete(dbName)
      }
    })
  })
})
