import { useEffect, useState } from 'react'
import { test, expect, vi } from 'vitest'
import { page, userEvent } from 'vitest/browser'
import { useParams } from 'react-router-dom'
import { freshStorage } from '../test/seed'
import { renderRoute } from '../test/renderRoute'

function EditorRouteStub() {
  const { deckId, cardId } = useParams()
  return <div data-testid="card-editor-route">{`${deckId}/${cardId}`}</div>
}

function DeckRouteStub() {
  const { deckId } = useParams()
  return <div data-testid="deck-detail-route">{deckId}</div>
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

  const input = page.getByRole('combobox', { name: 'Search all cards' })
  await expect.element(input).toBeVisible()

  // Input inside the frameless palette header must not inherit the harsh outer focus box.
  expect(getComputedStyle(input.element()).outlineStyle).toBe('none')

  await userEvent.type(input.element(), 'Pin')

  const results = page.getByRole('listbox', { name: 'Search results' })
  await expect.element(results).toHaveAttribute('aria-busy', 'false')
  await expect.element(page.getByText('What does Pin<P> guarantee?')).toBeVisible()
  await expect.element(page.getByText('What is WAL mode?')).not.toBeInTheDocument()

  // Active row highlight must contrast with the modal surface in light mode.
  const modal = document.querySelector<HTMLElement>('.search-modal')!
  const activeOption = page.getByRole('option').first().element()
  expect(getComputedStyle(activeOption).backgroundColor).not.toBe(
    getComputedStyle(modal).backgroundColor,
  )

  await page.getByText('What does Pin<P> guarantee?').click()
  await expect.element(page.getByTestId('card-editor-route')).toHaveTextContent(`${rust.id}/${target.id}`)
})

test('empty search lists decks for quick jump and nested deck queries match formatted, raw, and multi-word paths', async () => {
  const storage = freshStorage()
  const rust = await storage.createDeck('Rust')
  const sql = await storage.createDeck('SQL::Internals')
  await storage.createCard(rust.id, 'Ownership rule?', 'Each value has one owner.')
  await storage.createCard(sql.id, 'What is a B-Tree?', 'Balanced search tree.')

  await renderRoute({
    storage,
    entry: '/',
    path: '/',
    element: <div>Today home</div>,
    extraRoutes: [
      {
        path: '/decks/:deckId',
        element: <DeckRouteStub />,
      },
    ],
  })

  window.dispatchEvent(new KeyboardEvent('keydown', { key: 'k', ctrlKey: true, bubbles: true }))
  const input = page.getByRole('combobox', { name: 'Search all cards' })
  await expect.element(input).toBeVisible()

  // Empty query immediately shows quick-jump decks.
  await expect.element(page.getByText('Jump to deck')).toBeVisible()
  await expect.element(page.getByRole('option', { name: /SQL \/ Internals/ })).toBeVisible()

  // Formatted display path ("SQL / Internals") matches the deck.
  await userEvent.type(input.element(), 'SQL / Internals')
  const results = page.getByRole('listbox', { name: 'Search results' })
  await expect.element(results).toHaveAttribute('aria-busy', 'false')
  await expect.element(page.getByRole('option', { name: /SQL \/ Internals/ })).toBeVisible()

  // Multi-word query ("SQL Internals") also matches the nested deck.
  await page.getByRole('button', { name: 'Clear search' }).click()
  await userEvent.type(input.element(), 'SQL Internals')
  await expect.element(results).toHaveAttribute('aria-busy', 'false')
  await expect.element(page.getByRole('option', { name: /SQL \/ Internals/ })).toHaveAttribute(
    'aria-selected',
    'true',
  )

  await userEvent.keyboard('{Enter}')
  await expect.element(page.getByTestId('deck-detail-route')).toHaveTextContent(sql.id)
})

test('Tab + Enter on Clear and Esc buttons activates buttons instead of navigating to a result', async () => {
  const storage = freshStorage()
  const rust = await storage.createDeck('Rust')
  await storage.createCard(rust.id, 'Ownership rule?', 'Each value has one owner.')

  await renderRoute({
    storage,
    entry: '/',
    path: '/',
    element: <div>Today home</div>,
    extraRoutes: [
      {
        path: '/decks/:deckId',
        element: <DeckRouteStub />,
      },
    ],
  })

  window.dispatchEvent(new KeyboardEvent('keydown', { key: 'k', ctrlKey: true, bubbles: true }))
  const input = page.getByRole('combobox', { name: 'Search all cards' })
  await expect.element(input).toBeVisible()
  await userEvent.type(input.element(), 'Rust')

  const results = page.getByRole('listbox', { name: 'Search results' })
  await expect.element(results).toHaveAttribute('aria-busy', 'false')

  // Tab moves directly from input to Clear button (skipping non-tabbable options).
  await userEvent.tab()
  const clearBtn = page.getByRole('button', { name: 'Clear search' }).element()
  expect(document.activeElement).toBe(clearBtn)

  // Pressing Enter on Clear button clears the search and returns focus to input without navigating.
  await userEvent.keyboard('{Enter}')
  await expect.element(input).toHaveValue('')
  expect(document.activeElement).toBe(input.element())
  await expect.element(page.getByTestId('deck-detail-route')).not.toBeInTheDocument()

  // Tab from empty input lands on Esc button; pressing Enter closes modal without navigating.
  await userEvent.tab()
  const escBtn = page.getByRole('button', { name: 'Close search' }).element()
  expect(document.activeElement).toBe(escBtn)
  await userEvent.keyboard('{Enter}')
  await expect.element(input).not.toBeInTheDocument()
  await expect.element(page.getByTestId('deck-detail-route')).not.toBeInTheDocument()
})

test('snake_case identifiers preserve underscores, 6th matching tag hoists into view, and long rationale windows around match', async () => {
  const storage = freshStorage()
  const rust = await storage.createDeck('Rust')
  await storage.createCard(
    rust.id,
    'What does `safe_name` normalize?',
    'Trims whitespace and collapses slashes.',
    ['t1', 't2', 't3', 't4', 't5', 'specialtag'],
  )
  const longPrefix = 'A'.repeat(140)
  const proposed = await storage.proposeDrafts(
    rust.id,
    [
      {
        front: 'What does Drop do?',
        back: 'Runs cleanup logic when a value goes out of scope.',
        tags: ['memory'],
        rationale: `${longPrefix} Observed a tricky destructor ordering bug during allocator refactor.`,
        sources: [],
      },
    ],
    { proposedBy: 'agent' },
  )
  const draftOutcome = proposed.outcomes[0]
  if (draftOutcome.status !== 'created') throw new Error('expected draft')
  await storage.resolveDraft(draftOutcome.value.id, draftOutcome.value.revision, {
    decision: 'accept',
    deckId: rust.id,
    card: {
      front: draftOutcome.value.front,
      back: draftOutcome.value.back,
      tags: draftOutcome.value.tags,
    },
  })

  await renderRoute({
    storage,
    entry: '/',
    path: '/',
    element: <div>Today home</div>,
  })

  window.dispatchEvent(new KeyboardEvent('keydown', { key: 'k', ctrlKey: true, bubbles: true }))
  const input = page.getByRole('combobox', { name: 'Search all cards' })
  await expect.element(input).toBeVisible()

  // snake_case identifier keeps its underscore and gets highlighted.
  await userEvent.type(input.element(), 'safe_name')
  const results = page.getByRole('listbox', { name: 'Search results' })
  await expect.element(results).toHaveAttribute('aria-busy', 'false')
  await expect.element(page.getByText('What does safe_name normalize?')).toBeVisible()
  expect(document.querySelector('.search-mark')?.textContent).toBe('safe_name')

  // 6th tag matching query is hoisted into visible tags.
  await page.getByRole('button', { name: 'Clear search' }).click()
  await userEvent.type(input.element(), 'specialtag')
  await expect.element(results).toHaveAttribute('aria-busy', 'false')
  await expect.element(page.getByText('#specialtag')).toBeVisible()

  // Long rationale (>120 chars) still detects rationale-only match and windows snippet around the hit.
  await page.getByRole('button', { name: 'Clear search' }).click()
  await userEvent.type(input.element(), 'allocator')
  await expect.element(results).toHaveAttribute('aria-busy', 'false')
  await expect.element(page.getByText('What does Drop do?')).toBeVisible()
  await expect.element(page.getByText('Rationale')).toBeVisible()
  await expect.element(page.getByText(/allocator refactor/)).toBeVisible()
})

test('search error surfaces Retry button and recovers cleanly', async () => {
  const storage = freshStorage()
  const rust = await storage.createDeck('Rust')
  await storage.createCard(rust.id, 'Ownership rule?', 'Each value has one owner.')

  const originalSearch = storage.searchCards.bind(storage)
  let failOnce = true
  vi.spyOn(storage, 'searchCards').mockImplementation((q, opts) => {
    if (failOnce) {
      failOnce = false
      return Promise.reject(new Error('sqlite busy'))
    }
    return originalSearch(q, opts)
  })

  await renderRoute({
    storage,
    entry: '/',
    path: '/',
    element: <div>Today home</div>,
  })

  window.dispatchEvent(new KeyboardEvent('keydown', { key: 'k', ctrlKey: true, bubbles: true }))
  const input = page.getByRole('combobox', { name: 'Search all cards' })
  await expect.element(input).toBeVisible()
  await userEvent.type(input.element(), 'Ownership')

  await expect.element(page.getByRole('alert')).toHaveTextContent('Search failed. Please try again.')
  await page.getByRole('button', { name: 'Retry' }).click()

  const results = page.getByRole('listbox', { name: 'Search results' })
  await expect.element(results).toHaveAttribute('aria-busy', 'false')
  await expect.element(page.getByText('Ownership rule?')).toBeVisible()
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
  const input = page.getByRole('combobox', { name: 'Search all cards' })
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
  const input = page.getByRole('combobox', { name: 'Search all cards' })
  await expect.element(input).toBeVisible()

  // Let a prefix settle first, park the pointer over the first option, then continue typing
  // to verify neither stale-state transitions nor a stationary cursor hijack ArrowDown.
  await userEvent.type(input.element(), 'memo')
  const results = page.getByRole('listbox', { name: 'Search results' })
  await expect.element(results).toHaveAttribute('aria-busy', 'false')
  await userEvent.hover(page.getByRole('option').first().element())

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
