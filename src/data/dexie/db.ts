import Dexie, { type EntityTable } from 'dexie'
import type { Asset, Card, CardDraft, DailyStat, Deck, ReviewLog, Tombstone } from '../../domain/models'
import { getScheduler } from '../../domain/scheduler'
import { DEFAULT_DECK_SETTINGS } from '../../domain/models'
import { deckColor } from '../../ui/deckColor'

/** IndexedDB schema. Indexed fields are listed; payloads are stored whole. */
export class RemDB extends Dexie {
  decks!: EntityTable<Deck, 'id'>
  cards!: EntityTable<Card, 'id'>
  tombstones!: EntityTable<Tombstone, 'id'>
  assets!: EntityTable<Asset, 'hash'>
  dailyStats!: EntityTable<DailyStat, 'id'>
  reviewLogs!: EntityTable<ReviewLog, 'id'>
  cardDrafts!: EntityTable<CardDraft, 'id'>

  constructor(name = 'rem') {
    super(name)
    this.version(1).stores({
      decks: 'id, createdAt',
      cards: 'id, deckId, createdAt',
    })
    // v2: stamp the scheduling-algorithm discriminant onto pre-existing
    // records written before per-deck schedulers existed. Schema unchanged.
    this.version(2)
      .stores({
        decks: 'id, createdAt',
        cards: 'id, deckId, createdAt',
      })
      .upgrade(async (tx) => {
        await tx.table('decks').toCollection().modify((d) => {
          if (!d.schedulerKind) d.schedulerKind = 'sm2'
        })
        await tx.table('cards').toCollection().modify((c) => {
          if (c.scheduling && !c.scheduling.kind) c.scheduling.kind = 'sm2'
        })
      })
    // v3: add the tombstones table for deletion sync. Additive — existing data untouched.
    this.version(3).stores({
      decks: 'id, createdAt',
      cards: 'id, deckId, createdAt',
      tombstones: 'id, deletedAt',
    })
    // v4: add the assets table for embedded images. Additive — existing data untouched.
    this.version(4).stores({
      decks: 'id, createdAt',
      cards: 'id, deckId, createdAt',
      tombstones: 'id, deletedAt',
      assets: 'hash',
    })
    // v5: SM-2 support removed. Move every deck to FSRS and reset any non-FSRS
    // card to a fresh FSRS state (due now), since SM-2 state can no longer be
    // scheduled. Schema unchanged.
    this.version(5)
      .stores({
        decks: 'id, createdAt',
        cards: 'id, deckId, createdAt',
        tombstones: 'id, deletedAt',
        assets: 'hash',
      })
      .upgrade(async (tx) => {
        const now = Date.now()
        const fresh = getScheduler().initial(now)
        await tx.table('decks').toCollection().modify((d) => {
          d.schedulerKind = 'fsrs'
        })
        await tx.table('cards').toCollection().modify((c) => {
          if (c.scheduling?.kind !== 'fsrs') c.scheduling = { ...fresh }
        })
      })
    // v6: per-deck settings. Backfill updatedAt (= createdAt), a stable color,
    // and default settings on decks created before the Deck options screen.
    this.version(6)
      .stores({
        decks: 'id, createdAt',
        cards: 'id, deckId, createdAt',
        tombstones: 'id, deletedAt',
        assets: 'hash',
      })
      .upgrade(async (tx) => {
        await tx.table('decks').toCollection().modify((d) => {
          if (d.updatedAt === undefined) d.updatedAt = d.createdAt
          if (!d.color) d.color = deckColor(d.id)
          if (!d.settings) d.settings = { ...DEFAULT_DECK_SETTINGS }
        })
      })
    // v7: learning/relearning steps. Backfill step: 0 on cards scheduled before
    // the step machine existed. Schema unchanged.
    this.version(7)
      .stores({
        decks: 'id, createdAt',
        cards: 'id, deckId, createdAt',
        tombstones: 'id, deletedAt',
        assets: 'hash',
      })
      .upgrade(async (tx) => {
        await tx.table('cards').toCollection().modify((c) => {
          if (c.scheduling && c.scheduling.step === undefined) c.scheduling.step = 0
        })
      })
    // v8: add the dailyStats table for daily caps (#3b). Additive — existing data untouched.
    this.version(8).stores({
      decks: 'id, createdAt',
      cards: 'id, deckId, createdAt',
      tombstones: 'id, deletedAt',
      assets: 'hash',
      dailyStats: 'id, deckId, day',
    })
    // v9: durable leech tags + suspension. Schema unchanged; backfill card payloads.
    this.version(9)
      .stores({
        decks: 'id, createdAt',
        cards: 'id, deckId, createdAt',
        tombstones: 'id, deletedAt',
        assets: 'hash',
        dailyStats: 'id, deckId, day',
      })
      .upgrade(async (tx) => {
        await tx.table('cards').toCollection().modify((c) => {
          if (!Array.isArray(c.tags)) c.tags = []
          if (c.suspended === undefined) c.suspended = false
        })
      })
    // v10: exact custom-study forgotten-card selection. Schema unchanged.
    this.version(10)
      .stores({
        decks: 'id, createdAt',
        cards: 'id, deckId, createdAt',
        tombstones: 'id, deletedAt',
        assets: 'hash',
        dailyStats: 'id, deckId, day',
      })
      .upgrade(async (tx) => {
        await tx.table('cards').toCollection().modify((c) => {
          if (c.lastAgainAt === undefined) c.lastAgainAt = null
        })
      })
    // v11: immutable FSRS review history and per-deck personalized weights.
    this.version(11)
      .stores({
        decks: 'id, createdAt',
        cards: 'id, deckId, createdAt',
        tombstones: 'id, deletedAt',
        assets: 'hash',
        dailyStats: 'id, deckId, day',
        reviewLogs: 'id, deckId, cardId, reviewedAt',
      })
      .upgrade(async (tx) => {
        await tx.table('decks').toCollection().modify((d) => {
          if (d.settings && d.settings.fsrsWeights === undefined) d.settings.fsrsWeights = null
        })
      })
    // v12: local card proposals awaiting human approval. Drafts are excluded
    // from logical snapshots and therefore never enter backup or Git sync.
    this.version(12).stores({
      decks: 'id, createdAt',
      cards: 'id, deckId, createdAt',
      tombstones: 'id, deletedAt',
      assets: 'hash',
      dailyStats: 'id, deckId, day',
      reviewLogs: 'id, deckId, cardId, reviewedAt',
      cardDrafts: 'id, deckId, createdAt',
    })
  }
}
