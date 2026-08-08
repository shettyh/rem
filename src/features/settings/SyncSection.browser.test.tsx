import { describe, it, expect, beforeEach } from 'vitest'
import { render } from 'vitest-browser-react'
import { RemDB } from '../../data/dexie/db'
import { DexieStorage } from '../../data/dexie/DexieStorage'
import { StorageProvider } from '../../data/StorageContext'
import { FakeGitBridge } from '../../data/sync/FakeGitBridge'
import { GitSyncService } from '../../data/sync/GitSyncService'
import { SyncSection } from './SyncSection'

beforeEach(() => localStorage.clear())

describe('SyncSection', () => {
  it('syncs on click and reports success', async () => {
    const storage = new DexieStorage(new RemDB('sync-ui-test'))
    await storage.createDeck('S')
    const bridge = new FakeGitBridge(null)
    const makeService = (s: any, cfg: any) => new GitSyncService(s, bridge, cfg)

    const screen = await render(
      <StorageProvider storage={storage}>
        <SyncSection makeService={makeService} />
      </StorageProvider>,
    )

    await expect.element(screen.getByText(/optional/i)).toBeInTheDocument()
    await screen.getByRole('button', { name: 'Set up Git sync' }).click()
    await screen.getByLabelText('Git remote URL').fill('git@example.com:me/rem.git')

    expect(localStorage.getItem('rem.sync.remoteUrl')).toBeNull()
    await screen.getByRole('button', { name: 'Connect and sync' }).click()

    await expect.element(screen.getByText(/connected/i)).toBeInTheDocument()
    expect(localStorage.getItem('rem.sync.remoteUrl')).toBe('git@example.com:me/rem.git')
    expect(bridge.remote).not.toBeNull()
  })

  it('refuses to sync without a remote URL', async () => {
    const storage = new DexieStorage(new RemDB('sync-ui-test-2'))
    const makeService = (s: any, cfg: any) =>
      new GitSyncService(s, new FakeGitBridge(null), cfg)
    const screen = await render(
      <StorageProvider storage={storage}>
        <SyncSection makeService={makeService} />
      </StorageProvider>,
    )
    await screen.getByRole('button', { name: 'Set up Git sync' }).click()
    await screen.getByRole('button', { name: 'Connect and sync' }).click()
    await expect.element(screen.getByText(/enter a git remote url/i)).toBeInTheDocument()
  })

  it('shows configured sync as an optional layer over local storage', async () => {
    localStorage.setItem('rem.sync.remoteUrl', 'git@example.com:me/rem.git')
    const storage = new DexieStorage(new RemDB('sync-ui-test-3'))
    const screen = await render(
      <StorageProvider storage={storage}>
        <SyncSection />
      </StorageProvider>,
    )

    await expect.element(screen.getByText('Connected')).toBeInTheDocument()
    await expect.element(screen.getByText('git@example.com:me/rem.git')).toBeInTheDocument()
    await expect.element(screen.getByLabelText('Automatic sync')).toBeChecked()

    await screen.getByLabelText('Automatic sync').click()
    expect(localStorage.getItem('rem.sync.auto')).toBe('false')
  })

  it('updates the configured remote only after the new remote syncs successfully', async () => {
    localStorage.setItem('rem.sync.remoteUrl', 'git@example.com:me/old.git')
    const storage = new DexieStorage(new RemDB('sync-ui-test-4'))
    const bridge = new FakeGitBridge(null)
    const makeService = (s: any, cfg: any) => new GitSyncService(s, bridge, cfg)
    const screen = await render(
      <StorageProvider storage={storage}>
        <SyncSection makeService={makeService} />
      </StorageProvider>,
    )

    await screen.getByRole('button', { name: 'Change remote' }).click()
    await screen.getByLabelText('Git remote URL').fill('git@example.com:me/new.git')
    expect(localStorage.getItem('rem.sync.remoteUrl')).toBe('git@example.com:me/old.git')

    await screen.getByRole('button', { name: 'Update and sync' }).click()
    await expect.element(screen.getByText('git@example.com:me/new.git')).toBeInTheDocument()
    expect(localStorage.getItem('rem.sync.remoteUrl')).toBe('git@example.com:me/new.git')
    expect(bridge.remoteUrl).toBe('git@example.com:me/new.git')
  })
})
