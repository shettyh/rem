import { invoke } from '@tauri-apps/api/core'
import type { CommitPushResult, FetchResetResult, GitBridge } from './GitBridge'
import type { AssetBlob } from './snapshot'
import { assetFileName, assetFileToBlob, base64FromBytes } from './assetFile'

/** Real GitBridge: forwards each call to the app-confined Rust commands. */
export class TauriGitBridge implements GitBridge {
  isCloned(): Promise<boolean> {
    return invoke<boolean>('git_is_cloned')
  }

  clone(remoteUrl: string): Promise<void> {
    return invoke<void>('git_clone', { remoteUrl })
  }

  setRemoteUrl(remoteUrl: string): Promise<void> {
    return invoke<void>('git_set_remote_url', { remoteUrl })
  }

  async fetchReset(): Promise<FetchResetResult> {
    const remoteExists = await invoke<boolean>('git_fetch_reset')
    return { remoteExists }
  }

  readFiles(): Promise<Record<string, string>> {
    return invoke<Record<string, string>>('git_read_files')
  }

  writeFiles(files: Record<string, string>): Promise<void> {
    return invoke<void>('git_write_files', { files })
  }

  commitPush(message: string): Promise<CommitPushResult> {
    return invoke<CommitPushResult>('git_commit_push', { message })
  }

  async readAssets(): Promise<AssetBlob[]> {
    const files = await invoke<{ name: string; data: string }[]>('git_read_assets')
    return files.map((f) => assetFileToBlob(f.name, f.data))
  }

  async writeAssets(assets: AssetBlob[]): Promise<void> {
    const files = assets.map((a) => ({ name: assetFileName(a), data: base64FromBytes(a.bytes) }))
    await invoke<void>('git_write_assets', { files })
  }
}
