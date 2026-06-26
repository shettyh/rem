import { describe, it, expect } from 'vitest'
import { merge } from './merge'
import type { RepoSnapshot, CardRecord, DeckRecord, AssetBlob } from './snapshot'

const deck: DeckRecord = { id: 'd1', name: 'D', createdAt: 1, schedulerKind: 'sm2' }
function card(id: string, updatedAt: number, front = 'f'): CardRecord {
  return {
    id, deckId: 'd1', front, back: 'b', createdAt: 1, updatedAt,
    scheduling: { kind: 'sm2', repetitions: 0, intervalDays: 0, easeFactor: 2.5, due: 0 },
  }
}
function snap(p: Partial<RepoSnapshot>): RepoSnapshot {
  return { decks: [], cards: [], tombstones: [], assets: [], ...p }
}
const H = 'a'.repeat(64)
function imgCard(id: string, hash: string): CardRecord {
  return {
    id, deckId: 'd1', front: `![x](asset:${hash})`, back: 'b', createdAt: 1, updatedAt: 10,
    scheduling: { kind: 'sm2', repetitions: 0, intervalDays: 0, easeFactor: 2.5, due: 0 },
  }
}
const blob = (hash: string): AssetBlob => ({ hash, mime: 'image/png', bytes: new Uint8Array([1]) })

describe('merge', () => {
  it('unions cards edited on different sides', () => {
    const local = snap({ decks: [deck], cards: [card('a', 10)] })
    const remote = snap({ decks: [deck], cards: [card('b', 10)] })
    const { merged } = merge(local, remote)
    expect(merged.cards.map((c) => c.id).sort()).toEqual(['a', 'b'])
  })

  it('keeps the newer edit for the same card (LWW)', () => {
    const local = snap({ decks: [deck], cards: [card('a', 20, 'local')] })
    const remote = snap({ decks: [deck], cards: [card('a', 10, 'remote')] })
    const { merged } = merge(local, remote)
    expect(merged.cards).toHaveLength(1)
    expect(merged.cards[0].front).toBe('local')
  })

  it('a tombstone newer than the edit deletes the card', () => {
    const local = snap({ decks: [deck], cards: [card('a', 10)] })
    const remote = snap({ decks: [deck], tombstones: [{ id: 'a', kind: 'card', deletedAt: 20 }] })
    const { merged, dbOps } = merge(local, remote)
    expect(merged.cards).toHaveLength(0)
    expect(dbOps.deleteCardIds).toEqual(['a'])
  })

  it('an edit newer than the tombstone keeps the card', () => {
    const local = snap({ decks: [deck], cards: [card('a', 30)] })
    const remote = snap({ decks: [deck], tombstones: [{ id: 'a', kind: 'card', deletedAt: 20 }] })
    const { merged } = merge(local, remote)
    expect(merged.cards.map((c) => c.id)).toEqual(['a'])
  })

  it('a deck tombstone removes the deck and cascades to its cards', () => {
    const local = snap({ decks: [deck], cards: [card('a', 10)] })
    const remote = snap({ tombstones: [{ id: 'd1', kind: 'deck', deletedAt: 50 }] })
    const { merged, dbOps } = merge(local, remote)
    expect(merged.decks).toHaveLength(0)
    expect(merged.cards).toHaveLength(0)
    expect(dbOps.deleteDeckIds).toEqual(['d1'])
    expect(dbOps.deleteCardIds).toEqual(['a'])
  })

  it('pulls a remote-only deck into dbOps upserts', () => {
    const remote = snap({ decks: [deck], cards: [card('a', 10)] })
    const { dbOps } = merge(snap({}), remote)
    expect(dbOps.upsertDecks).toEqual([deck])
    expect(dbOps.upsertCards.map((c) => c.id)).toEqual(['a'])
  })

  it('keeps the newest tombstone when both sides have one', () => {
    const local = snap({ tombstones: [{ id: 'a', kind: 'card', deletedAt: 10 }] })
    const remote = snap({ tombstones: [{ id: 'a', kind: 'card', deletedAt: 99 }] })
    const { merged } = merge(local, remote)
    expect(merged.tombstones).toEqual([{ id: 'a', kind: 'card', deletedAt: 99 }])
  })

  it('deletes a deck when the tombstone equals its createdAt (>= boundary)', () => {
    const local = snap({ decks: [deck], cards: [card('a', 10)] })
    const remote = snap({ tombstones: [{ id: 'd1', kind: 'deck', deletedAt: deck.createdAt }] })
    const { merged } = merge(local, remote)
    expect(merged.decks).toHaveLength(0)
  })

  it('keeps the remote card when updatedAt ties (first-seen wins, remote is spread first)', () => {
    const local = snap({ decks: [deck], cards: [card('a', 10, 'local')] })
    const remote = snap({ decks: [deck], cards: [card('a', 10, 'remote')] })
    const { merged } = merge(local, remote)
    expect(merged.cards).toHaveLength(1)
    expect(merged.cards[0].front).toBe('remote')
  })

  it('keeps a card when the tombstone equals its updatedAt (strict > boundary)', () => {
    const local = snap({ decks: [deck], cards: [card('a', 20)] })
    const remote = snap({ decks: [deck], tombstones: [{ id: 'a', kind: 'card', deletedAt: 20 }] })
    const { merged } = merge(local, remote)
    expect(merged.cards.map((c) => c.id)).toEqual(['a'])
  })
})

describe('merge assets', () => {
  it('keeps an asset referenced by a merged card', () => {
    const local = snap({ decks: [deck], cards: [imgCard('a', H)], assets: [blob(H)] })
    const { merged } = merge(local, snap({ decks: [deck] }))
    expect(merged.assets.map((a) => a.hash)).toEqual([H])
  })

  it('prunes an asset referenced by no merged card', () => {
    const local = snap({ decks: [deck], cards: [], assets: [blob(H)] })
    const { merged, dbOps } = merge(local, snap({}))
    expect(merged.assets).toEqual([])
    expect(dbOps.deleteAssetHashes).toEqual([H])
  })

  it('unions asset bytes from the remote side for a referenced card', () => {
    const remote = snap({ decks: [deck], cards: [imgCard('a', H)], assets: [blob(H)] })
    const { merged, dbOps } = merge(snap({ decks: [deck] }), remote)
    expect(merged.assets.map((a) => a.hash)).toEqual([H])
    expect(dbOps.upsertAssets.map((a) => a.hash)).toEqual([H])
  })
})
