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

  it('forwards draft proposal, listing, and resolution through native commands', async () => {
    const draft = {
      id: 'draft-1',
      deckId: deck.id,
      front: 'Question',
      back: 'Answer',
      tags: ['rust'],
      rationale: 'Worth remembering',
      sources: [{ locator: 'src/lib.rs:1-5', label: 'Core' }],
      proposedBy: 'pi',
      createdAt: 123,
      updatedAt: 123,
      revision: 0,
    }
    const fake = fakeInvoke((command) => {
      if (command === 'storage_propose_drafts') {
        return { outcomes: [{ status: 'created', value: draft }] }
      }
      if (command === 'storage_list_drafts') return [draft]
      if (command === 'storage_resolve_draft') return { status: 'rejected' }
      throw new Error(`unexpected command ${command}`)
    })
    const storage = new TauriStorage(fake.invoke, () => 123)
    let notifications = 0
    storage.subscribe(() => notifications++)
    const input = {
      front: draft.front,
      back: draft.back,
      tags: draft.tags,
      rationale: draft.rationale,
      sources: draft.sources,
    }

    await expect(storage.proposeDrafts(deck.id, [input], { proposedBy: 'pi' })).resolves.toEqual({
      outcomes: [{ status: 'created', value: draft }],
    })
    await expect(storage.listDrafts()).resolves.toEqual([draft])
    await expect(storage.resolveDraft(draft.id, 0, { decision: 'reject' })).resolves.toEqual({
      status: 'rejected',
    })
    expect(fake.calls).toEqual([
      ['storage_propose_drafts', {
        deckId: deck.id,
        inputs: [input],
        metadata: { proposedBy: 'pi' },
        now: 123,
        dryRun: false,
      }],
      ['storage_list_drafts', undefined],
      ['storage_resolve_draft', {
        draftId: draft.id,
        expectedRevision: 0,
        decision: { decision: 'reject' },
        now: 123,
      }],
    ])
    expect(notifications).toBe(2)
  })

  it('forwards the opaque native study session lifecycle', async () => {
    const view = {
      current: null,
      revealed: false,
      nextStates: null,
      reviewed: 0,
      remaining: 0,
      preview: false,
      notice: null,
    }
    const fake = fakeInvoke((command) => {
      if (command === 'study_start') return { sessionId: 'study-1', view }
      if (command === 'study_reveal') return { ...view, revealed: true }
      if (command === 'study_grade') return { status: 'graded', view }
      if (command === 'study_advance_preview') return view
      if (command === 'study_end') return undefined
      throw new Error(`unexpected command ${command}`)
    })
    const storage = new TauriStorage(fake.invoke, () => 123)
    let notifications = 0
    storage.subscribe(() => notifications++)
    const request = { deckId: deck.id, custom: null }

    await expect(storage.startStudy(request)).resolves.toEqual({ sessionId: 'study-1', view })
    await storage.revealStudy('study-1')
    await storage.gradeStudy('study-1', 'good')
    await storage.advanceStudyPreview('study-1')
    await storage.endStudy('study-1')

    expect(fake.calls).toEqual([
      ['study_start', { request, now: 123 }],
      ['study_reveal', { sessionId: 'study-1', now: 123 }],
      ['study_grade', { sessionId: 'study-1', grade: 'good', now: 123 }],
      ['study_advance_preview', { sessionId: 'study-1', now: 123 }],
      ['study_end', { sessionId: 'study-1' }],
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
