import { test, expect } from 'vitest'
import { page } from 'vitest/browser'
import { DeckDetailPage } from './DeckDetailPage'
import { DeckSettingsPage } from '../decks/DeckSettingsPage'
import { freshStorage } from '../../test/seed'
import { renderRoute } from '../../test/renderRoute'

test('Options button opens the deck options screen', async () => {
  const storage = freshStorage()
  const deck = await storage.createDeck('Spanish')
  await renderRoute({
    storage,
    path: '/decks/:deckId',
    entry: `/decks/${deck.id}`,
    element: <DeckDetailPage />,
    extraRoutes: [{ path: '/decks/:deckId/options', element: <DeckSettingsPage /> }],
  })

  await page.getByRole('button', { name: 'Options' }).click()
  await expect.element(page.getByText('Deck options')).toBeVisible()
})

test('Options shows alongside Add card when the deck has cards', async () => {
  const storage = freshStorage()
  const deck = await storage.createDeck('Spanish')
  await storage.createCard(deck.id, 'front', 'back')
  await renderRoute({
    storage,
    path: '/decks/:deckId',
    entry: `/decks/${deck.id}`,
    element: <DeckDetailPage />,
  })

  await expect.element(page.getByRole('button', { name: 'Options' })).toBeVisible()
  await expect.element(page.getByRole('button', { name: '+ Add card' })).toBeVisible()
})
