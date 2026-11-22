import { test, expect, vi } from 'vitest'
import { page } from 'vitest/browser'
import { SettingsPage } from './SettingsPage'
import { freshStorage } from '../../test/seed'
import { renderRoute } from '../../test/renderRoute'
import { serializeBackup, type DeckBackup } from '../../data/backup'
import { DEFAULT_DECK_SETTINGS } from '../../domain/models'
import {
  DEFAULT_CLOUD_SERVER_URL,
  type AuthProviders,
  type OAuthProvider,
  type SignInOutcome,
  type SyncClient,
  type SyncStatus,
} from '../../data/SyncClient'
import type { CliStatus, PendingUpdateHandle, UpdaterClient } from '../../data/UpdaterClient'
import packageInfo from '../../../package.json'

function renderSettings(
  storage: ReturnType<typeof freshStorage>,
  syncClient?: SyncClient,
  updaterClient?: UpdaterClient,
) {
  return renderRoute({
    storage,
    syncClient,
    updaterClient,
    entry: '/settings',
    path: '/settings',
    element: <SettingsPage />,
  })
}

function makeMockSyncClient(
  initial: Partial<SyncStatus> = {},
  overrides: Partial<SyncClient> = {},
): SyncClient & { current: SyncStatus } {
  let current: SyncStatus = {
    mode: 'local',
    activity: 'idle',
    defaultServerUrl: DEFAULT_CLOUD_SERVER_URL,
    serverUrl: null,
    accountId: null,
    email: null,
    provider: null,
    signedIn: false,
    deviceId: 'dev-test-1',
    lastSeq: 0,
    pendingOutbox: 0,
    lastSyncedAt: null,
    lastError: null,
    subscriptionRequired: false,
    upgradeUrl: null,
    ...initial,
  }
  const listeners = new Set<(s: SyncStatus) => void>()
  const emit = (next: SyncStatus) => {
    current = next
    for (const l of listeners) l(next)
  }

  return {
    get current() {
      return current
    },
    set current(val) {
      current = val
    },
    supported: true,
    getStatus: vi.fn(async () => current),
    fetchProviders: vi.fn(
      async (): Promise<AuthProviders> => ({
        google: true,
        apple: true,
        devToken: true,
        subscriptionRequired: true,
        upgradeUrl: 'https://rem-sync.shettyh.fyi/cloud',
      }),
    ),
    signInOAuth: vi.fn(async (serverUrl: string, provider: OAuthProvider): Promise<SignInOutcome> => {
      const next: SyncStatus = {
        ...current,
        mode: 'cloud',
        serverUrl,
        accountId: 'usr_alice',
        email: 'alice@example.com',
        provider,
        signedIn: true,
        lastSyncedAt: Date.now(),
        lastError: null,
      }
      emit(next)
      return { status: 'connected', snapshot: next, synced: true, syncError: null }
    }),
    signInToken: vi.fn(async (serverUrl: string): Promise<SignInOutcome> => {
      const next: SyncStatus = {
        ...current,
        mode: 'cloud',
        serverUrl,
        accountId: 'selfhost-1',
        email: null,
        provider: 'dev_token',
        signedIn: true,
        lastSyncedAt: Date.now(),
        lastError: null,
      }
      emit(next)
      return { status: 'connected', snapshot: next, synced: true, syncError: null }
    }),
    confirmAccountSwitch: vi.fn(async (): Promise<SignInOutcome> => {
      const next: SyncStatus = {
        ...current,
        mode: 'cloud',
        accountId: 'usr_bob',
        email: 'bob@example.com',
        provider: 'google',
        signedIn: true,
        lastSyncedAt: Date.now(),
        lastError: null,
      }
      emit(next)
      return { status: 'connected', snapshot: next, synced: true, syncError: null }
    }),
    cancelAccountSwitch: vi.fn(async () => {}),
    syncNow: vi.fn(async () => {
      const next: SyncStatus = { ...current, lastSyncedAt: Date.now(), pendingOutbox: 0, lastError: null }
      emit(next)
      return next
    }),
    signOut: vi.fn(async () => {
      const next: SyncStatus = { ...current, mode: 'local', signedIn: false, lastError: null }
      emit(next)
      return next
    }),
    deleteAccount: vi.fn(async () => {
      const next: SyncStatus = {
        ...current,
        mode: 'local',
        serverUrl: null,
        accountId: null,
        email: null,
        provider: null,
        signedIn: false,
        lastSeq: 0,
        pendingOutbox: 0,
        lastSyncedAt: null,
        lastError: null,
      }
      emit(next)
      return next
    }),
    openExternalUrl: vi.fn(async () => {}),
    subscribeStatus: (listener) => {
      listeners.add(listener)
      return () => listeners.delete(listener)
    },
    subscribeRemoteApplied: () => () => {},
    ...overrides,
  }
}

test('presents local storage first and keeps per-deck export behind choose decks', async () => {
  const storage = freshStorage()
  await storage.createDeck('TypeScript')
  await storage.createDeck('Spanish')
  renderSettings(storage)

  await expect.element(page.getByRole('heading', { name: 'Data & storage' })).toBeVisible()
  await expect.element(page.getByText('Active')).toBeVisible()
  await expect.element(page.getByRole('button', { name: 'Export all' })).toBeEnabled()
  await expect.element(page.getByText(`rem version ${packageInfo.version}`)).toBeVisible()
  await expect.element(page.getByLabelText('Select all decks')).not.toBeInTheDocument()

  await page.getByRole('button', { name: 'Choose decks' }).click()
  const exportBtn = page.getByRole('button', { name: 'Export selected' })
  await expect.element(exportBtn).toBeDisabled()
  await page.getByLabelText('Select all decks').click()
  await expect.element(exportBtn).toBeEnabled()
})

test('importing a same-named deck warns, then replaces on confirm', async () => {
  const storage = freshStorage()
  await storage.createDeck('Spanish')
  renderSettings(storage)

  const incoming: DeckBackup[] = [
    {
      name: 'Spanish',
      createdAt: 1,
      schedulerKind: 'fsrs',
      settings: DEFAULT_DECK_SETTINGS,
      cards: [
        { front: 'hola', back: 'hello', createdAt: 1, updatedAt: 1, tags: [], suspended: false, lastAgainAt: null, scheduling: { kind: 'fsrs', stability: 0, difficulty: 0, reps: 0, lapses: 0, state: 0, step: 0, lastReview: null, due: 1 }, reviews: [] },
      ],
    },
  ]
  const file = new File([serializeBackup(incoming, 1000)], 'backup.json', { type: 'application/json' })

  await page.getByLabelText('Import backup file').upload(file)

  await expect.element(page.getByText('will be replaced', { exact: false })).toBeVisible()
  await page.getByRole('button', { name: 'Replace' }).click()
  await expect.element(page.getByText('Imported', { exact: false })).toBeVisible()
})

test('updates and persists card text size preference', async () => {
  const storage = freshStorage()
  renderSettings(storage)

  await page.getByRole('button', { name: 'Large' }).click()
  expect(document.documentElement.dataset.cardSize).toBe('large')
  expect(localStorage.getItem('rem-card-text-size')).toBe('large')

  await page.getByRole('button', { name: 'Standard' }).click()
  expect(document.documentElement.dataset.cardSize).toBeUndefined()
  expect(localStorage.getItem('rem-card-text-size')).toBe('standard')
})

test('defaults sidebar hover auto-expand to on and persists toggling', async () => {
  localStorage.removeItem('rem-sidebar-hover-expand')
  const storage = freshStorage()
  renderSettings(storage)

  const toggle = page.getByRole('checkbox', { name: /Auto-expand collapsed sidebar on hover/ })
  await expect.element(toggle).toBeChecked()

  await toggle.click()
  await expect.element(toggle).not.toBeChecked()
  expect(localStorage.getItem('rem-sidebar-hover-expand')).toBe('false')
})

test('defaults cloud sync to hosted rem Cloud without asking for a URL, with self-hosted option behind toggle', async () => {
  const storage = freshStorage()
  const syncClient = makeMockSyncClient()
  renderSettings(storage, syncClient)

  // Local-only mode by default: no sidebar sync button.
  await expect.element(page.getByRole('button', { name: 'Cloud sync' })).toBeVisible()
  await expect.element(page.getByRole('button', { name: /Cloud synced/ })).not.toBeInTheDocument()

  // Switch to Cloud sync -> goes straight to hosted OAuth without showing a server URL input.
  await page.getByRole('button', { name: 'Cloud sync' }).click()
  await expect.element(page.getByRole('button', { name: 'Sign in with Apple' })).toBeVisible()
  await expect.element(page.getByRole('button', { name: 'Sign in with Google' })).toBeVisible()
  await expect.element(page.getByLabelText('Custom sync server URL')).not.toBeInTheDocument()
  await expect.element(page.getByLabelText('Personal access token')).not.toBeInTheDocument()
  expect(syncClient.fetchProviders).toHaveBeenCalledWith(DEFAULT_CLOUD_SERVER_URL)

  // Custom server disclosure reveals URL + personal token setup and subtle docs link.
  await page.getByRole('button', { name: 'Use a custom server' }).click()
  const urlInput = page.getByLabelText('Custom sync server URL')
  await expect.element(urlInput).toBeVisible()
  await expect.element(page.getByRole('link', { name: 'Setup guide ↗' })).toBeVisible()
  await urlInput.fill('https://sync.example.com')
  await page.getByRole('button', { name: 'Continue' }).click()
  await expect.element(page.getByLabelText('Personal access token')).toBeVisible()
  await page.getByRole('button', { name: 'Use rem Cloud instead' }).click()

  // Sign in with Google on default hosted rem Cloud -> shows connected state and sidebar sync indicator.
  await page.getByRole('button', { name: 'Sign in with Google' }).click()
  expect(syncClient.signInOAuth).toHaveBeenCalledWith(DEFAULT_CLOUD_SERVER_URL, 'google')
  await expect.element(page.getByText('alice@example.com', { exact: false })).toBeVisible()
  await expect.element(page.getByText('rem Cloud · Last synced', { exact: false })).toBeVisible()
  await expect.element(page.getByRole('button', { name: 'Sync now' })).toBeVisible()
  await expect.element(page.getByRole('button', { name: 'Cloud synced — click to sync now' })).toBeVisible()

  // Manual sync button calls syncNow.
  await page.getByRole('button', { name: 'Sync now' }).click()
  expect(syncClient.syncNow).toHaveBeenCalledTimes(1)

  // Delete cloud account requires explicit confirmation.
  await page.getByRole('button', { name: 'Delete cloud account…' }).click()
  await expect.element(page.getByRole('heading', { name: 'Delete cloud account?' })).toBeVisible()
  await page.getByRole('button', { name: 'Delete cloud account' }).click()
  expect(syncClient.deleteAccount).toHaveBeenCalledTimes(1)
  await expect.element(page.getByText('Cloud account and remote data deleted', { exact: false })).toBeVisible()
})

test('shows subscription notice calmly when server returns subscriptionRequired', async () => {
  const storage = freshStorage()
  const syncClient = makeMockSyncClient({
    mode: 'cloud',
    serverUrl: DEFAULT_CLOUD_SERVER_URL,
    accountId: 'usr_unpaid',
    email: 'unpaid@example.com',
    provider: 'google',
    signedIn: true,
    subscriptionRequired: true,
    upgradeUrl: 'https://rem-sync.shettyh.fyi/cloud',
    lastError: 'Cloud sync requires an active rem Cloud subscription.',
  })
  renderSettings(storage, syncClient)

  await expect.element(page.getByText('Plan required')).toBeVisible()
  await expect.element(page.getByText('rem Cloud subscription required')).toBeVisible()
  const upgradeBtn = page.getByRole('button', { name: 'Get rem Cloud' })
  await expect.element(upgradeBtn).toBeVisible()
  await upgradeBtn.click()
  expect(syncClient.openExternalUrl).toHaveBeenCalledWith('https://rem-sync.shettyh.fyi/cloud')
  await expect.element(page.getByRole('button', { name: 'Retry sync' })).toBeVisible()
})

test('shows calm early-access state when non-allowlisted account signs in without upgradeUrl', async () => {
  const storage = freshStorage()
  const syncClient = makeMockSyncClient({
    mode: 'cloud',
    serverUrl: DEFAULT_CLOUD_SERVER_URL,
    accountId: 'usr_waitlist',
    email: 'alex@example.com',
    provider: 'google',
    signedIn: true,
    subscriptionRequired: true,
    upgradeUrl: null,
    lastError: 'Cloud sync requires an active rem Cloud subscription.',
  })
  renderSettings(storage, syncClient)

  await expect.element(page.getByText('Early access')).toBeVisible()
  await expect.element(page.getByText('Cloud sync is in early access')).toBeVisible()
  await expect.element(page.getByText("alex@example.com isn't on the early-access list yet", { exact: false })).toBeVisible()
  await expect.element(page.getByRole('button', { name: 'Try another account' })).toBeVisible()
  await expect.element(page.getByRole('button', { name: 'Stay local' })).toBeVisible()
})

test('prompts before switching a collection bound to another sync account', async () => {
  const storage = freshStorage()
  const syncClient = makeMockSyncClient(
    {
      mode: 'local',
      serverUrl: 'https://sync.example.com',
      accountId: 'usr_alice',
      email: 'alice@example.com',
      signedIn: false,
    },
    {
      signInOAuth: vi.fn(async (): Promise<SignInOutcome> => ({
        status: 'accountMismatch',
        boundServerUrl: 'https://sync.example.com',
        boundAccountId: 'usr_alice',
        incomingServerUrl: 'https://sync.example.com',
        incomingAccountId: 'usr_bob',
        incomingEmail: 'bob@example.com',
        incomingProvider: 'google',
      })),
    },
  )
  renderSettings(storage, syncClient)

  await expect.element(page.getByText('Cloud sync is paused', { exact: false })).toBeVisible()
  await page.getByRole('button', { name: 'Cloud sync' }).click()
  await page.getByRole('button', { name: 'Continue' }).click()
  await page.getByRole('button', { name: 'Sign in with Google' }).click()

  await expect.element(page.getByRole('heading', { name: 'Switch cloud sync account?' })).toBeVisible()
  await page.getByRole('button', { name: 'Switch account' }).click()
  expect(syncClient.confirmAccountSwitch).toHaveBeenCalledTimes(1)
  await expect.element(page.getByText('bob@example.com', { exact: false })).toBeVisible()
})

function makeMockUpdaterClient(overrides: Partial<UpdaterClient> = {}): UpdaterClient {
  const defaultCli: CliStatus = {
    supported: true,
    bundledPath: '/Applications/rem.app/Contents/MacOS/rem',
    installedPath: null,
    managedByHomebrew: false,
    linkedToBundled: false,
    onPath: false,
  }
  return {
    supported: true,
    check: vi.fn(async () => null),
    relaunch: vi.fn(async () => {}),
    getCliStatus: vi.fn(async () => defaultCli),
    installCli: vi.fn(async () => ({
      ...defaultCli,
      installedPath: '/Users/alex/.local/bin/rem',
      linkedToBundled: true,
      onPath: true,
    })),
    ...overrides,
  }
}

test('checks for updates, downloads with progress, and prompts to restart', async () => {
  const storage = freshStorage()
  const downloadAndInstall = vi.fn(async (onProgress) => {
    onProgress?.({ downloaded: 50, total: 100 })
    onProgress?.({ downloaded: 100, total: 100 })
  })
  const pending: PendingUpdateHandle = {
    info: { version: '0.19.0', body: '- Bundled rem CLI\n- In-app updates' },
    downloadAndInstall,
  }
  const updaterClient = makeMockUpdaterClient({
    check: vi.fn(async () => pending),
  })
  renderSettings(storage, undefined, updaterClient)

  await expect.element(page.getByRole('heading', { name: 'App & updates' })).toBeVisible()
  await page.getByRole('button', { name: 'Check for updates' }).click()

  await expect.element(page.getByText('Version v0.19.0 is available.')).toBeVisible()
  await expect.element(page.getByText('In-app updates', { exact: false })).toBeVisible()
  await page.getByRole('button', { name: 'Install v0.19.0' }).click()

  expect(downloadAndInstall).toHaveBeenCalledTimes(1)
  await expect.element(page.getByText('Restart to finish updating', { exact: false })).toBeVisible()
  await page.getByRole('button', { name: 'Restart to update' }).click()
  expect(updaterClient.relaunch).toHaveBeenCalledTimes(1)
})

test('reports up-to-date and links the bundled rem CLI in one click', async () => {
  const storage = freshStorage()
  const updaterClient = makeMockUpdaterClient()
  renderSettings(storage, undefined, updaterClient)

  await page.getByRole('button', { name: 'Check for updates' }).click()
  await expect.element(page.getByText(`You're up to date on v${packageInfo.version}.`)).toBeVisible()

  await page.getByRole('button', { name: 'Install rem CLI' }).click()
  expect(updaterClient.installCli).toHaveBeenCalledTimes(1)
  await expect
    .element(page.getByText('Linked at /Users/alex/.local/bin/rem. Updates automatically with rem.'))
    .toBeVisible()
})
