import { invoke } from '@tauri-apps/api/core'
import type { CommitPushResult, FetchResetResult, GitBridge } from './GitBridge'

/** Real GitBridge: forwards each call to the Rust commands from Task 8. */
export class TauriGitBridge implements GitBridge {
  isCloned(dir: string): Promise<boolean> {
    return invoke<boolean>('git_is_cloned', { dir })
  }

  clone(remoteUrl: string, dir: string): Promise<void> {
    return invoke<void>('git_clone', { remoteUrl, dir })
  }

  async fetchReset(dir: string): Promise<FetchResetResult> {
    const remoteExists = await invoke<boolean>('git_fetch_reset', { dir })
    return { remoteExists }
  }

  readFiles(dir: string): Promise<Record<string, string>> {
    return invoke<Record<string, string>>('git_read_files', { dir })
  }

  writeFiles(dir: string, files: Record<string, string>): Promise<void> {
    return invoke<void>('git_write_files', { dir, files })
  }

  commitPush(dir: string, message: string): Promise<CommitPushResult> {
    return invoke<CommitPushResult>('git_commit_push', { dir, message })
  }
}
