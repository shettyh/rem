import { afterEach, test, expect } from 'vitest'
import { page } from 'vitest/browser'
import { DeckDetailPage } from './DeckDetailPage'
import { DeckSettingsPage } from '../decks/DeckSettingsPage'
import { freshStorage } from '../../test/seed'
import { renderRoute } from '../../test/renderRoute'

afterEach(async () => {
  await page.viewport(1280, 800)
})

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

test('groups deck stats and cards into single quiet surfaces', async () => {
  const storage = freshStorage()
  const deck = await storage.createDeck('Spanish')
  await storage.createCard(deck.id, 'Hola', 'Hello')
  await storage.createCard(deck.id, 'Adiós', 'Goodbye')
  await renderRoute({
    storage,
    path: '/decks/:deckId',
    entry: `/decks/${deck.id}`,
    element: <DeckDetailPage />,
  })

  const summary = page.getByLabelText('Deck summary')
  const cardList = page.getByLabelText('Cards')
  await expect.element(summary).toBeVisible()
  await expect.element(cardList).toBeVisible()
  expect(summary.element().children).toHaveLength(3)
  expect(cardList.element().children).toHaveLength(2)

  const firstRow = cardList.element().querySelector<HTMLElement>('.card-row')!
  expect(getComputedStyle(firstRow).borderRadius).toBe('0px')
  const routineStatus = cardList.element().querySelector<HTMLElement>('.status-new')!
  expect(getComputedStyle(routineStatus).backgroundColor).toBe('rgba(0, 0, 0, 0)')
})

test('deck detail stays within the minimum supported window width', async () => {
  await page.viewport(760, 720)
  const storage = freshStorage()
  const deck = await storage.createDeck('A long deck name that still fits cleanly')
  await storage.createCard(
    deck.id,
    'A long card question that should truncate cleanly',
    'A long answer preview that should not make the row overflow',
    ['a-long-tag'],
  )
  await renderRoute({
    storage,
    path: '/decks/:deckId',
    entry: `/decks/${deck.id}`,
    element: <DeckDetailPage />,
  })
  await expect.element(page.getByLabelText('Cards')).toBeVisible()

  const content = document.querySelector<HTMLElement>('.content')!
  expect(content.scrollWidth).toBe(content.clientWidth)
})

test('shows user tags and filters cards by tag', async () => {
  const storage = freshStorage()
  const deck = await storage.createDeck('Spanish')
  await storage.createCard(deck.id, 'Ser vs estar', 'Two forms of “to be”', ['grammar'])
  await storage.createCard(deck.id, 'Hola', 'Hello', ['vocabulary'])
  await renderRoute({
    storage,
    path: '/decks/:deckId',
    entry: `/decks/${deck.id}`,
    element: <DeckDetailPage />,
  })

  await expect.element(page.getByLabelText('Tags: grammar')).toBeVisible()
  await page.getByLabelText('Filter by tag').selectOptions('grammar')

  await expect.element(page.getByText('Ser vs estar')).toBeVisible()
  await expect.element(page.getByText('Hola', { exact: true })).not.toBeInTheDocument()
  await expect.element(page.getByText('1 of 2 cards')).toBeVisible()
})

test('searches cards and lets the user clear active filters', async () => {
  const storage = freshStorage()
  const deck = await storage.createDeck('Spanish')
  await storage.createCard(deck.id, 'Ser vs estar', 'Two forms of “to be”')
  await storage.createCard(deck.id, 'Hola', 'Hello')
  await renderRoute({
    storage,
    path: '/decks/:deckId',
    entry: `/decks/${deck.id}`,
    element: <DeckDetailPage />,
  })

  await page.getByLabelText('Search cards').fill('estar')
  await expect.element(page.getByText('Ser vs estar')).toBeVisible()
  await expect.element(page.getByText('Hola', { exact: true })).not.toBeInTheDocument()

  await page.getByLabelText('Search cards').fill('nonexistent')
  await expect.element(page.getByRole('heading', { name: 'No matching cards' })).toBeVisible()
  await page.getByRole('button', { name: 'Clear filters' }).click()
  await expect.element(page.getByText('Hola', { exact: true })).toBeVisible()
})

test('shows caught-up pill beside title, code snippet preview, and status filter pills', async () => {
  const storage = freshStorage()
  const deck = await storage.createDeck('Rust')
  const card = await storage.createCard(
    deck.id,
    'Main program',
    '```rust\nfn main() {\n  println!("hi");\n}\n```',
  )
  await storage.updateCard(card.id, {
    scheduling: { ...card.scheduling, reps: 2, due: Date.now() + 86_400_000 * 10 },
  })
  await renderRoute({
    storage,
    path: '/decks/:deckId',
    entry: `/decks/${deck.id}`,
    element: <DeckDetailPage />,
  })

  await expect.element(page.getByText('Caught up')).toBeVisible()
  await expect.element(page.getByRole('button', { name: 'Custom study' })).toBeVisible()
  await expect.element(page.getByText('fn main() {')).toBeVisible()
  await expect.element(page.getByText('rust', { exact: true })).toBeVisible()

  await page.getByRole('button', { name: 'Suspend' }).click()
  await expect.element(page.getByText('suspended')).toBeVisible()

  await page.getByRole('button', { name: 'Suspended', exact: true }).click()
  await expect.element(page.getByText('Main program')).toBeVisible()
})
