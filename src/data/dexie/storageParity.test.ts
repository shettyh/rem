import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import Dexie from 'dexie'
import { DEFAULT_DECK_SETTINGS, FSRS_STATE } from '../../domain/models'
import { MS_PER_DAY } from '../../domain/scheduler'
import { RemDB } from './db'
import { DexieStorage } from './DexieStorage'

const DB_NAME = 'rem-parity-contract'
let db: RemDB
let storage: DexieStorage

beforeEach(async () => {
  await Dexie.delete(DB_NAME)
  db = new RemDB(DB_NAME)
  storage = new DexieStorage(db)
})

afterEach(async () => {
  db.close()
  await Dexie.delete(DB_NAME)
})

describe('DexieStorage rem-core parity contract', () => {
  it('enforces deck-name normalization and cascades parent rename to nested subdecks', async () => {
    await expect(storage.createDeck('   ')).rejects.toThrow(/blank/i)
    await expect(storage.createDeck('CS::::Rust')).rejects.toThrow(/empty '::'/i)

    const parent = await storage.createDeck(' CS ')
    const child = await storage.createDeck(' CS :: Systems ')
    const leaf = await storage.createDeck('CS::Systems::Rust')
    const unrelated = await storage.createDeck('CSS')

    expect(parent.name).toBe('CS')
    expect(child.name).toBe('CS::Systems')

    await storage.updateDeck(parent.id, { name: ' Computing ' })
    expect((await storage.getDeck(parent.id))?.name).toBe('Computing')
    expect((await storage.getDeck(child.id))?.name).toBe('Computing::Systems')
    expect((await storage.getDeck(leaf.id))?.name).toBe('Computing::Systems::Rust')
    expect((await storage.getDeck(unrelated.id))?.name).toBe('CSS')
  })

  it('rejects blank fronts, missing decks, normalizes user tags, and skips code-span clozes on card creation', async () => {
    const deck = await storage.createDeck('Validation')
    await expect(storage.createCard(deck.id, '   ', 'back')).rejects.toThrow(/blank/i)
    await expect(storage.createCard('missing-deck', 'front', 'back')).rejects.toThrow(/deck not found/i)

    const card = await storage.createCard(deck.id, 'Question', 'Answer', [' Rust ', 'rust', 'leech', '  ', 'Async'])
    expect(card.tags).toEqual(['Rust', 'Async'])

    // Cloze syntax inside inline code must NOT split into cloze cards.
    const codeOnly = await storage.createCardOrCloze(deck.id, 'Use `{{c1::foo}}` syntax', 'Answer')
    expect(codeOnly).toHaveLength(1)
    expect(codeOnly[0].clozeIndex).toBeUndefined()
    expect(codeOnly[0].siblingGroupId).toBeUndefined()
  })

  it('updateClozeNote propagates edits, preserves per-sibling leech tags, adds new clozes, deletes unreviewed orphans, and suspends reviewed orphans', async () => {
    const deck = await storage.createDeck('Cloze')
    const [c1, c2] = await storage.createCardOrCloze(deck.id, '{{c1::One}} and {{c2::Two}}', 'Extra', ['orig'])
    expect(c1.siblingGroupId).toBeTruthy()
    expect(c2.siblingGroupId).toBe(c1.siblingGroupId)

    // Mark c1 as reviewed + leech; leave c2 unreviewed.
    await storage.updateCard(c1.id, {
      tags: ['leech', 'orig'],
      scheduling: { ...c1.scheduling, reps: 2, state: FSRS_STATE.Review, stability: 5, difficulty: 5 },
    })

    // Remove c1 and c2, add c3.
    const affected = await storage.updateClozeNote(c1.id, 'Only {{c3::Three}} now', 'Updated extra', ['updated', 'leech'])
    const byCloze = new Map(affected.map((c) => [c.clozeIndex, c]))

    // c1 (reviewed) is suspended and retains its leech tag + updated text/user tags.
    const updatedC1 = await storage.getCard(c1.id)
    expect(updatedC1).toMatchObject({
      front: 'Only {{c3::Three}} now',
      back: 'Updated extra',
      suspended: true,
      tags: ['leech', 'updated'],
    })
    expect(byCloze.get(1)?.id).toBe(c1.id)

    // c2 (unreviewed) is deleted with a card tombstone.
    expect(await storage.getCard(c2.id)).toBeUndefined()
    expect(await db.tombstones.get(c2.id)).toMatchObject({ id: c2.id, kind: 'card' })

    // c3 is newly created in the same sibling group.
    const c3 = byCloze.get(3)
    expect(c3).toBeDefined()
    expect(c3?.siblingGroupId).toBe(c1.siblingGroupId)
    expect(c3?.suspended).toBe(false)
    expect(c3?.tags).toEqual(['updated'])
  })

  it('importDecks replace clears dailyStats, records deck tombstones, normalizes tags, and remaps siblingGroupId', async () => {
    const existing = await storage.createDeck('Spanish')
    const oldCard = await storage.createCard(existing.id, 'old', 'old')
    await storage.commitReview({
      cardId: oldCard.id,
      deckId: existing.id,
      patch: {},
      reviewedAt: Date.now(),
      daily: { day: '2026-08-01', field: 'newIntroduced' },
    })

    const res = await storage.importDecks([{
      name: '  Spanish  ',
      createdAt: 10,
      schedulerKind: 'fsrs',
      settings: DEFAULT_DECK_SETTINGS,
      cards: [
        {
          front: '{{c1::hola}} {{c2::mundo}}',
          back: 'hello world',
          createdAt: 11,
          updatedAt: 12,
          tags: [' Vocab ', 'vocab', 'leech', 'LEECH'],
          suspended: false,
          lastAgainAt: null,
          scheduling: { kind: 'fsrs', stability: 0, difficulty: 0, reps: 0, lapses: 0, state: 0, step: 0, lastReview: null, due: 10 },
          reviews: [],
          siblingGroupId: 'orig-group',
          clozeIndex: 1,
        },
        {
          front: '{{c1::hola}} {{c2::mundo}}',
          back: 'hello world',
          createdAt: 11,
          updatedAt: 12,
          tags: ['vocab'],
          suspended: false,
          lastAgainAt: null,
          scheduling: { kind: 'fsrs', stability: 0, difficulty: 0, reps: 0, lapses: 0, state: 0, step: 0, lastReview: null, due: 10 },
          reviews: [],
          siblingGroupId: 'orig-group',
          clozeIndex: 2,
        },
      ],
    }])

    expect(res).toEqual({ added: [], replaced: ['Spanish'] })
    expect(await storage.getDailyStat(existing.id, '2026-08-01')).toEqual({ newIntroduced: 0, reviewsDone: 0 })
    expect(await db.tombstones.get(existing.id)).toMatchObject({ id: existing.id, kind: 'deck' })

    const [newDeck] = await storage.listDecks()
    const importedCards = await storage.listCards(newDeck.id)
    expect(importedCards).toHaveLength(2)
    const c1 = importedCards.find((c) => c.clozeIndex === 1)!
    const c2 = importedCards.find((c) => c.clozeIndex === 2)!
    expect(c1.tags).toEqual(['leech', 'Vocab'])
    expect(c2.tags).toEqual(['vocab'])
    expect(c1.siblingGroupId).toBeTruthy()
    expect(c1.siblingGroupId).not.toBe('orig-group')
    expect(c2.siblingGroupId).toBe(c1.siblingGroupId)
  })

  it('sweepOrphanAssets protects assets referenced by both cards and pending drafts', async () => {
    const deck = await storage.createDeck('Media')
    const cardAsset = await storage.putAsset(new Uint8Array([1, 2, 3]), 'image/png')
    const draftAsset = await storage.putAsset(new Uint8Array([4, 5, 6]), 'image/png')
    const orphanAsset = await storage.putAsset(new Uint8Array([7, 8, 9]), 'image/png')

    await storage.createCard(deck.id, `![card](asset:${cardAsset.hash})`, 'back')
    await storage.proposeDrafts(
      deck.id,
      [{ front: `![draft](asset:${draftAsset.hash})`, back: 'back', tags: [], rationale: null, sources: [] }],
      { proposedBy: 'agent' },
    )

    await storage.sweepOrphanAssets()

    expect(await storage.getAsset(cardAsset.hash)).toBeDefined()
    expect(await storage.getAsset(draftAsset.hash)).toBeDefined()
    expect(await storage.getAsset(orphanAsset.hash)).toBeUndefined()
  })

  it('searchCards matches across fields with deck/limit filters and getStats computes upcoming7d forecast', async () => {
    const now = new Date(2026, 7, 10, 12, 0, 0, 0).getTime()
    const d1 = await storage.createDeck('Rust')
    const d2 = await storage.createDeck('SQL')
    const c1 = await storage.createCard(d1.id, 'Ownership model', 'Borrow checker', ['memory'])
    const c2 = await storage.createCard(d2.id, 'WAL checkpoints', 'SQLite durability', ['storage'])

    await storage.updateCard(c1.id, {
      scheduling: { kind: 'fsrs', stability: 5, difficulty: 5, reps: 2, lapses: 0, state: 2, step: 0, lastReview: now - MS_PER_DAY, due: now - 1000 },
    })
    await storage.updateCard(c2.id, {
      scheduling: { kind: 'fsrs', stability: 5, difficulty: 5, reps: 2, lapses: 0, state: 2, step: 0, lastReview: now - MS_PER_DAY, due: now + 2 * MS_PER_DAY },
    })

    expect(await storage.searchCards('   ')).toEqual([])
    const hits = await storage.searchCards('borrow memory')
    expect(hits.map((h) => h.card.id)).toEqual([c1.id])
    expect(await storage.searchCards('borrow', { deckId: d2.id })).toEqual([])
    expect((await storage.searchCards('e', { limit: 1 }))).toHaveLength(1)

    const stats = await storage.getStats(null, 7, now)
    expect(stats.upcoming7d).toHaveLength(7)
    expect(stats.upcoming7d[0].count).toBe(1) // overdue c1 counted today
    expect(stats.upcoming7d[2].count).toBe(1) // c2 due in 2 days
  })
})
