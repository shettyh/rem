import type { CommitPushResult, FetchResetResult, GitBridge } from './GitBridge'
import type { AssetBlob } from './snapshot'

/** In-memory GitBridge for tests. `remote === null` models an empty remote
 *  (no `main` yet). `pushInterceptor` fires once at the next commitPush to
 *  simulate a concurrent push (forcing a rejection). */
export class FakeGitBridge implements GitBridge {
  remote: Record<string, string> | null
  remoteUrl: string | null = null
  remoteAssets: AssetBlob[] = []
  pushInterceptor: (() => void) | null = null
  private working: Record<string, string> = {}
  private workingAssets: AssetBlob[] = []
  private cloned = false
  private remoteVersion = 0
  private fetchedVersion = -1

  constructor(remote: Record<string, string> | null = null) {
    this.remote = remote
    if (remote) this.remoteVersion = 1
  }

  /** Test helper: mark the remote as advanced (used inside pushInterceptor). */
  bumpRemote(): void {
    this.remoteVersion++
  }

  async isCloned(_dir: string): Promise<boolean> {
    return this.cloned
  }

  async clone(remoteUrl: string, _dir: string): Promise<void> {
    this.cloned = true
    this.remoteUrl = remoteUrl
    this.working = this.remote ? { ...this.remote } : {}
    this.workingAssets = [...this.remoteAssets]
  }

  async setRemoteUrl(remoteUrl: string, _dir: string): Promise<void> {
    this.remoteUrl = remoteUrl
  }

  async fetchReset(_dir: string): Promise<FetchResetResult> {
    this.working = this.remote ? { ...this.remote } : {}
    this.workingAssets = [...this.remoteAssets]
    this.fetchedVersion = this.remoteVersion
    return { remoteExists: this.remote !== null }
  }

  async readFiles(_dir: string): Promise<Record<string, string>> {
    return { ...this.working }
  }

  async writeFiles(_dir: string, files: Record<string, string>): Promise<void> {
    this.working = { ...files }
  }

  async readAssets(_dir: string): Promise<AssetBlob[]> {
    return [...this.workingAssets]
  }

  async writeAssets(_dir: string, assets: AssetBlob[]): Promise<void> {
    this.workingAssets = [...assets]
  }

  async commitPush(_dir: string, _message: string): Promise<CommitPushResult> {
    if (this.pushInterceptor) {
      const fn = this.pushInterceptor
      this.pushInterceptor = null
      fn()
    }
    if (this.remoteVersion !== this.fetchedVersion) {
      return { pushed: false, rejected: true }
    }
    this.remote = { ...this.working }
    this.remoteAssets = [...this.workingAssets]
    this.remoteVersion++
    this.fetchedVersion = this.remoteVersion
    return { pushed: true, rejected: false }
  }
}
