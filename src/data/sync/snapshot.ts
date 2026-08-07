import type { SchedulerKind, SchedulingState, Tombstone, DeckSettings } from '../../domain/models'
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
    settings: d.settings ?? DEFAULT_DECK_SETTINGS,
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
  scheduling: SchedulingState
}

export interface AssetBlob {
  hash: string
  mime: string
  bytes: Uint8Array
}

export interface RepoSnapshot {
  decks: DeckRecord[]
  cards: CardRecord[]
  tombstones: Tombstone[]
  assets: AssetBlob[]
}

export const SYNC_FORMAT = 'rem-sync'
export const SYNC_VERSION = 1
export const EMPTY_SNAPSHOT: RepoSnapshot = { decks: [], cards: [], tombstones: [], assets: [] }

interface DeckFile {
  deck: DeckRecord
  cards: CardRecord[]
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
  for (const deck of snap.decks) {
    const payload: DeckFile = { deck, cards: cardsByDeck.get(deck.id) ?? [] }
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
  let tombstones: Tombstone[] = []
  for (const [path, content] of Object.entries(files)) {
    if (path === 'rem.json') continue
    if (path === 'tombstones.json') {
      tombstones = JSON.parse(content) as Tombstone[]
      continue
    }
    if (path.startsWith('decks/') && path.endsWith('.json')) {
      const { deck, cards: deckCards } = JSON.parse(content) as DeckFile
      decks.push(normalizeDeck(deck))
      for (const c of deckCards) cards.push(normalizeCard(c))
    }
  }
  return { decks, cards, tombstones, assets: [] }
}
