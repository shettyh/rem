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

  async isCloned(): Promise<boolean> {
    return this.cloned
  }

  async clone(remoteUrl: string): Promise<void> {
    this.cloned = true
    this.remoteUrl = remoteUrl
    this.working = this.remote ? { ...this.remote } : {}
    this.workingAssets = [...this.remoteAssets]
  }

  async setRemoteUrl(remoteUrl: string): Promise<void> {
    this.remoteUrl = remoteUrl
  }

  async fetchReset(): Promise<FetchResetResult> {
    this.working = this.remote ? { ...this.remote } : {}
    this.workingAssets = [...this.remoteAssets]
    this.fetchedVersion = this.remoteVersion
    return { remoteExists: this.remote !== null }
  }

  async readFiles(): Promise<Record<string, string>> {
    return { ...this.working }
  }

  async writeFiles(files: Record<string, string>): Promise<void> {
    this.working = { ...files }
  }

  async readAssets(): Promise<AssetBlob[]> {
    return [...this.workingAssets]
  }

  async writeAssets(assets: AssetBlob[]): Promise<void> {
    this.workingAssets = [...assets]
  }

  async commitPush(_message: string): Promise<CommitPushResult> {
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
