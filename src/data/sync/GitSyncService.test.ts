import { describe, it, expect, beforeEach, afterEach } from 'vitest'
import Dexie from 'dexie'
import { RemDB } from '../dexie/db'
import { DexieStorage } from '../dexie/DexieStorage'
import { FakeGitBridge } from './FakeGitBridge'
import { GitSyncService } from './GitSyncService'
import { serializeSnapshot } from './snapshot'

const DB = 'rem-sync-service-test'
let db: RemDB
let storage: DexieStorage

beforeEach(async () => {
  await Dexie.delete(DB)
  db = new RemDB(DB)
  storage = new DexieStorage(db)
})
afterEach(() => db.close())

const cfg = { remoteUrl: 'url', repoDir: 'dir' }

describe('GitSyncService', () => {
  it('pushes local data to an empty remote on first sync', async () => {
    const deck = await storage.createDeck('S')
    await storage.createCard(deck.id, 'q', 'a')
    const bridge = new FakeGitBridge(null)
    await new GitSyncService(storage, bridge, cfg).sync()
    expect(bridge.remote).not.toBeNull()
    expect(Object.keys(bridge.remote!)).toContain(`decks/${deck.id}.json`)
  })

  it('pulls a remote-only deck into the local store', async () => {
    const remote = serializeSnapshot({
      decks: [{ id: 'd1', name: 'Remote', createdAt: 1, schedulerKind: 'sm2' }],
      cards: [],
      tombstones: [],
      assets: [],
    })
    const bridge = new FakeGitBridge(remote)
    await new GitSyncService(storage, bridge, cfg).sync()
    const decks = await storage.listDecks()
    expect(decks.map((d) => d.name)).toEqual(['Remote'])
  })

  it('applies a remote tombstone to delete a local card', async () => {
    const deck = await storage.createDeck('S')
    const c = await storage.createCard(deck.id, 'q', 'a')
    const remote = serializeSnapshot({
      decks: [{ id: deck.id, name: 'S', createdAt: deck.createdAt, schedulerKind: 'sm2' }],
      cards: [],
      tombstones: [{ id: c.id, kind: 'card', deletedAt: Date.now() + 10000 }],
      assets: [],
    })
    const bridge = new FakeGitBridge(remote)
    await new GitSyncService(storage, bridge, cfg).sync()
    expect(await storage.getCard(c.id)).toBeUndefined()
  })

  it('retries when the push is rejected, then succeeds', async () => {
    await storage.createDeck('S')
    const bridge = new FakeGitBridge({ 'rem.json': '{}' })
    bridge.pushInterceptor = () => {
      bridge.remote = { 'rem.json': '{}', 'tombstones.json': '[]' }
      bridge.bumpRemote()
    }
    const outcome = await new GitSyncService(storage, bridge, cfg).sync()
    expect(outcome.pushed).toBe(true)
  })

  it('is idempotent: a second sync leaves remote and local unchanged', async () => {
    const deck = await storage.createDeck('S')
    await storage.createCard(deck.id, 'q', 'a')
    const bridge = new FakeGitBridge(null)
    const service = new GitSyncService(storage, bridge, cfg)
    await service.sync()
    const remoteAfterFirst = JSON.stringify(bridge.remote)
    const decksAfterFirst = await storage.listDecks()
    const cardsAfterFirst = await storage.listCards(deck.id)
    await service.sync()
    expect(JSON.stringify(bridge.remote)).toBe(remoteAfterFirst)
    expect(await storage.listDecks()).toEqual(decksAfterFirst)
    expect(await storage.listCards(deck.id)).toEqual(cardsAfterFirst)
  })

  it('syncs an image asset from one machine to another', async () => {
    const bridge = new FakeGitBridge(null)
    // Machine A: a deck with a card embedding an asset.
    const deck = await storage.createDeck('D', 'fsrs')
    const asset = await storage.putAsset(new Uint8Array([3, 1, 4]), 'image/png')
    await storage.createCard(deck.id, `![x](asset:${asset.hash})`, 'back')
    await new GitSyncService(storage, bridge, cfg).sync()

    // Machine B: a fresh store syncs from the same remote.
    const DB_B = 'rem-sync-service-test-b'
    await Dexie.delete(DB_B)
    const dbB = new RemDB(DB_B)
    const storageB = new DexieStorage(dbB)
    try {
      await new GitSyncService(storageB, bridge, cfg).sync()
      const got = await storageB.getAsset(asset.hash)
      expect(got?.bytes).toEqual(new Uint8Array([3, 1, 4]))
    } finally {
      dbB.close()
      await Dexie.delete(DB_B)
    }
  })
})
