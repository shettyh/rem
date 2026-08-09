import type { Storage } from '../Storage'
import type { GitBridge } from './GitBridge'
import { merge } from './merge'
import {
  deserializeSnapshot,
  serializeSnapshot,
  EMPTY_SNAPSHOT,
  type RepoSnapshot,
} from './snapshot'

export interface SyncConfig {
  remoteUrl: string
}

export interface SyncOutcome {
  pushed: boolean
}

const MAX_PUSH_ATTEMPTS = 5
const MAX_LOCAL_MERGE_ATTEMPTS = 5

/** Orchestrates the sync protocol: reset working copy to remote, merge in-app,
 *  write back, commit, push — retrying if the remote advanced mid-sync. */
export class GitSyncService {
  constructor(
    private readonly storage: Storage,
    private readonly bridge: GitBridge,
    private readonly config: SyncConfig,
  ) {}

  async sync(): Promise<SyncOutcome> {
    const { remoteUrl } = this.config
    if (!(await this.bridge.isCloned())) {
      await this.bridge.clone(remoteUrl)
    } else {
      await this.bridge.setRemoteUrl(remoteUrl)
    }
    for (let attempt = 0; attempt < MAX_PUSH_ATTEMPTS; attempt++) {
      const { remoteExists } = await this.bridge.fetchReset()
      const remote = remoteExists
        ? {
            ...deserializeSnapshot(await this.bridge.readFiles()),
            assets: await this.bridge.readAssets(),
          }
        : EMPTY_SNAPSHOT
      const merged = await this.mergeIntoLocal(remote)
      await this.bridge.writeFiles(serializeSnapshot(merged))
      await this.bridge.writeAssets(merged.assets)
      const { pushed, rejected } = await this.bridge.commitPush(
        `sync ${new Date().toISOString()}`,
      )
      if (!rejected) return { pushed }
    }
    throw new Error('Sync failed: the remote kept changing during push. Try again.')
  }

  private async mergeIntoLocal(remote: RepoSnapshot): Promise<RepoSnapshot> {
    for (let attempt = 0; attempt < MAX_LOCAL_MERGE_ATTEMPTS; attempt++) {
      const { snapshot: local, revision } = await this.storage.exportSnapshot()
      const { merged, dbOps } = merge(local, remote)
      const result = await this.storage.applyMerge(dbOps, revision)
      if (result.status === 'applied') return merged
    }
    throw new Error('Sync failed: the local collection kept changing. Try again.')
  }
}
