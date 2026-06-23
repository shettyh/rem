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

    await screen.getByLabelText('Git remote URL').fill('git@example.com:me/rem.git')
    await screen.getByRole('button', { name: 'Sync now' }).click()

    await expect.element(screen.getByText(/synced/i)).toBeInTheDocument()
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
    await screen.getByRole('button', { name: 'Sync now' }).click()
    await expect.element(screen.getByText(/enter a git remote url/i)).toBeInTheDocument()
  })
})
