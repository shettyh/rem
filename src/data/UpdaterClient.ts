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
  downloadAndInstall: (onProgress?: (progress: DownloadProgress) => void) => Promise<void>
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

class TauriUpdaterClient implements UpdaterClient {
  readonly supported = true

  async check(): Promise<PendingUpdateHandle | null> {
    const { check } = await import('@tauri-apps/plugin-updater')
    const update = await check()
    if (!update) return null

    const info: UpdateInfo = {
      version: update.version,
      date: update.date,
      body: update.body,
    }

    return {
      info,
      downloadAndInstall: async (onProgress) => {
        let downloaded = 0
        let total: number | null = null
        await update.downloadAndInstall((event) => {
          if (event.event === 'Started') {
            total = event.data.contentLength ?? null
            onProgress?.({ downloaded, total })
          } else if (event.event === 'Progress') {
            downloaded += event.data.chunkLength
            onProgress?.({ downloaded, total })
          } else if (event.event === 'Finished') {
            onProgress?.({ downloaded: total ?? downloaded, total })
          }
        })
      },
    }
  }

  async relaunch(): Promise<void> {
    const { relaunch } = await import('@tauri-apps/plugin-process')
    await relaunch()
  }

  async getCliStatus(): Promise<CliStatus> {
    return invoke<CliStatus>('cli_status')
  }

  async installCli(): Promise<CliStatus> {
    return invoke<CliStatus>('cli_install')
  }
}

export const defaultUpdaterClient: UpdaterClient = isTauriRuntime()
  ? new TauriUpdaterClient()
  : new UnsupportedUpdaterClient()
