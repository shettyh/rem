import { afterEach, test, expect } from 'vitest'
import { page } from 'vitest/browser'
import { DeckListPage } from './DeckListPage'
import { freshStorage } from '../../test/seed'
import { renderRoute } from '../../test/renderRoute'

afterEach(async () => {
  await page.viewport(1280, 800)
})

test('creates a deck using the FSRS scheduler', async () => {
  const storage = freshStorage()
  await renderRoute({ storage, entry: '/', path: '/', element: <DeckListPage /> })

  await page.getByLabelText('New deck name').fill('Algorithms')
  await page.getByRole('button', { name: 'Add deck' }).click()

  await expect.poll(async () => (await storage.listDecks())[0]?.schedulerKind).toBe('fsrs')
})

test('uses the selected deck color as a small identity marker', async () => {
  const storage = freshStorage()
  const deck = await storage.createDeck('Spanish')
  await storage.updateDeck(deck.id, { color: '#2fa86b' })
  await renderRoute({ storage, entry: '/', path: '/', element: <DeckListPage /> })

  const deckList = page.getByLabelText('Decks')
  await expect.element(deckList).toBeVisible()
  const deckLink = deckList.element().querySelector<HTMLAnchorElement>('.deck-list-row')!
  const colorMarker = deckLink.querySelector<HTMLElement>('.deck-list-dot')
  expect(colorMarker?.style.background).toBe('rgb(47, 168, 107)')
  await expect.element(page.getByText('FSRS')).not.toBeInTheDocument()
})

test('presents due cards with one compact action instead of a gradient panel', async () => {
  const storage = freshStorage()
  const deck = await storage.createDeck('Spanish')
  await storage.createCard(deck.id, 'Hola', 'Hello')
  await renderRoute({ storage, entry: '/', path: '/', element: <DeckListPage /> })

  const summary = page.getByLabelText('Review summary')
  const action = page.getByRole('link', { name: /Start review/ })
  await expect.element(summary).toBeVisible()
  await expect.element(action).toBeVisible()

  expect(getComputedStyle(summary.element()).backgroundImage).toBe('none')
  expect(action.element().getBoundingClientRect().width).toBeLessThan(
    summary.element().getBoundingClientRect().width / 2,
  )
})

test('Today stays within the minimum supported window width', async () => {
  await page.viewport(760, 720)
  const storage = freshStorage()
  const deck = await storage.createDeck('A long deck name that still fits cleanly')
  await storage.createCard(deck.id, 'Question', 'Answer')
  await renderRoute({ storage, entry: '/', path: '/', element: <DeckListPage /> })
  await expect.element(page.getByLabelText('Review summary')).toBeVisible()

  const content = document.querySelector<HTMLElement>('.content')!
  expect(content.scrollWidth).toBe(content.clientWidth)
})
