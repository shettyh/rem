import { test, expect } from 'vitest'
import { page } from 'vitest/browser'
import { DeckListPage } from './DeckListPage'
import { freshStorage } from '../../test/seed'
import { renderRoute } from '../../test/renderRoute'

test('creates a deck using the FSRS scheduler', async () => {
  const storage = freshStorage()
  await renderRoute({ storage, entry: '/', path: '/', element: <DeckListPage /> })

  await page.getByLabelText('New deck name').fill('Algorithms')
  await page.getByRole('button', { name: 'Add deck' }).click()

  await expect.poll(async () => (await storage.listDecks())[0]?.schedulerKind).toBe('fsrs')
})

test('uses the selected deck color on Today', async () => {
  const storage = freshStorage()
  const deck = await storage.createDeck('Spanish')
  await storage.updateDeck(deck.id, { color: '#2fa86b' })
  await renderRoute({ storage, entry: '/', path: '/', element: <DeckListPage /> })

  const deckLink = page.getByRole('link', { name: /FSRS.*Spanish/ })
  await expect.element(deckLink).toBeVisible()
  const colorBar = deckLink.element().querySelector<HTMLElement>('.deck-card-bar')
  expect(colorBar?.style.background).toBe('rgb(47, 168, 107)')
})
