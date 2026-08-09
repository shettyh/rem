import { invoke } from '@tauri-apps/api/core'
import type { Asset, Card, Deck, ID, ReviewLog, SchedulerKind } from '../domain/models'
import type { DeckBackup } from './backup'
import type {
  ApplyMergeResult,
  CardPatch,
  DeckPatch,
  ImportResult,
  ReviewCommit,
  Storage,
  VersionedRepoSnapshot,
} from './Storage'
import type { DbOps } from './sync/merge'
import type { AssetBlob, RepoSnapshot } from './sync/snapshot'

export type StorageInvoke = <T>(
  command: string,
  args?: Record<string, unknown>,
) => Promise<T>

type NativeAsset = Omit<Asset, 'bytes'> & { bytes: number[] }
type NativeAssetBlob = Omit<AssetBlob, 'bytes'> & { bytes: number[] }
type NativeSnapshot = Omit<RepoSnapshot, 'assets'> & { assets: NativeAssetBlob[] }

type NativeVersionedSnapshot = {
  snapshot: NativeSnapshot
  revision: number
}

/** Native production adapter. All persistence rules remain in rem-core. */
export class TauriStorage implements Storage {
  private readonly listeners = new Set<() => void>()

  constructor(
    private readonly call: StorageInvoke = invoke,
    private readonly now: () => number = Date.now,
  ) {}

  subscribe(listener: () => void): () => void {
    this.listeners.add(listener)
    return () => this.listeners.delete(listener)
  }

  async createDeck(name: string, _kind: SchedulerKind = 'fsrs'): Promise<Deck> {
    const deck = await this.call<Deck>('storage_create_deck', { name, now: this.now() })
    this.notify()
    return deck
  }

  listDecks(): Promise<Deck[]> {
    return this.call('storage_list_decks')
  }

  async getDeck(id: ID): Promise<Deck | undefined> {
    return (await this.call<Deck | null>('storage_get_deck', { id })) ?? undefined
  }

  async deleteDeck(id: ID): Promise<void> {
    await this.call('storage_delete_deck', { id, now: this.now() })
    this.notify()
  }

  async updateDeck(id: ID, patch: DeckPatch): Promise<void> {
    await this.call('storage_update_deck', { id, patch, now: this.now() })
    this.notify()
  }

  async createCard(
    deckId: ID,
    front: string,
    back: string,
    tags: string[] = [],
  ): Promise<Card> {
    const card = await this.call<Card>('storage_create_card', {
      deckId,
      front,
      back,
      tags,
      now: this.now(),
    })
    this.notify()
    return card
  }

  async getCard(id: ID): Promise<Card | undefined> {
    return (await this.call<Card | null>('storage_get_card', { id })) ?? undefined
  }

  listCards(deckId: ID): Promise<Card[]> {
    return this.call('storage_list_cards', { deckId })
  }

  async updateCard(id: ID, patch: CardPatch): Promise<void> {
    await this.call('storage_update_card', { id, patch, now: this.now() })
    this.notify()
  }

  async deleteCard(id: ID): Promise<void> {
    await this.call('storage_delete_card', { id, now: this.now() })
    this.notify()
  }

  async commitReview(commit: ReviewCommit): Promise<ReviewLog | null> {
    const log = await this.call<ReviewLog | null>('storage_commit_review', { commit })
    this.notify()
    return log
  }

  listReviewLogs(deckId: ID): Promise<ReviewLog[]> {
    return this.call('storage_list_review_logs', { deckId })
  }

  dueCards(deckId: ID, now: number): Promise<Card[]> {
    return this.call('storage_due_cards', { deckId, now })
  }

  countDue(deckId: ID, now: number): Promise<number> {
    return this.call('storage_count_due', { deckId, now })
  }

  getDailyStat(
    deckId: ID,
    day: string,
  ): Promise<{ newIntroduced: number; reviewsDone: number }> {
    return this.call('storage_get_daily_stat', { deckId, day })
  }

  async importDecks(decks: DeckBackup[]): Promise<ImportResult> {
    const result = await this.call<ImportResult>('storage_import_decks', {
      decks,
      now: this.now(),
    })
    this.notify()
    return result
  }

  async exportSnapshot(): Promise<VersionedRepoSnapshot> {
    const result = await this.call<NativeVersionedSnapshot>('storage_export_snapshot')
    return {
      revision: result.revision,
      snapshot: {
        ...result.snapshot,
        assets: result.snapshot.assets.map(fromNativeAssetBlob),
      },
    }
  }

  async applyMerge(ops: DbOps, expectedRevision: number): Promise<ApplyMergeResult> {
    const result = await this.call<ApplyMergeResult>('storage_apply_merge', {
      operations: {
        ...ops,
        upsertAssets: ops.upsertAssets.map(toNativeAssetBlob),
      },
      expectedRevision,
      now: this.now(),
    })
    if (result.status === 'applied') this.notify()
    return result
  }

  async putAsset(bytes: Uint8Array, mime: string): Promise<Asset> {
    const asset = await this.call<NativeAsset>('storage_put_asset', {
      bytes: Array.from(bytes),
      mime,
      now: this.now(),
    })
    this.notify()
    return fromNativeAsset(asset)
  }

  async getAsset(hash: ID): Promise<Asset | undefined> {
    const asset = await this.call<NativeAsset | null>('storage_get_asset', { hash })
    return asset ? fromNativeAsset(asset) : undefined
  }

  async sweepOrphanAssets(): Promise<void> {
    await this.call<number>('storage_sweep_orphan_assets')
    this.notify()
  }

  private notify(): void {
    for (const listener of this.listeners) listener()
  }
}

function fromNativeAsset(asset: NativeAsset): Asset {
  return { ...asset, bytes: new Uint8Array(asset.bytes) }
}

function fromNativeAssetBlob(asset: NativeAssetBlob): AssetBlob {
  return { ...asset, bytes: new Uint8Array(asset.bytes) }
}

function toNativeAssetBlob(asset: AssetBlob): NativeAssetBlob {
  return { ...asset, bytes: Array.from(asset.bytes) }
}
