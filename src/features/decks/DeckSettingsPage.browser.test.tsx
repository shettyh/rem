import { test, expect } from 'vitest'
import { page } from 'vitest/browser'
import { useLocation } from 'react-router-dom'
import { DeckSettingsPage } from './DeckSettingsPage'
import { freshStorage } from '../../test/seed'
import { renderRoute } from '../../test/renderRoute'

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
  const name = page.getByLabelText('Deck name')
  await name.fill('Español')
  await name.element().blur()

  await expect.poll(async () => (await storage.getDeck(deck.id))?.name).toBe('Español')
})

test('persists a color swatch and the desired-retention stepper', async () => {
  const storage = freshStorage()
  const deck = await storage.createDeck('Spanish')
  await renderRoute({
    storage,
    path: '/decks/:deckId/options',
    entry: `/decks/${deck.id}/options`,
    element: <DeckSettingsPage />,
  })

  await page.getByLabelText('Color #2fa86b').click()
  await expect.poll(async () => (await storage.getDeck(deck.id))?.color).toBe('#2fa86b')

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

  await page.getByRole('button', { name: 'TAG' }).click()
  await expect.poll(async () => (await storage.getDeck(deck.id))?.settings.leechAction).toBe('tag')
})

test('toggles timer and starts the selected custom-study preset', async () => {
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

  await page.getByRole('switch', { name: 'Show answer timer' }).click()
  await expect.poll(async () => (await storage.getDeck(deck.id))?.settings.showTimer).toBe(true)

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
