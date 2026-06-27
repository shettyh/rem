import type { AssetBlob, CardRecord, DeckRecord, RepoSnapshot, Tombstone } from './snapshot'
import { assetRefs } from '../assetRefs'

export interface DbOps {
  upsertDecks: DeckRecord[]
  upsertCards: CardRecord[]
  deleteDeckIds: string[]
  deleteCardIds: string[]
  tombstones: Tombstone[]
  upsertAssets: AssetBlob[]
  deleteAssetHashes: string[]
}

export interface MergeResult {
  merged: RepoSnapshot
  dbOps: DbOps
}

function newestTombstones(a: Tombstone[], b: Tombstone[]): Map<string, Tombstone> {
  const map = new Map<string, Tombstone>()
  for (const t of [...a, ...b]) {
    const prev = map.get(t.id)
    if (!prev || t.deletedAt > prev.deletedAt) map.set(t.id, t)
  }
  return map
}

/** Reconcile two snapshots: per-card last-writer-wins by updatedAt, with
 *  tombstones removing records whose deletion is newer than their last edit.
 *  `dbOps` reconciles the LOCAL store toward `merged` (upserts are idempotent). */
export function merge(local: RepoSnapshot, remote: RepoSnapshot): MergeResult {
  const tombstones = newestTombstones(local.tombstones, remote.tombstones)

  // Decks: union by id (decks are immutable today). Drop if a deck tombstone
  // is at/after the deck's creation (a re-created deck gets a fresh id, so the
  // tombstone never shadows it).
  const deckById = new Map<string, DeckRecord>()
  for (const d of [...remote.decks, ...local.decks]) deckById.set(d.id, d)
  const mergedDecks: DeckRecord[] = []
  for (const d of deckById.values()) {
    const t = tombstones.get(d.id)
    if (t && t.kind === 'deck' && t.deletedAt >= d.createdAt) continue
    mergedDecks.push(d)
  }
  const liveDeckIds = new Set(mergedDecks.map((d) => d.id))

  // Cards: union by id, newest updatedAt wins; drop if deck gone or tombstoned.
  const cardById = new Map<string, CardRecord>()
  for (const c of [...remote.cards, ...local.cards]) {
    const prev = cardById.get(c.id)
    if (!prev || c.updatedAt > prev.updatedAt) cardById.set(c.id, c)
  }
  const mergedCards: CardRecord[] = []
  for (const c of cardById.values()) {
    if (!liveDeckIds.has(c.deckId)) continue
    const t = tombstones.get(c.id)
    if (t && t.kind === 'card' && t.deletedAt > c.updatedAt) continue
    mergedCards.push(c)
  }

  // Assets are immutable + content-addressed: union by hash, then keep only
  // those referenced by a surviving card. No last-writer-wins needed.
  const referencedHashes = new Set(
    mergedCards.flatMap((c) => [...assetRefs(c.front), ...assetRefs(c.back)]),
  )
  const assetByHash = new Map<string, AssetBlob>()
  for (const a of [...remote.assets, ...local.assets]) assetByHash.set(a.hash, a)
  const mergedAssets = [...assetByHash.values()].filter((a) => referencedHashes.has(a.hash))

  const merged: RepoSnapshot = {
    decks: mergedDecks,
    cards: mergedCards,
    tombstones: [...tombstones.values()],
    assets: mergedAssets,
  }

  const mergedDeckIds = new Set(mergedDecks.map((d) => d.id))
  const mergedCardIds = new Set(mergedCards.map((c) => c.id))
  const dbOps: DbOps = {
    upsertDecks: mergedDecks,
    upsertCards: mergedCards,
    deleteDeckIds: local.decks.filter((d) => !mergedDeckIds.has(d.id)).map((d) => d.id),
    deleteCardIds: local.cards.filter((c) => !mergedCardIds.has(c.id)).map((c) => c.id),
    tombstones: merged.tombstones,
    upsertAssets: mergedAssets,
    deleteAssetHashes: local.assets.filter((a) => !referencedHashes.has(a.hash)).map((a) => a.hash),
  }

  return { merged, dbOps }
}
