import { invoke } from '@tauri-apps/api/core'
import type { CommitPushResult, FetchResetResult, GitBridge } from './GitBridge'
import type { AssetBlob } from './snapshot'
import { assetFileName, assetFileToBlob, base64FromBytes } from './assetFile'

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

  async readAssets(dir: string): Promise<AssetBlob[]> {
    const files = await invoke<{ name: string; data: string }[]>('git_read_assets', { dir })
    return files.map((f) => assetFileToBlob(f.name, f.data))
  }

  async writeAssets(dir: string, assets: AssetBlob[]): Promise<void> {
    const files = assets.map((a) => ({ name: assetFileName(a), data: base64FromBytes(a.bytes) }))
    await invoke<void>('git_write_assets', { dir, files })
  }
}
