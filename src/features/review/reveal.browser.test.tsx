import { test, expect, vi, afterEach } from 'vitest'
import { page } from 'vitest/browser'
import { ReviewPage } from './ReviewPage'
import { freshStorage } from '../../test/seed'
import { renderRoute } from '../../test/renderRoute'
import { getScheduler } from '../../domain/scheduler'
import { DEFAULT_DECK_SETTINGS } from '../../domain/models'
import { localDay } from './day'

test('revealing shows the answer and grade buttons', async () => {
  const storage = freshStorage()
  const deck = await storage.createDeck('TypeScript')
  await storage.createCard(deck.id, 'Q?', 'A — the answer.')
  await renderRoute({
    storage,
    entry: `/decks/${deck.id}/study`,
    path: '/decks/:deckId/study',
    element: <ReviewPage />,
  })

  await expect.element(page.getByRole('button', { name: 'Show answer', exact: false })).toBeVisible()
  await expect.element(page.getByText('A — the answer.')).not.toBeInTheDocument()

  await page.getByRole('button', { name: 'Show answer', exact: false }).click()

  await expect.element(page.getByText('A — the answer.')).toBeVisible()
  await expect.element(page.getByRole('button', { name: 'Good', exact: false })).toBeVisible()
})

afterEach(() => {
  vi.restoreAllMocks()
})

test('scheduling rejection shows recoverable error and no grade buttons', async () => {
  const sched = getScheduler()
  vi.spyOn(sched, 'previewNextStates').mockRejectedValueOnce(new Error('boom'))

  const storage = freshStorage()
  const deck = await storage.createDeck('TypeScript')
  await storage.createCard(deck.id, 'Q?', 'A — the answer.')
  await renderRoute({
    storage,
    entry: `/decks/${deck.id}/study`,
    path: '/decks/:deckId/study',
    element: <ReviewPage />,
  })

  await page.getByRole('button', { name: 'Show answer', exact: false }).click()

  await expect.element(page.getByText("Couldn't schedule this card.")).toBeVisible()
  await expect.element(page.getByRole('button', { name: 'Good', exact: false })).not.toBeInTheDocument()
})

test('retry after scheduling failure resolves grade buttons', async () => {
  const sched = getScheduler()
  vi.spyOn(sched, 'previewNextStates').mockRejectedValueOnce(new Error('boom'))

  const storage = freshStorage()
  const deck = await storage.createDeck('TypeScript')
  await storage.createCard(deck.id, 'Q?', 'A — the answer.')
  await renderRoute({
    storage,
    entry: `/decks/${deck.id}/study`,
    path: '/decks/:deckId/study',
    element: <ReviewPage />,
  })

  await page.getByRole('button', { name: 'Show answer', exact: false }).click()
  await expect.element(page.getByRole('button', { name: 'Retry' })).toBeVisible()

  await page.getByRole('button', { name: 'Retry' }).click()

  await expect.element(page.getByRole('button', { name: 'Good', exact: false })).toBeVisible()
  await expect.element(page.getByText("Couldn't schedule this card.")).not.toBeInTheDocument()
})

test('grading twice before the first persist resolves applies only one grade', async () => {
  const storage = freshStorage()
  const deck = await storage.createDeck('TypeScript')
  await storage.createCard(deck.id, 'Q?', 'A — the answer.')
  const spy = vi.spyOn(storage, 'updateCard')

  await renderRoute({
    storage,
    entry: `/decks/${deck.id}/study`,
    path: '/decks/:deckId/study',
    element: <ReviewPage />,
  })

  await page.getByRole('button', { name: 'Show answer', exact: false }).click()
  const good = page.getByRole('button', { name: 'Good', exact: false })
  await expect.element(good).toBeVisible()

  // Two synchronous clicks land in the same tick, before the first grade's async
  // persist resolves and re-renders. Only the first should take effect.
  const el = good.element() as HTMLElement
  el.click()
  el.click()

  await vi.waitFor(() => expect(spy).toHaveBeenCalled())
  expect(spy).toHaveBeenCalledTimes(1)
})

test('a long answer renders after reveal', async () => {
  const storage = freshStorage()
  const deck = await storage.createDeck('Long')
  const longBack =
    Array.from({ length: 40 }, (_, i) => `Paragraph ${i} of the answer with some content.`).join(
      '\n\n',
    ) + '\n\nThe final distinctive closing phrase.'
  await storage.createCard(deck.id, 'Q?', longBack)
  await renderRoute({
    storage,
    entry: `/decks/${deck.id}/study`,
    path: '/decks/:deckId/study',
    element: <ReviewPage />,
  })

  await page.getByRole('button', { name: 'Show answer', exact: false }).click()

  await expect.element(page.getByText('The final distinctive closing phrase.')).toBeVisible()
})

test('enforces newPerDay: only the day\'s new allowance enters the session', async () => {
  const storage = freshStorage()
  const deck = await storage.createDeck('Caps')
  await storage.updateDeck(deck.id, {
    settings: { ...DEFAULT_DECK_SETTINGS, newPerDay: 1, learnSteps: '1m' },
  })
  await storage.createCard(deck.id, 'Q1', 'A1')
  await storage.createCard(deck.id, 'Q2', 'A2')
  await renderRoute({
    storage,
    entry: `/decks/${deck.id}/study`,
    path: '/decks/:deckId/study',
    element: <ReviewPage />,
  })

  // Two new cards are due, but only one enters: position reads "1 / 1", not "1 / 2".
  await expect.element(page.getByText('1 / 1', { exact: false })).toBeVisible()

  // Grading it out of New bumps newIntroduced.
  await page.getByRole('button', { name: 'Show answer', exact: false }).click()
  await page.getByRole('button', { name: 'Good', exact: false }).click()
  await vi.waitFor(async () => {
    const stat = await storage.getDailyStat(deck.id, localDay(Date.now()))
    expect(stat.newIntroduced).toBe(1)
  })
})
