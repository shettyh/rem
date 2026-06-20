import type { ReactElement } from 'react'
import { MemoryRouter, Routes, Route } from 'react-router-dom'
import { render } from 'vitest-browser-react'
import { Layout } from '../ui/Layout'
import { StorageProvider } from '../data/StorageContext'
import type { Storage } from '../data/Storage'

/**
 * Mount a page element at `path` (visited via `entry`) under the real Layout and a
 * MemoryRouter, with `storage` injected. Wrapped in a screenshot target div.
 */
export function renderRoute(opts: {
  storage: Storage
  /** Route pattern, e.g. '/decks/:deckId'. */
  path: string
  /** URL actually visited, e.g. '/decks/abc'. */
  entry: string
  element: ReactElement
}) {
  return render(
    <div data-testid="screen">
      <StorageProvider storage={opts.storage}>
        <MemoryRouter initialEntries={[opts.entry]}>
          <Routes>
            <Route element={<Layout />}>
              <Route path={opts.path} element={opts.element} />
            </Route>
          </Routes>
        </MemoryRouter>
      </StorageProvider>
    </div>,
  )
}
