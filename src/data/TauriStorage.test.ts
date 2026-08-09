import { describe, expect, it } from 'vitest'
import type { Deck } from '../domain/models'
import type { DbOps } from './sync/merge'
import { TauriStorage, type StorageInvoke } from './TauriStorage'

const deck: Deck = {
  id: 'd1',
  name: 'Rust',
  createdAt: 1,
  updatedAt: 1,
  color: '#7e6cff',
  schedulerKind: 'fsrs',
  settings: {
    newPerDay: 20,
    maxReviews: 200,
    learnSteps: '1m 10m',
    insertionOrder: 'sequential',
    relearnSteps: '10m',
    minimumInterval: 1,
    leechThreshold: 8,
    leechAction: 'suspend',
    buryRelated: true,
    showTimer: false,
    desiredRetention: 0.9,
    maximumInterval: 36500,
    fsrsWeights: null,
  },
}

function fakeInvoke(
  respond: (command: string, args: Record<string, unknown> | undefined) => unknown,
): { invoke: StorageInvoke; calls: Array<[string, Record<string, unknown> | undefined]> } {
  const calls: Array<[string, Record<string, unknown> | undefined]> = []
  const invoke: StorageInvoke = async <T>(command: string, args?: Record<string, unknown>) => {
    calls.push([command, args])
    return respond(command, args) as T
  }
  return { invoke, calls }
}

const emptyOps: DbOps = {
  upsertDecks: [],
  upsertCards: [],
  upsertReviewLogs: [],
  deleteReviewLogIds: [],
  deleteDeckIds: [],
  deleteCardIds: [],
  tombstones: [],
  upsertAssets: [],
  deleteAssetHashes: [],
}

describe('TauriStorage', () => {
  it('forwards mutations to typed commands and notifies subscribers after success', async () => {
    const fake = fakeInvoke((command) => {
      if (command === 'storage_create_deck') return deck
      if (command === 'storage_list_decks') return [deck]
      throw new Error(`unexpected command ${command}`)
    })
    const storage = new TauriStorage(fake.invoke, () => 123)
    let notifications = 0
    storage.subscribe(() => notifications++)

    expect(await storage.createDeck('Rust')).toEqual(deck)
    expect(await storage.listDecks()).toEqual([deck])
    expect(fake.calls).toEqual([
      ['storage_create_deck', { name: 'Rust', now: 123 }],
      ['storage_list_decks', undefined],
    ])
    expect(notifications).toBe(1)
  })

  it('maps native null option values to undefined', async () => {
    const fake = fakeInvoke(() => null)
    const storage = new TauriStorage(fake.invoke)

    expect(await storage.getDeck('missing')).toBeUndefined()
    expect(await storage.getCard('missing')).toBeUndefined()
    expect(await storage.getAsset('missing')).toBeUndefined()
  })

  it('maps Uint8Array asset bytes across JSON IPC', async () => {
    const nativeAsset = { hash: 'a'.repeat(64), mime: 'image/png', bytes: [1, 2, 3], createdAt: 9 }
    const fake = fakeInvoke((command, args) => {
      if (command === 'storage_put_asset') {
        expect(args).toEqual({ bytes: [1, 2, 3], mime: 'image/png', now: 123 })
        return nativeAsset
      }
      if (command === 'storage_get_asset') return nativeAsset
      throw new Error(`unexpected command ${command}`)
    })
    const storage = new TauriStorage(fake.invoke, () => 123)

    const created = await storage.putAsset(new Uint8Array([1, 2, 3]), 'image/png')
    const loaded = await storage.getAsset(nativeAsset.hash)

    expect(created.bytes).toEqual(new Uint8Array([1, 2, 3]))
    expect(loaded?.bytes).toEqual(new Uint8Array([1, 2, 3]))
  })

  it('applies a merge against the revision observed during export', async () => {
    const fake = fakeInvoke((command, args) => {
      if (command === 'storage_export_snapshot') {
        return {
          snapshot: { decks: [], cards: [], reviewLogs: [], tombstones: [], assets: [] },
          revision: 7,
        }
      }
      if (command === 'storage_apply_merge') {
        expect(args).toEqual({ operations: emptyOps, expectedRevision: 7, now: 123 })
        return { status: 'applied', revision: 8 }
      }
      throw new Error(`unexpected command ${command}`)
    })
    const storage = new TauriStorage(fake.invoke, () => 123)

    const exported = await storage.exportSnapshot()
    expect(exported.revision).toBe(7)
    await expect(storage.applyMerge(emptyOps, exported.revision)).resolves.toEqual({
      status: 'applied',
      revision: 8,
    })
  })

  it('returns a stale merge result without notifying', async () => {
    const fake = fakeInvoke((command) => {
      if (command === 'storage_export_snapshot') {
        return {
          snapshot: { decks: [], cards: [], reviewLogs: [], tombstones: [], assets: [] },
          revision: 7,
        }
      }
      if (command === 'storage_apply_merge') return { status: 'stale', currentRevision: 8 }
      throw new Error(`unexpected command ${command}`)
    })
    const storage = new TauriStorage(fake.invoke, () => 123)
    let notifications = 0
    storage.subscribe(() => notifications++)

    const exported = await storage.exportSnapshot()
    await expect(storage.applyMerge(emptyOps, exported.revision)).resolves.toEqual({
      status: 'stale',
      currentRevision: 8,
    })
    expect(notifications).toBe(0)
  })
})
