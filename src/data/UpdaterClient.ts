import { invoke } from '@tauri-apps/api/core'

export interface CliStatus {
  supported: boolean
  bundledPath: string | null
  installedPath: string | null
  managedByHomebrew: boolean
  linkedToBundled: boolean
  onPath: boolean
}

export interface UpdateInfo {
  version: string
  date?: string
  body?: string
}

export interface DownloadProgress {
  downloaded: number
  total: number | null
}

export interface PendingUpdateHandle {
  info: UpdateInfo
  /** Download and stage the update without exiting the running app. */
  downloadAndInstall: (onProgress?: (progress: DownloadProgress) => void) => Promise<void>
  /**
   * Optional finalization step invoked right before relaunch (used on Windows where
   * `update.install()` exits immediately, so background updates call `update.download()`
   * first and defer `update.install()` until the user chooses to restart).
   * Returns `true` if the installer takes over restarting/exiting the app.
   */
  finishInstall?: () => Promise<boolean>
  /** Release any native backend resource held by this update handle. */
  close?: () => Promise<void>
}

export interface UpdaterClient {
  readonly supported: boolean
  check: () => Promise<PendingUpdateHandle | null>
  relaunch: () => Promise<void>
  getCliStatus: () => Promise<CliStatus>
  installCli: () => Promise<CliStatus>
}

const UNSUPPORTED_CLI_STATUS: CliStatus = {
  supported: false,
  bundledPath: null,
  installedPath: null,
  managedByHomebrew: false,
  linkedToBundled: false,
  onPath: false,
}

const isTauriRuntime = (): boolean =>
  typeof window !== 'undefined' && ('__TAURI_INTERNALS__' in window || '__TAURI__' in window)

const isWindowsPlatform = (): boolean =>
  typeof navigator !== 'undefined' && /Win/i.test(navigator.userAgent || navigator.platform || '')

class UnsupportedUpdaterClient implements UpdaterClient {
  readonly supported = false

  async check(): Promise<PendingUpdateHandle | null> {
    return null
  }

  async relaunch(): Promise<void> {}

  async getCliStatus(): Promise<CliStatus> {
    return UNSUPPORTED_CLI_STATUS
  }

  async installCli(): Promise<CliStatus> {
    return UNSUPPORTED_CLI_STATUS
  }
}

export class TauriUpdaterClient implements UpdaterClient {
  readonly supported = true
  private readonly invokeCommand: typeof invoke

  constructor(invokeCommand: typeof invoke = invoke) {
    this.invokeCommand = invokeCommand
  }

  async check(): Promise<PendingUpdateHandle | null> {
    const { check } = await import('@tauri-apps/plugin-updater')
    const update = await check()
    if (!update) return null

    const info: UpdateInfo = {
      version: update.version,
      date: update.date,
      body: update.body,
    }

    let downloadedOnWindows = false
    let installed = false

    const handleProgress = (onProgress?: (progress: DownloadProgress) => void) => {
      let downloaded = 0
      let total: number | null = null
      return (
        event:
          | { event: 'Started'; data: { contentLength?: number } }
          | { event: 'Progress'; data: { chunkLength: number } }
          | { event: 'Finished' },
      ) => {
        if (event.event === 'Started') {
          total = event.data.contentLength ?? null
          onProgress?.({ downloaded, total })
        } else if (event.event === 'Progress') {
          downloaded += event.data.chunkLength
          onProgress?.({ downloaded, total })
        } else if (event.event === 'Finished') {
          onProgress?.({ downloaded: total ?? downloaded, total })
        }
      }
    }

    return {
      info,
      downloadAndInstall: async (onProgress) => {
        const listener = handleProgress(onProgress)
        if (isWindowsPlatform()) {
          // On Windows, `install()` immediately exits the app to run the installer.
          // Download safely in the background and defer `install()` until user-initiated restart.
          await update.download(listener)
          downloadedOnWindows = true
        } else {
          await update.downloadAndInstall(listener)
          installed = true
        }
      },
      finishInstall: async () => {
        if (downloadedOnWindows && !installed) {
          await update.install({ restartAfterInstall: true })
          installed = true
          return true
        }
        return false
      },
      close: async () => {
        await update.close().catch(() => {})
      },
    }
  }

  async relaunch(): Promise<void> {
    await this.invokeCommand('app_relaunch')
  }

  async getCliStatus(): Promise<CliStatus> {
    return this.invokeCommand<CliStatus>('cli_status')
  }

  async installCli(): Promise<CliStatus> {
    return this.invokeCommand<CliStatus>('cli_install')
  }
}

export const defaultUpdaterClient: UpdaterClient = isTauriRuntime()
  ? new TauriUpdaterClient()
  : new UnsupportedUpdaterClient()
