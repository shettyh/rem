import { act, render } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { freshStorage } from '../test/seed'
import { StorageProvider } from './StorageContext'
import type { AuthProviders, SignInOutcome, SyncClient, SyncStatus } from './SyncClient'
import {
  PERIODIC_SYNC_FALLBACK_MS,
  PERIODIC_SYNC_POKE_CONNECTED_MS,
  SyncProvider,
  periodicSyncIntervalMs,
} from './SyncContext'

function makeFakeSyncClient(initial: Partial<SyncStatus> = {}) {
  let current: SyncStatus = {
    mode: 'cloud',
    activity: 'idle',
    defaultServerUrl: 'https://rem-sync.shettyh.fyi',
    serverUrl: 'https://rem-sync.shettyh.fyi',
    accountId: 'usr_123',
    email: 'alice@example.com',
    provider: 'google',
    signedIn: true,
    deviceId: 'dev_desktop_1',
    lastSeq: 10,
    pendingOutbox: 0,
    lastSyncedAt: null,
    lastError: null,
    subscriptionRequired: false,
    upgradeUrl: null,
    entitlement: null,
    pokeConnected: false,
    ...initial,
  }

  const statusListeners = new Set<(status: SyncStatus) => void>()
  const syncNowSpy = vi.fn(async () => ({ ...current }))

  const emitStatus = (patch: Partial<SyncStatus>) => {
    current = { ...current, ...patch }
    for (const listener of statusListeners) {
      listener(current)
    }
  }

  const client: SyncClient = {
    supported: true,
    getStatus: vi.fn(async () => ({ ...current })),
    fetchProviders: vi.fn(
      async (): Promise<AuthProviders> => ({
        google: true,
        apple: true,
        devToken: false,
        subscriptionRequired: false,
        upgradeUrl: null,
      }),
    ),
    signInOAuth: vi.fn(async (): Promise<SignInOutcome> => ({
      status: 'connected',
      snapshot: { ...current },
      synced: true,
      syncError: null,
    })),
    signInToken: vi.fn(async (): Promise<SignInOutcome> => ({
      status: 'connected',
      snapshot: { ...current },
      synced: true,
      syncError: null,
    })),
    confirmAccountSwitch: vi.fn(async (): Promise<SignInOutcome> => ({
      status: 'connected',
      snapshot: { ...current },
      synced: true,
      syncError: null,
    })),
    cancelAccountSwitch: vi.fn(async () => {}),
    syncNow: syncNowSpy,
    refreshAccount: vi.fn(async () => ({ ...current })),
    startTrial: vi.fn(async () => ({ ...current })),
    openPolarCheckout: vi.fn(async () => {}),
    openPolarPortal: vi.fn(async () => {}),
    signOut: vi.fn(
      async (): Promise<SyncStatus> => ({
        ...current,
        mode: 'local',
        signedIn: false,
        pokeConnected: false,
      }),
    ),
    deleteAccount: vi.fn(
      async (): Promise<SyncStatus> => ({
        ...current,
        mode: 'local',
        signedIn: false,
        pokeConnected: false,
      }),
    ),
    openExternalUrl: vi.fn(async () => {}),
    subscribeStatus: (listener) => {
      statusListeners.add(listener)
      return () => {
        statusListeners.delete(listener)
      }
    },
    subscribeRemoteApplied: () => () => {},
  }

  return { client, syncNowSpy, emitStatus }
}

describe('SyncContext periodic polling backoff', () => {
  beforeEach(() => {
    vi.useFakeTimers()
  })

  afterEach(() => {
    vi.useRealTimers()
  })

  it('computes 30 min interval when WS poke is connected and 5 min fallback otherwise', () => {
    expect(periodicSyncIntervalMs(false)).toBe(5 * 60_000)
    expect(periodicSyncIntervalMs(true)).toBe(30 * 60_000)
  })

  it('switches periodic sync cadence between 5 min and 30 min based on pokeConnected', async () => {
    const storage = freshStorage()
    const { client, syncNowSpy, emitStatus } = makeFakeSyncClient({ pokeConnected: false })

    render(
      <StorageProvider storage={storage}>
        <SyncProvider client={client}>
          <div />
        </SyncProvider>
      </StorageProvider>,
    )

    // Flush startup getStatus -> refreshAccount -> initial triggerSync.
    await act(async () => {
      await Promise.resolve()
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(syncNowSpy).toHaveBeenCalledTimes(1)

    // Disconnected: polls every 5 minutes.
    await act(async () => {
      await vi.advanceTimersByTimeAsync(PERIODIC_SYNC_FALLBACK_MS)
    })
    expect(syncNowSpy).toHaveBeenCalledTimes(2)

    // WS poke connects: backs off to 30 minutes (no sync after 5 or 25 minutes).
    await act(async () => {
      emitStatus({ pokeConnected: true })
      await Promise.resolve()
    })
    await act(async () => {
      await vi.advanceTimersByTimeAsync(PERIODIC_SYNC_FALLBACK_MS * 5)
    })
    expect(syncNowSpy).toHaveBeenCalledTimes(2)

    await act(async () => {
      await vi.advanceTimersByTimeAsync(PERIODIC_SYNC_POKE_CONNECTED_MS - PERIODIC_SYNC_FALLBACK_MS * 5)
    })
    expect(syncNowSpy).toHaveBeenCalledTimes(3)

    // WS poke disconnects: immediately falls back to 5-minute polling.
    await act(async () => {
      emitStatus({ pokeConnected: false })
      await Promise.resolve()
    })
    await act(async () => {
      await vi.advanceTimersByTimeAsync(PERIODIC_SYNC_FALLBACK_MS)
    })
    expect(syncNowSpy).toHaveBeenCalledTimes(4)
  })
})
