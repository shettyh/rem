import { act, render, waitFor } from '@testing-library/react'
import { afterEach, describe, expect, test, vi } from 'vitest'
import {
  TauriUpdaterClient,
  type CliStatus,
  type PendingUpdateHandle,
  type UpdaterClient,
} from './UpdaterClient'
import { UpdaterProvider, useUpdaterOptional, type UpdaterContextValue } from './UpdaterContext'
import { AUTO_UPDATE_KEY, getAutoUpdateEnabled, setAutoUpdateEnabled } from './updaterPrefs'

const DEFAULT_CLI: CliStatus = {
  supported: true,
  bundledPath: '/Applications/rem.app/Contents/MacOS/rem',
  installedPath: null,
  managedByHomebrew: false,
  linkedToBundled: false,
  onPath: false,
}

function makeClient(overrides: Partial<UpdaterClient> = {}): UpdaterClient {
  return {
    supported: true,
    check: vi.fn(async () => null),
    relaunch: vi.fn(async () => {}),
    getCliStatus: vi.fn(async () => DEFAULT_CLI),
    installCli: vi.fn(async () => DEFAULT_CLI),
    ...overrides,
  }
}

function mountUpdater(
  client: UpdaterClient,
  opts: { autoCheck?: boolean; startupDelayMs?: number; pollIntervalMs?: number } = {},
) {
  let latest: UpdaterContextValue | null = null
  function Probe() {
    latest = useUpdaterOptional()
    return null
  }
  const utils = render(
    <UpdaterProvider
      client={client}
      autoCheck={opts.autoCheck ?? true}
      startupDelayMs={opts.startupDelayMs ?? 10}
      pollIntervalMs={opts.pollIntervalMs ?? 0}
    >
      <Probe />
    </UpdaterProvider>,
  )
  return {
    ...utils,
    get current() {
      if (!latest) throw new Error('UpdaterContext not mounted')
      return latest
    },
  }
}

afterEach(() => {
  localStorage.removeItem(AUTO_UPDATE_KEY)
})

describe('updaterPrefs', () => {
  test('defaults auto-update to enabled and persists changes', () => {
    expect(getAutoUpdateEnabled()).toBe(true)
    setAutoUpdateEnabled(false)
    expect(localStorage.getItem(AUTO_UPDATE_KEY)).toBe('false')
    expect(getAutoUpdateEnabled()).toBe(false)
    setAutoUpdateEnabled(true)
    expect(localStorage.getItem(AUTO_UPDATE_KEY)).toBe('true')
    expect(getAutoUpdateEnabled()).toBe(true)
  })
})

describe('UpdaterProvider auto-update lifecycle', () => {
  test('auto-checks, downloads in background, and becomes ready when enabled', async () => {
    const downloadAndInstall = vi.fn(async (onProgress) => {
      onProgress?.({ downloaded: 50, total: 100 })
      onProgress?.({ downloaded: 100, total: 100 })
    })
    const pending: PendingUpdateHandle = {
      info: { version: '0.27.0', body: 'Auto update' },
      downloadAndInstall,
    }
    const client = makeClient({ check: vi.fn(async () => pending) })

    const ctx = mountUpdater(client, { autoCheck: true, startupDelayMs: 5, pollIntervalMs: 25 })

    await waitFor(() => {
      expect(ctx.current.state.status).toBe('ready')
    })
    expect(client.check).toHaveBeenCalledTimes(1)
    expect(downloadAndInstall).toHaveBeenCalledTimes(1)

    // Subsequent periodic ticks do not re-check once already ready.
    await new Promise((r) => setTimeout(r, 60))
    expect(client.check).toHaveBeenCalledTimes(1)
  })

  test('skips background checks when disabled and stages immediately when re-enabled on an available update', async () => {
    setAutoUpdateEnabled(false)
    const downloadAndInstall = vi.fn(async () => {})
    const pending: PendingUpdateHandle = {
      info: { version: '0.27.0' },
      downloadAndInstall,
    }
    const client = makeClient({ check: vi.fn(async () => pending) })

    const ctx = mountUpdater(client, { autoCheck: true, startupDelayMs: 5, pollIntervalMs: 0 })
    expect(ctx.current.autoUpdateEnabled).toBe(false)

    await new Promise((r) => setTimeout(r, 40))
    expect(client.check).not.toHaveBeenCalled()

    // Manual check finds the update without auto-downloading.
    await act(async () => {
      await ctx.current.checkForUpdates()
    })
    expect(ctx.current.state.status).toBe('available')
    expect(downloadAndInstall).not.toHaveBeenCalled()

    // Turning auto-updates back on downloads the already-available update.
    act(() => {
      ctx.current.setAutoUpdateEnabled(true)
    })
    await waitFor(() => {
      expect(ctx.current.state.status).toBe('ready')
    })
    expect(downloadAndInstall).toHaveBeenCalledTimes(1)
  })

  test('falls back to available when a silent background download fails', async () => {
    const pending: PendingUpdateHandle = {
      info: { version: '0.27.0' },
      downloadAndInstall: vi.fn(async () => {
        throw new Error('network dropped')
      }),
    }
    const client = makeClient({ check: vi.fn(async () => pending) })

    const ctx = mountUpdater(client, { autoCheck: true, startupDelayMs: 5, pollIntervalMs: 0 })

    await waitFor(() => {
      expect(ctx.current.state.status).toBe('available')
    })
    expect(ctx.current.state).toEqual({ status: 'available', info: { version: '0.27.0' } })
  })

  test('delegates restart to finishInstall on platforms where the installer relaunches (Windows)', async () => {
    const finishInstall = vi.fn(async () => true)
    const pending: PendingUpdateHandle = {
      info: { version: '0.27.0' },
      downloadAndInstall: vi.fn(async () => {}),
      finishInstall,
    }
    const client = makeClient({ check: vi.fn(async () => pending) })

    const ctx = mountUpdater(client, { autoCheck: true, startupDelayMs: 5, pollIntervalMs: 0 })
    await waitFor(() => {
      expect(ctx.current.state.status).toBe('ready')
    })

    await act(async () => {
      await ctx.current.relaunch()
    })
    expect(finishInstall).toHaveBeenCalledTimes(1)
    expect(client.relaunch).not.toHaveBeenCalled()
  })

  test('closes superseded pending handles on a subsequent check', async () => {
    const closeFirst = vi.fn(async () => {})
    const first: PendingUpdateHandle = {
      info: { version: '0.27.0' },
      downloadAndInstall: vi.fn(async () => {}),
      close: closeFirst,
    }
    const second: PendingUpdateHandle = {
      info: { version: '0.28.0' },
      downloadAndInstall: vi.fn(async () => {}),
    }
    const check = vi.fn<() => Promise<PendingUpdateHandle | null>>()
      .mockResolvedValueOnce(first)
      .mockResolvedValueOnce(second)
    const client = makeClient({ check })

    const ctx = mountUpdater(client, { autoCheck: false })
    await act(async () => {
      await ctx.current.checkForUpdates()
    })
    expect(closeFirst).not.toHaveBeenCalled()

    await act(async () => {
      await ctx.current.checkForUpdates()
    })
    expect(closeFirst).toHaveBeenCalledTimes(1)
  })
})

describe('TauriUpdaterClient', () => {
  test('invokes native app_relaunch command on relaunch', async () => {
    const invokeMock = vi.fn(async () => undefined)
    const client = new TauriUpdaterClient(invokeMock as never)

    await client.relaunch()

    expect(invokeMock).toHaveBeenCalledWith('app_relaunch')
  })
})
