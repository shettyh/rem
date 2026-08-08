import type { AssetBlob } from './snapshot'

export interface CommitPushResult {
  pushed: boolean
  /** True when the push was a non-fast-forward (remote advanced); the caller retries. */
  rejected: boolean
}

export interface FetchResetResult {
  /** False when the remote has no `main` branch yet (fresh/empty repo). */
  remoteExists: boolean
}

/** Git transport confined by each adapter to its own working copy. */
export interface GitBridge {
  isCloned(): Promise<boolean>
  clone(remoteUrl: string): Promise<void>
  setRemoteUrl(remoteUrl: string): Promise<void>
  fetchReset(): Promise<FetchResetResult>
  readFiles(): Promise<Record<string, string>>
  writeFiles(files: Record<string, string>): Promise<void>
  commitPush(message: string): Promise<CommitPushResult>
  /** Binary asset files under assets/, as content-addressed blobs. */
  readAssets(): Promise<AssetBlob[]>
  /** Replace the assets/ set with `assets` (delete-absent), matching writeFiles. */
  writeAssets(assets: AssetBlob[]): Promise<void>
}
