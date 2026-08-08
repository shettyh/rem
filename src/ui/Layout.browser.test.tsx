import { afterEach, expect, test } from 'vitest'
import { page } from 'vitest/browser'
import { DeckSettingsPage } from '../features/decks/DeckSettingsPage'
import { ReviewPage } from '../features/review/ReviewPage'
import { freshStorage } from '../test/seed'
import { renderRoute } from '../test/renderRoute'

afterEach(async () => {
  delete document.documentElement.dataset.tauri
  delete document.documentElement.dataset.platform
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
