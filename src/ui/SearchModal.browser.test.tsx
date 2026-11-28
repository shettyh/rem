import { useEffect, useState } from 'react'
import { test, expect } from 'vitest'
import { page, userEvent } from 'vitest/browser'
import { useParams } from 'react-router-dom'
import { freshStorage } from '../test/seed'
import { renderRoute } from '../test/renderRoute'

function EditorRouteStub() {
  const { deckId, cardId } = useParams()
  return <div data-testid="card-editor-route">{`${deckId}/${cardId}`}</div>
}

function EscapeListenerHost() {
  const [escaped, setEscaped] = useState(false)
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.defaultPrevented) return
      if (e.key === 'Escape') setEscaped(true)
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [])
  return <div data-testid="escape-host">{escaped ? 'ESCAPED' : 'STILL_HERE'}</div>
}

test('global search opens via sidebar button, finds cards across decks, and navigates to edit', async () => {
  const storage = freshStorage()
  const rust = await storage.createDeck('Rust')
  const sql = await storage.createDeck('SQL')
  const target = await storage.createCard(
    rust.id,
    'What does Pin<P> guarantee?',
    'The pointee cannot be moved in memory.',
    ['async'],
  )
  await storage.createCard(sql.id, 'What is WAL mode?', 'Write-Ahead Logging.')

  await renderRoute({
    storage,
    entry: '/',
    path: '/',
    element: <div>Today home</div>,
    extraRoutes: [
      {
        path: '/decks/:deckId/cards/:cardId/edit',
        element: <EditorRouteStub />,
      },
    ],
  })

  await expect.element(page.getByText('Today home')).toBeVisible()
  await page.getByRole('button', { name: 'Search', exact: false }).click()

  const input = page.getByRole('searchbox', { name: 'Search all cards' })
  await expect.element(input).toBeVisible()
  await userEvent.type(input.element(), 'Pin')

  const results = page.getByRole('listbox', { name: 'Search results' })
  await expect.element(results).toHaveAttribute('aria-busy', 'false')
  await expect.element(page.getByText('What does Pin<P> guarantee?')).toBeVisible()
  await expect.element(page.getByText('What is WAL mode?')).not.toBeInTheDocument()

  await page.getByText('What does Pin<P> guarantee?').click()
  await expect.element(page.getByTestId('card-editor-route')).toHaveTextContent(`${rust.id}/${target.id}`)
})

test('Cmd+K opens search, arrow keys + Enter navigate, and Escape does not leak to window listeners', async () => {
  const storage = freshStorage()
  const rust = await storage.createDeck('Rust')
  const first = await storage.createCard(rust.id, 'Alpha memory question', 'A1')
  const second = await storage.createCard(rust.id, 'Beta memory question', 'A2')

  await renderRoute({
    storage,
    entry: '/',
    path: '/',
    element: <EscapeListenerHost />,
    extraRoutes: [
      {
        path: '/decks/:deckId/cards/:cardId/edit',
        element: <EditorRouteStub />,
      },
    ],
  })

  await expect.element(page.getByTestId('escape-host')).toHaveTextContent('STILL_HERE')

  // Open via Ctrl+K, then Escape must close modal without triggering underlying Escape listener.
  window.dispatchEvent(new KeyboardEvent('keydown', { key: 'k', ctrlKey: true, bubbles: true }))
  const input = page.getByRole('searchbox', { name: 'Search all cards' })
  await expect.element(input).toBeVisible()

  await userEvent.keyboard('{Escape}')
  await expect.element(input).not.toBeInTheDocument()
  await expect.element(page.getByTestId('escape-host')).toHaveTextContent('STILL_HERE')

  // Reopen via Ctrl+K, search, move selection with ArrowDown, and open with Enter.
  window.dispatchEvent(new KeyboardEvent('keydown', { key: 'k', ctrlKey: true, bubbles: true }))
  await expect.element(input).toBeVisible()
  await userEvent.type(input.element(), 'memory')
  const results = page.getByRole('listbox', { name: 'Search results' })
  await expect.element(results).toHaveAttribute('aria-busy', 'false')
  await expect.element(page.getByText('Alpha memory question')).toBeVisible()
  await expect.element(page.getByText('Beta memory question')).toBeVisible()

  await userEvent.keyboard('{ArrowDown}')
  await expect.element(page.getByRole('option').nth(1)).toHaveAttribute('aria-selected', 'true')
  await userEvent.keyboard('{Enter}')
  const editorRoute = page.getByTestId('card-editor-route')
  await expect.element(editorRoute).toBeVisible()
  expect([`${rust.id}/${first.id}`, `${rust.id}/${second.id}`]).toContain(
    editorRoute.element().textContent,
  )
})

test('incremental typing waits for updated query to settle before keyboard navigation', async () => {
  const storage = freshStorage()
  const rust = await storage.createDeck('Rust')
  const first = await storage.createCard(rust.id, 'Alpha memory question', 'A1')
  const second = await storage.createCard(rust.id, 'Beta memory question', 'A2')

  await renderRoute({
    storage,
    entry: '/',
    path: '/',
    element: <div>Today home</div>,
    extraRoutes: [
      {
        path: '/decks/:deckId/cards/:cardId/edit',
        element: <EditorRouteStub />,
      },
    ],
  })

  window.dispatchEvent(new KeyboardEvent('keydown', { key: 'k', ctrlKey: true, bubbles: true }))
  const input = page.getByRole('searchbox', { name: 'Search all cards' })
  await expect.element(input).toBeVisible()

  // Let a prefix settle first, then continue typing to exercise the stale-results transition.
  await userEvent.type(input.element(), 'memo')
  const results = page.getByRole('listbox', { name: 'Search results' })
  await expect.element(results).toHaveAttribute('aria-busy', 'false')

  await userEvent.type(input.element(), 'ry')
  await expect.element(input).toHaveValue('memory')
  await expect.element(results).toHaveAttribute('aria-busy', 'false')

  await userEvent.keyboard('{ArrowDown}')
  await expect.element(page.getByRole('option').nth(1)).toHaveAttribute('aria-selected', 'true')
  await userEvent.keyboard('{Enter}')
  const editorRoute = page.getByTestId('card-editor-route')
  await expect.element(editorRoute).toBeVisible()
  expect([`${rust.id}/${first.id}`, `${rust.id}/${second.id}`]).toContain(
    editorRoute.element().textContent,
  )
})
