import { test, expect } from 'vitest'
import { page } from 'vitest/browser'
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
