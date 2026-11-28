import { afterEach, expect, test } from 'vitest'
import { page } from 'vitest/browser'
import { CardEditorPage } from '../features/cards/CardEditorPage'
import { DeckListPage } from '../features/decks/DeckListPage'
import { DeckSettingsPage } from '../features/decks/DeckSettingsPage'
import { ReviewPage } from '../features/review/ReviewPage'
import { DraftInboxPage } from '../features/drafts/DraftInboxPage'
import { freshStorage } from '../test/seed'
import { renderRoute } from '../test/renderRoute'
import { setSidebarCollapsed, setSidebarHoverExpand } from './sidebarPrefs'

afterEach(async () => {
  delete document.documentElement.dataset.tauri
  delete document.documentElement.dataset.platform
  setSidebarCollapsed(false)
  setSidebarHoverExpand(true)
  await page.viewport(1280, 800)
})

test('focused review toolbar aligns with the macOS overlay titlebar', async () => {
  await page.viewport(1040, 720)
  document.documentElement.dataset.tauri = 'yes'
  document.documentElement.dataset.platform = 'mac'

  const storage = freshStorage()
  const deck = await storage.createDeck('TypeScript')
  await storage.createCard(deck.id, 'Question', 'Answer')
  await renderRoute({
    storage,
    entry: `/decks/${deck.id}/study`,
    path: '/decks/:deckId/study',
    element: <ReviewPage />,
  })
  await expect.element(page.getByRole('link', { name: 'End session' })).toBeVisible()

  const header = document.querySelector<HTMLElement>('.page-header')!
  const title = header.querySelector<HTMLElement>('h1')!
  const action = page.getByRole('link', { name: 'End session' }).element()
  const progress = page.getByRole('progressbar', { name: 'Review progress' }).element()
  const headerRect = header.getBoundingClientRect()
  const titleRect = title.getBoundingClientRect()
  const actionRect = action.getBoundingClientRect()

  expect(headerRect.height).toBe(38)
  expect(getComputedStyle(header).paddingLeft).toBe('92px')
  expect(Math.abs((titleRect.top + titleRect.bottom) / 2 - headerRect.height / 2)).toBeLessThan(1)
  expect(Math.abs((actionRect.top + actionRect.bottom) / 2 - headerRect.height / 2)).toBeLessThan(1)
  expect(progress.getBoundingClientRect().top).toBe(headerRect.bottom)
})

test('review actions do not overflow at the minimum supported window width', async () => {
  await page.viewport(760, 720)

  const storage = freshStorage()
  const deck = await storage.createDeck('TypeScript')
  await storage.createCard(deck.id, 'Question', 'Answer')
  await renderRoute({
    storage,
    entry: `/decks/${deck.id}/study`,
    path: '/decks/:deckId/study',
    element: <ReviewPage />,
  })
  await page.getByRole('button', { name: 'Show answer', exact: false }).click()
  await expect.element(page.getByRole('button', { name: 'Good', exact: false })).toBeVisible()

  const content = document.querySelector<HTMLElement>('.content')!
  expect(content.scrollWidth).toBe(content.clientWidth)
})

test('card editor does not overflow at the minimum supported window width', async () => {
  await page.viewport(760, 720)

  const storage = freshStorage()
  const deck = await storage.createDeck('A long deck name that still fits cleanly')
  await renderRoute({
    storage,
    entry: `/decks/${deck.id}/cards/new`,
    path: '/decks/:deckId/cards/new',
    element: <CardEditorPage />,
  })
  await expect.element(page.getByLabelText('Card content')).toBeVisible()

  const content = document.querySelector<HTMLElement>('.content')!
  expect(content.scrollWidth).toBe(content.clientWidth)
})

test('draft inbox does not overflow at the minimum supported window width', async () => {
  await page.viewport(760, 720)

  const storage = freshStorage()
  const deck = await storage.createDeck('A long deck name that still fits cleanly')
  await storage.proposeDrafts(deck.id, [{
    front: 'A sufficiently long question that still needs to fit the draft inbox',
    back: 'A proposed answer',
    tags: ['one', 'two'],
    rationale: 'A rationale',
    sources: [{ locator: 'a/long/source/locator/that/must/wrap/without/overflow', label: null }],
  }], { proposedBy: 'pi' })
  await renderRoute({
    storage,
    entry: '/drafts',
    path: '/drafts',
    element: <DraftInboxPage />,
  })
  await page.getByRole('button', { name: 'Reveal proposal' }).click()
  await expect.element(page.getByRole('button', { name: 'Accept draft' })).toBeVisible()

  const content = document.querySelector<HTMLElement>('.content')!
  expect(content.scrollWidth).toBe(content.clientWidth)
})

test('deck options do not overflow at the minimum supported window width', async () => {
  await page.viewport(760, 720)

  const storage = freshStorage()
  const deck = await storage.createDeck('A long deck name that still fits cleanly')
  await renderRoute({
    storage,
    entry: `/decks/${deck.id}/options`,
    path: '/decks/:deckId/options',
    element: <DeckSettingsPage />,
  })
  await expect.element(page.getByText('Deck options')).toBeVisible()

  const content = document.querySelector<HTMLElement>('.content')!
  expect(content.scrollWidth).toBe(content.clientWidth)
})

test('sidebar collapses to an icon rail and persists state', async () => {
  const storage = freshStorage()
  await storage.createDeck('Rust')
  await renderRoute({
    storage,
    entry: '/',
    path: '/',
    element: <DeckListPage />,
  })

  const collapseBtn = page.getByRole('button', { name: 'Collapse sidebar' })
  await expect.element(collapseBtn).toBeVisible()
  await collapseBtn.click()

  const app = document.querySelector<HTMLElement>('.app')!
  expect(app.classList.contains('is-sidebar-collapsed')).toBe(true)
  expect(localStorage.getItem('rem-sidebar-collapsed')).toBe('true')
  await expect.element(page.getByRole('button', { name: 'Expand sidebar' })).toBeVisible()
})

test('deleting a parent folder from the sidebar redirects away from a deleted child route', async () => {
  localStorage.removeItem('rem:collapsed-deck-paths')
  const storage = freshStorage()
  const child = await storage.createDeck('Anatomy::Unit 01::Cells')
  await storage.createDeck('Anatomy::Unit 02::Bones')
  const keep = await storage.createDeck('Rust')
  await storage.createCard(child.id, 'Cell?', 'Yes')

  await renderRoute({
    storage,
    entry: `/decks/${child.id}/options`,
    path: '/decks/:deckId/options',
    element: <DeckSettingsPage />,
    extraRoutes: [{ path: '/', element: <div>Home after delete</div> }],
  })

  await expect.element(page.getByText('Deck options')).toBeVisible()
  await expect
    .poll(() =>
      [...document.querySelectorAll<HTMLElement>('.side-decks .side-deck-row')].some(
        (row) => row.querySelector('.nav-grow')?.textContent?.trim() === 'Anatomy',
      ),
    )
    .toBe(true)
  const sidebar = document.querySelector<HTMLElement>('.side-decks')!
  const anatomyRow = [...sidebar.querySelectorAll<HTMLElement>('.side-deck-row')].find(
    (row) => row.querySelector('.nav-grow')?.textContent?.trim() === 'Anatomy',
  )!
  anatomyRow.querySelector<HTMLButtonElement>('.side-deck-delete')!.click()

  await expect.element(page.getByText('Delete "Anatomy" and subdecks?')).toBeVisible()
  await page.getByRole('button', { name: 'Delete all' }).click()

  await expect.poll(async () => (await storage.listDecks()).map((d) => d.id)).toEqual([keep.id])
  await expect.element(page.getByText('Home after delete')).toBeVisible()
})
