import type { Storage } from '../Storage'
import type { GitBridge } from './GitBridge'
import { merge } from './merge'
import { deserializeSnapshot, serializeSnapshot, EMPTY_SNAPSHOT } from './snapshot'

export interface SyncConfig {
  remoteUrl: string
  repoDir: string
}

export interface SyncOutcome {
  pushed: boolean
}

const MAX_PUSH_ATTEMPTS = 5

/** Orchestrates the sync protocol: reset working copy to remote, merge in-app,
 *  write back, commit, push — retrying if the remote advanced mid-sync. */
export class GitSyncService {
  constructor(
    private readonly storage: Storage,
    private readonly bridge: GitBridge,
    private readonly config: SyncConfig,
  ) {}

  async sync(): Promise<SyncOutcome> {
    const { remoteUrl, repoDir } = this.config
    if (!(await this.bridge.isCloned(repoDir))) {
      await this.bridge.clone(remoteUrl, repoDir)
    }
    for (let attempt = 0; attempt < MAX_PUSH_ATTEMPTS; attempt++) {
      const { remoteExists } = await this.bridge.fetchReset(repoDir)
      const remote = remoteExists
        ? deserializeSnapshot(await this.bridge.readFiles(repoDir))
        : EMPTY_SNAPSHOT
      const local = await this.storage.exportSnapshot()
      const { merged, dbOps } = merge(local, remote)
      await this.storage.applyMerge(dbOps)
      await this.bridge.writeFiles(repoDir, serializeSnapshot(merged))
      const { pushed, rejected } = await this.bridge.commitPush(
        repoDir,
        `sync ${new Date().toISOString()}`,
      )
      if (!rejected) return { pushed }
    }
    throw new Error('Sync failed: the remote kept changing during push. Try again.')
  }
}
