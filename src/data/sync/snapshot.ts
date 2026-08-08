import type { DeckSettings, Grade, SchedulerKind, SchedulingState, Tombstone } from '../../domain/models'
import { DEFAULT_DECK_SETTINGS } from '../../domain/models'
import { deckColor } from '../../ui/deckColor'

export type { Tombstone }

export interface DeckRecord {
  id: string
  name: string
  createdAt: number
  updatedAt: number
  color: string
  schedulerKind: SchedulerKind
  settings: DeckSettings
}

function normalizeDeck(d: DeckRecord): DeckRecord {
  // Fields below may be absent in snapshots written before v6 (the JSON is cast, not validated), so ?? supplies backward-compatible defaults.
  return {
    ...d,
    updatedAt: d.updatedAt ?? d.createdAt,
    color: d.color ?? deckColor(d.id),
    settings: { ...DEFAULT_DECK_SETTINGS, ...(d.settings ?? {}) },
  }
}

function normalizeCard(c: CardRecord): CardRecord {
  // Fields may be absent in snapshots written before their migrations because JSON is cast, not validated.
  const s = c.scheduling
  const scheduling = s.kind === 'fsrs' && s.step === undefined ? { ...s, step: 0 } : s
  return {
    ...c,
    tags: Array.isArray(c.tags) && c.tags.every((tag) => typeof tag === 'string') ? c.tags : [],
    suspended: c.suspended === true,
    lastAgainAt: typeof c.lastAgainAt === 'number' ? c.lastAgainAt : null,
    scheduling,
  }
}

export interface CardRecord {
  id: string
  deckId: string
  front: string
  back: string
  createdAt: number
  updatedAt: number
  tags: string[]
  suspended: boolean
  lastAgainAt: number | null
  scheduling: SchedulingState
}

export interface ReviewLogRecord {
  id: string
  deckId: string
  cardId: string
  reviewedAt: number
  grade: Grade
}

export interface AssetBlob {
  hash: string
  mime: string
  bytes: Uint8Array
}

export interface RepoSnapshot {
  decks: DeckRecord[]
  cards: CardRecord[]
  reviewLogs: ReviewLogRecord[]
  tombstones: Tombstone[]
  assets: AssetBlob[]
}

export const SYNC_FORMAT = 'rem-sync'
export const SYNC_VERSION = 1
export const EMPTY_SNAPSHOT: RepoSnapshot = { decks: [], cards: [], reviewLogs: [], tombstones: [], assets: [] }

const SAFE_ID = /^[A-Za-z0-9_-]+$/

interface DeckFile {
  deck: DeckRecord
  cards: CardRecord[]
  reviewLogs?: ReviewLogRecord[]
}

/** Serialize a snapshot to the file-per-deck layout: rem.json manifest,
 *  decks/<id>.json (deck + its cards), tombstones.json. */
export function serializeSnapshot(snap: RepoSnapshot): Record<string, string> {
  const files: Record<string, string> = {}
  files['rem.json'] = JSON.stringify({ format: SYNC_FORMAT, version: SYNC_VERSION }, null, 2)

  const cardsByDeck = new Map<string, CardRecord[]>()
  for (const c of snap.cards) {
    const arr = cardsByDeck.get(c.deckId) ?? []
    arr.push(c)
    cardsByDeck.set(c.deckId, arr)
  }
  const reviewsByDeck = new Map<string, ReviewLogRecord[]>()
  for (const review of snap.reviewLogs) {
    const arr = reviewsByDeck.get(review.deckId) ?? []
    arr.push(review)
    reviewsByDeck.set(review.deckId, arr)
  }
  for (const deck of snap.decks) {
    const payload: DeckFile = {
      deck,
      cards: cardsByDeck.get(deck.id) ?? [],
      reviewLogs: reviewsByDeck.get(deck.id) ?? [],
    }
    files[`decks/${deck.id}.json`] = JSON.stringify(payload, null, 2)
  }
  files['tombstones.json'] = JSON.stringify(snap.tombstones, null, 2)
  return files
}

/** Inverse of {@link serializeSnapshot}. Unknown paths (e.g. future reviews/)
 *  are ignored. An empty map yields the empty snapshot. */
export function deserializeSnapshot(files: Record<string, string>): RepoSnapshot {
  const decks: DeckRecord[] = []
  const cards: CardRecord[] = []
  const reviewLogs: ReviewLogRecord[] = []
  let tombstones: Tombstone[] = []
  for (const [path, content] of Object.entries(files)) {
    if (path === 'rem.json') continue
    if (path === 'tombstones.json') {
      tombstones = JSON.parse(content) as Tombstone[]
      continue
    }
    if (path.startsWith('decks/') && path.endsWith('.json')) {
      const { deck, cards: deckCards, reviewLogs: deckReviews = [] } = JSON.parse(content) as DeckFile
      const fileDeckId = path.slice('decks/'.length, -'.json'.length)
      if (!SAFE_ID.test(fileDeckId) || deck?.id !== fileDeckId) {
        throw new Error(`Invalid deck ID in ${path}.`)
      }
      if (!Array.isArray(deckCards)) throw new Error(`Invalid cards in ${path}.`)
      decks.push(normalizeDeck(deck))
      for (const c of deckCards) {
        if (!SAFE_ID.test(c?.id) || c.deckId !== fileDeckId) {
          throw new Error(`Invalid card in ${path}.`)
        }
        cards.push(normalizeCard(c))
      }
      reviewLogs.push(...deckReviews)
    }
  }
  return { decks, cards, reviewLogs, tombstones, assets: [] }
}
