import type { CommitPushResult, FetchResetResult, GitBridge } from './GitBridge'

/** In-memory GitBridge for tests. `remote === null` models an empty remote
 *  (no `main` yet). `pushInterceptor` fires once at the next commitPush to
 *  simulate a concurrent push (forcing a rejection). */
export class FakeGitBridge implements GitBridge {
  remote: Record<string, string> | null
  pushInterceptor: (() => void) | null = null
  private working: Record<string, string> = {}
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

  async clone(_remoteUrl: string, _dir: string): Promise<void> {
    this.cloned = true
    this.working = this.remote ? { ...this.remote } : {}
  }

  async fetchReset(_dir: string): Promise<FetchResetResult> {
    this.working = this.remote ? { ...this.remote } : {}
    this.fetchedVersion = this.remoteVersion
    return { remoteExists: this.remote !== null }
  }

  async readFiles(_dir: string): Promise<Record<string, string>> {
    return { ...this.working }
  }

  async writeFiles(_dir: string, files: Record<string, string>): Promise<void> {
    this.working = { ...files }
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
    this.remoteVersion++
    this.fetchedVersion = this.remoteVersion
    return { pushed: true, rejected: false }
  }
}
