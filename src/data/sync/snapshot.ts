import type { SchedulerKind, SchedulingState, Tombstone } from '../../domain/models'

export type { Tombstone }

export interface DeckRecord {
  id: string
  name: string
  createdAt: number
  schedulerKind: SchedulerKind
}

export interface CardRecord {
  id: string
  deckId: string
  front: string
  back: string
  createdAt: number
  updatedAt: number
  scheduling: SchedulingState
}

export interface RepoSnapshot {
  decks: DeckRecord[]
  cards: CardRecord[]
  tombstones: Tombstone[]
}

export const SYNC_FORMAT = 'rem-sync'
export const SYNC_VERSION = 1
export const EMPTY_SNAPSHOT: RepoSnapshot = { decks: [], cards: [], tombstones: [] }

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
      decks.push(deck)
      for (const c of deckCards) cards.push(c)
    }
  }
  return { decks, cards, tombstones }
}
