import { test, expect } from 'vitest'
import { page } from 'vitest/browser'
import { ReviewPage } from './ReviewPage'
import { freshStorage } from '../../test/seed'
import { renderRoute } from '../../test/renderRoute'

test('revealing flips the card (data-revealed) and shows grade buttons', async () => {
  const storage = freshStorage()
  const deck = await storage.createDeck('TypeScript')
  await storage.createCard(deck.id, 'Q?', 'A — the answer.')
  await renderRoute({
    storage,
    entry: `/decks/${deck.id}/study`,
    path: '/decks/:deckId/study',
    element: <ReviewPage />,
  })
  await expect.element(page.getByTestId('flip')).toHaveAttribute('data-revealed', 'false')
  await page.getByRole('button', { name: 'Show answer', exact: false }).click()
  await expect.element(page.getByTestId('flip')).toHaveAttribute('data-revealed', 'true')
  await expect.element(page.getByRole('button', { name: 'Good', exact: false })).toBeVisible()
})
