export interface CommitPushResult {
  pushed: boolean
  /** True when the push was a non-fast-forward (remote advanced); the caller retries. */
  rejected: boolean
}

export interface FetchResetResult {
  /** False when the remote has no `main` branch yet (fresh/empty repo). */
  remoteExists: boolean
}

/** Dumb git transport. Implementations: {@link ./TauriGitBridge} (real) and
 *  {@link ./FakeGitBridge} (tests). All paths are absolute working-copy dirs. */
export interface GitBridge {
  isCloned(dir: string): Promise<boolean>
  clone(remoteUrl: string, dir: string): Promise<void>
  fetchReset(dir: string): Promise<FetchResetResult>
  readFiles(dir: string): Promise<Record<string, string>>
  writeFiles(dir: string, files: Record<string, string>): Promise<void>
  commitPush(dir: string, message: string): Promise<CommitPushResult>
}
