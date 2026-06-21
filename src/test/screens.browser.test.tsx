import { test, expect } from 'vitest'
import { page } from 'vitest/browser'
import { DeckListPage } from '../features/decks/DeckListPage'
import { DeckDetailPage } from '../features/cards/DeckDetailPage'
import { CardEditorPage } from '../features/cards/CardEditorPage'
import { ReviewPage } from '../features/review/ReviewPage'
import { freshStorage, MS_PER_DAY } from './seed'
import { renderRoute } from './renderRoute'
import { shoot } from './screenshot'

const CODE_BACK = 'Use a guard:\n\n```ts\nfunction f(x: unknown) {\n  if (typeof x === "string") return x\n}\n```'

async function pushToFuture(storage: ReturnType<typeof freshStorage>, cardId: string) {
  const card = await storage.getCard(cardId)
  if (!card) throw new Error('seed card missing')
  await storage.updateCard(cardId, { scheduling: { ...card.scheduling, due: Date.now() + 10 * MS_PER_DAY } })
}

test('deck list — with decks', async () => {
  const storage = freshStorage()
  await storage.createDeck('TypeScript')
  await storage.createDeck('Spanish vocabulary')
  await renderRoute({ storage, entry: '/', path: '/', element: <DeckListPage /> })
  await expect.element(page.getByText('TypeScript')).toBeVisible()
  await shoot('screen', 'deck-list')
})

test('deck list — empty', async () => {
  const storage = freshStorage()
  await renderRoute({ storage, entry: '/', path: '/', element: <DeckListPage /> })
  await expect.element(page.getByText('No decks yet', { exact: false })).toBeVisible()
  await shoot('screen', 'deck-list-empty')
})

test('deck detail — with due cards', async () => {
  const storage = freshStorage()
  const deck = await storage.createDeck('TypeScript')
  await storage.createCard(deck.id, 'What is the `never` type?', 'The empty type.')
  await storage.createCard(deck.id, 'What does `satisfies` do?', 'Checks without widening.')
  await renderRoute({ storage, entry: `/decks/${deck.id}`, path: '/decks/:deckId', element: <DeckDetailPage /> })
  await expect.element(page.getByText('TypeScript')).toBeVisible()
  await expect.element(page.getByText('Study', { exact: false })).toBeVisible()
  await shoot('screen', 'deck-detail')
})

test('deck detail — nothing due', async () => {
  const storage = freshStorage()
  const deck = await storage.createDeck('TypeScript')
  const card = await storage.createCard(deck.id, 'Reviewed already', 'Yes')
  await pushToFuture(storage, card.id)
  await renderRoute({ storage, entry: `/decks/${deck.id}`, path: '/decks/:deckId', element: <DeckDetailPage /> })
  await expect.element(page.getByText('All caught up today', { exact: false })).toBeVisible()
  await shoot('screen', 'deck-detail-nothing-due')
})

test('deck detail — empty', async () => {
  const storage = freshStorage()
  const deck = await storage.createDeck('Empty deck')
  await renderRoute({ storage, entry: `/decks/${deck.id}`, path: '/decks/:deckId', element: <DeckDetailPage /> })
  await expect.element(page.getByText('No cards yet', { exact: false })).toBeVisible()
  await shoot('screen', 'deck-detail-empty')
})

test('card editor — new', async () => {
  const storage = freshStorage()
  const deck = await storage.createDeck('TypeScript')
  await renderRoute({
    storage,
    entry: `/decks/${deck.id}/cards/new`,
    path: '/decks/:deckId/cards/new',
    element: <CardEditorPage />,
  })
  await expect.element(page.getByText('New card')).toBeVisible()
  await shoot('screen', 'card-editor-new')
})

test('card editor — edit with code block', async () => {
  const storage = freshStorage()
  const deck = await storage.createDeck('TypeScript')
  const card = await storage.createCard(deck.id, 'How to narrow `unknown`?', CODE_BACK)
  await renderRoute({
    storage,
    entry: `/decks/${deck.id}/cards/${card.id}`,
    path: '/decks/:deckId/cards/:cardId',
    element: <CardEditorPage />,
  })
  await expect.element(page.getByText('Edit card')).toBeVisible()
  await expect.element(page.getByText('narrow', { exact: false })).toBeVisible()
  await shoot('screen', 'card-editor-edit')
})

test('review — question side', async () => {
  const storage = freshStorage()
  const deck = await storage.createDeck('TypeScript')
  await storage.createCard(deck.id, 'What is the `never` type?', 'The empty type.')
  await renderRoute({ storage, entry: `/decks/${deck.id}/study`, path: '/decks/:deckId/study', element: <ReviewPage /> })
  await expect.element(page.getByRole('button', { name: 'Show answer', exact: false })).toBeVisible()
  await shoot('screen', 'review-question')
})

test('review — answer revealed', async () => {
  const storage = freshStorage()
  const deck = await storage.createDeck('TypeScript')
  await storage.createCard(deck.id, 'What is the `never` type?', 'The empty type — no value is assignable to it.')
  await renderRoute({ storage, entry: `/decks/${deck.id}/study`, path: '/decks/:deckId/study', element: <ReviewPage /> })
  await page.getByRole('button', { name: 'Show answer', exact: false }).click()
  await expect.element(page.getByText('no value is assignable', { exact: false })).toBeVisible()
  await shoot('screen', 'review-answer')
})

test('review — nothing due', async () => {
  const storage = freshStorage()
  const deck = await storage.createDeck('TypeScript')
  const card = await storage.createCard(deck.id, 'Already reviewed', 'Yes')
  await pushToFuture(storage, card.id)
  await renderRoute({ storage, entry: `/decks/${deck.id}/study`, path: '/decks/:deckId/study', element: <ReviewPage /> })
  await expect.element(page.getByText('Nothing due in this deck', { exact: false })).toBeVisible()
  await shoot('screen', 'review-nothing-due')
})

test('review — session complete', async () => {
  const storage = freshStorage()
  const deck = await storage.createDeck('TypeScript')
  await storage.createCard(deck.id, 'Only card', 'Done')
  await renderRoute({ storage, entry: `/decks/${deck.id}/study`, path: '/decks/:deckId/study', element: <ReviewPage /> })
  await page.getByRole('button', { name: 'Show answer', exact: false }).click()
  await page.getByRole('button', { name: 'Good', exact: false }).click()
  await expect.element(page.getByText('Review complete', { exact: false })).toBeVisible()
  await shoot('screen', 'review-complete')
})
