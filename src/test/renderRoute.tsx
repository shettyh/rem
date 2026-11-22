import type { ReactElement } from 'react'
import { createMemoryRouter, RouterProvider } from 'react-router-dom'
import { render } from 'vitest-browser-react'
import { Layout } from '../ui/Layout'
import { StorageProvider } from '../data/StorageContext'
import { SyncProvider } from '../data/SyncContext'
import { UpdaterProvider } from '../data/UpdaterContext'
import type { Storage } from '../data/Storage'
import type { SyncClient } from '../data/SyncClient'
import type { UpdaterClient } from '../data/UpdaterClient'

/**
 * Mount a page element at `path` (visited via `entry`) under the real Layout and a
 * MemoryRouter, with `storage` injected. Wrapped in a screenshot target div.
 */
export function renderRoute(opts: {
  storage: Storage
  /** Optional sync client override for testing cloud sync states. */
  syncClient?: SyncClient
  /** Enable SyncProvider background focus/auto-sync handlers (default false for deterministic tests). */
  autoSync?: boolean
  /** Optional updater client override for testing update and CLI states. */
  updaterClient?: UpdaterClient
  /** Route pattern, e.g. '/decks/:deckId'. */
  path: string
  /** URL actually visited, e.g. '/decks/abc'. */
  entry: string
  element: ReactElement
  /** Optional additional routes registered alongside the primary one. */
  extraRoutes?: { path: string; element: ReactElement }[]
}) {
  const router = createMemoryRouter(
    [
      {
        element: <Layout />,
        children: [
          { path: opts.path, element: opts.element },
          ...(opts.extraRoutes ?? []),
        ],
      },
    ],
    { initialEntries: [opts.entry] },
  )

  return render(
    <div data-testid="screen">
      <StorageProvider storage={opts.storage}>
        <SyncProvider client={opts.syncClient} autoSync={opts.autoSync ?? false}>
          <UpdaterProvider client={opts.updaterClient} autoCheck={false}>
            <RouterProvider router={router} />
          </UpdaterProvider>
        </SyncProvider>
      </StorageProvider>
    </div>,
  )
}
