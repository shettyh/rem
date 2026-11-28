import { afterEach, expect, test, vi } from 'vitest'
import { page } from 'vitest/browser'
import { useLocation } from 'react-router-dom'
import { DeckSettingsPage } from './DeckSettingsPage'
import { freshStorage } from '../../test/seed'
import { renderRoute } from '../../test/renderRoute'
import { MS_PER_DAY } from '../../domain/models'
import { DEFAULT_FSRS_WEIGHTS, getFsrsOptimizer } from '../../domain/scheduler/optimizer'

afterEach(() => vi.restoreAllMocks())

test('renders the General section and persists a rename on blur', async () => {
  const storage = freshStorage()
  const deck = await storage.createDeck('Spanish')

  await renderRoute({
    storage,
    path: '/decks/:deckId/options',
    entry: `/decks/${deck.id}/options`,
    element: <DeckSettingsPage />,
  })

  await expect.element(page.getByText('Deck options')).toBeVisible()
  await expect.element(page.getByText('Color', { exact: true })).not.toBeInTheDocument()
  await expect.element(page.getByText('Default schedule')).toBeVisible()
  await expect.element(page.getByText('0 recorded reviews')).toBeVisible()
  await expect.element(page.getByRole('button', { name: 'Optimize' })).toBeDisabled()
  const name = page.getByLabelText('Deck name')
  await name.fill('Español')
  await name.element().blur()

  await expect.poll(async () => (await storage.getDeck(deck.id))?.name).toBe('Español')
})

test('returns to the deck from the header back button', async () => {
  const storage = freshStorage()
  const deck = await storage.createDeck('Spanish')

  await renderRoute({
    storage,
    path: '/decks/:deckId/options',
    entry: `/decks/${deck.id}/options`,
    element: <DeckSettingsPage />,
    extraRoutes: [{ path: '/decks/:deckId', element: <div>Deck screen</div> }],
  })

  await page.getByRole('button', { name: 'Back to deck' }).click()
  await expect.element(page.getByText('Deck screen')).toBeVisible()
})

test('persists the desired-retention stepper', async () => {
  const storage = freshStorage()
  const deck = await storage.createDeck('Spanish')
  await renderRoute({
    storage,
    path: '/decks/:deckId/options',
    entry: `/decks/${deck.id}/options`,
    element: <DeckSettingsPage />,
  })

  await page.getByLabelText('Increase Desired retention').click()
  await expect.poll(async () => (await storage.getDeck(deck.id))?.settings.desiredRetention).toBe(0.91)
})

test('persists daily-limit, new-card, and lapse edits', async () => {
  const storage = freshStorage()
  const deck = await storage.createDeck('Spanish')
  await renderRoute({
    storage,
    path: '/decks/:deckId/options',
    entry: `/decks/${deck.id}/options`,
    element: <DeckSettingsPage />,
  })

  await page.getByLabelText('Increase New cards/day').click()
  await expect.poll(async () => (await storage.getDeck(deck.id))?.settings.newPerDay).toBe(25)

  await page.getByRole('button', { name: 'RANDOM' }).click()
  await expect.poll(async () => (await storage.getDeck(deck.id))?.settings.insertionOrder).toBe('random')

  const learn = page.getByLabelText('Learning steps')
  await learn.fill('1m 10m 1h')
  await learn.element().blur()
  await expect.poll(async () => (await storage.getDeck(deck.id))?.settings.learnSteps).toBe('1m 10m 1h')

  await page.getByRole('button', { name: 'SUSPEND' }).click()
  await expect.poll(async () => (await storage.getDeck(deck.id))?.settings.leechAction).toBe('suspend')
})

test('starts the selected custom-study preset', async () => {
  const storage = freshStorage()
  const deck = await storage.createDeck('Spanish')
  function LocationProbe() {
    const location = useLocation()
    return <div>{location.pathname}{location.search}</div>
  }
  await renderRoute({
    storage,
    path: '/decks/:deckId/options',
    entry: `/decks/${deck.id}/options`,
    element: <DeckSettingsPage />,
    extraRoutes: [{ path: '/decks/:deckId/study', element: <LocationProbe /> }],
  })

  await expect.element(page.getByText('Bury related siblings')).toBeVisible()
  await expect.element(page.getByText('Show answer timer')).toBeVisible()

  const start = page.getByRole('button', { name: 'Start' })
  await expect.element(start).toBeDisabled()
  const studyAhead = page.getByRole('button', { name: /^Study ahead Review cards due later/ })
  await studyAhead.click()
  await expect.element(studyAhead).toHaveAttribute('aria-pressed', 'true')
  await expect.element(start).toBeEnabled()
  await expect.element(page.getByText('1 day')).toBeVisible()

  await page.getByLabelText('Increase Study ahead days').click()
  await start.click()
  await expect.element(page.getByText(`/decks/${deck.id}/study?custom=study-ahead&amount=2`)).toBeVisible()
})

test('optimizes and resets per-deck FSRS parameters from delayed review history', async () => {
  const storage = freshStorage()
  const deck = await storage.createDeck('Spanish')
  const card = await storage.createCard(deck.id, 'q', 'a')
  storage.addReviewLog({ cardId: card.id, deckId: deck.id, reviewedAt: 0, grade: 'good' })
  storage.addReviewLog({ cardId: card.id, deckId: deck.id, reviewedAt: MS_PER_DAY, grade: 'again' })

  await renderRoute({
    storage,
    path: '/decks/:deckId/options',
    entry: `/decks/${deck.id}/options`,
    element: <DeckSettingsPage />,
  })

  await expect.element(page.getByText('2 recorded reviews')).toBeVisible()
  await page.getByRole('button', { name: 'Optimize' }).click()
  await expect.poll(async () => (await storage.getDeck(deck.id))?.settings.fsrsWeights).toEqual([...DEFAULT_FSRS_WEIGHTS])
  await expect.element(page.getByText('Personalized schedule')).toBeVisible()

  await page.getByRole('button', { name: 'Reset' }).click()
  await expect.poll(async () => (await storage.getDeck(deck.id))?.settings.fsrsWeights).toBeNull()
  await expect.element(page.getByText('Default schedule')).toBeVisible()
})

test('guards the optimizer action while training is in flight', async () => {
  let finish!: (weights: number[]) => void
  vi.spyOn(getFsrsOptimizer(), 'optimize').mockReturnValueOnce(new Promise((resolve) => {
    finish = resolve
  }))
  const storage = freshStorage()
  const deck = await storage.createDeck('Spanish')
  const card = await storage.createCard(deck.id, 'q', 'a')
  storage.addReviewLog({ cardId: card.id, deckId: deck.id, reviewedAt: 0, grade: 'good' })
  storage.addReviewLog({ cardId: card.id, deckId: deck.id, reviewedAt: MS_PER_DAY, grade: 'good' })
  await renderRoute({
    storage,
    path: '/decks/:deckId/options',
    entry: `/decks/${deck.id}/options`,
    element: <DeckSettingsPage />,
  })

  await page.getByRole('button', { name: 'Optimize' }).click()
  await expect.element(page.getByRole('button', { name: 'Optimizing…' })).toBeDisabled()
  finish([...DEFAULT_FSRS_WEIGHTS])
  await expect.element(page.getByText('Personalized schedule')).toBeVisible()
})

test('shows a recoverable optimizer error', async () => {
  vi.spyOn(getFsrsOptimizer(), 'optimize').mockRejectedValueOnce(new Error('boom'))
  const storage = freshStorage()
  const deck = await storage.createDeck('Spanish')
  const card = await storage.createCard(deck.id, 'q', 'a')
  storage.addReviewLog({ cardId: card.id, deckId: deck.id, reviewedAt: 0, grade: 'good' })
  storage.addReviewLog({ cardId: card.id, deckId: deck.id, reviewedAt: MS_PER_DAY, grade: 'good' })
  await renderRoute({
    storage,
    path: '/decks/:deckId/options',
    entry: `/decks/${deck.id}/options`,
    element: <DeckSettingsPage />,
  })

  await page.getByRole('button', { name: 'Optimize' }).click()
  await expect.element(page.getByRole('alert')).toMatchTextContent("Couldn't optimize schedule")
  await expect.element(page.getByRole('button', { name: 'Optimize' })).toBeEnabled()
})

test('deletes the deck after confirm and navigates away', async () => {
  const storage = freshStorage()
  const deck = await storage.createDeck('Spanish')
  await renderRoute({
    storage,
    path: '/decks/:deckId/options',
    entry: `/decks/${deck.id}/options`,
    element: <DeckSettingsPage />,
    extraRoutes: [{ path: '/', element: <div>Home</div> }],
  })

  await page.getByRole('button', { name: 'Delete deck' }).click()
  await page.getByRole('button', { name: 'Confirm delete' }).click()
  await expect.poll(async () => await storage.getDeck(deck.id)).toBeUndefined()
  await expect.element(page.getByText('Home')).toBeInTheDocument()
})

test('shows subtree counts and cascades deletion to subdecks from Deck options', async () => {
  const storage = freshStorage()
  const parent = await storage.createDeck('Anatomy::Unit 03')
  const child1 = await storage.createDeck('Anatomy::Unit 03::3.1 Intro')
  const child2 = await storage.createDeck('Anatomy::Unit 03::3.2 Tissues')
  const keep = await storage.createDeck('Rust')
  await storage.createCard(parent.id, 'P', 'P')
  await storage.createCard(child1.id, 'C1', 'C1')
  await storage.createCard(child2.id, 'C2', 'C2')

  await renderRoute({
    storage,
    path: '/decks/:deckId/options',
    entry: `/decks/${parent.id}/options`,
    element: <DeckSettingsPage />,
    extraRoutes: [{ path: '/', element: <div>Home</div> }],
  })

  await expect.element(page.getByText(/2 subdecks, and 3 cards total/)).toBeVisible()
  await page.getByRole('button', { name: 'Delete deck & subdecks' }).click()
  await page.getByRole('button', { name: 'Confirm delete' }).click()

  await expect.poll(async () => (await storage.listDecks()).map((d) => d.name)).toEqual([
    'Anatomy',
    keep.name,
  ])
  await expect.element(page.getByText('Home')).toBeInTheDocument()
})
