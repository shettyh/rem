import { test, expect, afterEach } from 'vitest'
import { page } from 'vitest/browser'
import { DeckListPage } from '../features/decks/DeckListPage'
import { DeckDetailPage } from '../features/cards/DeckDetailPage'
import { CardEditorPage } from '../features/cards/CardEditorPage'
import { ReviewPage } from '../features/review/ReviewPage'
import { SettingsPage } from '../features/settings/SettingsPage'
import { StatsPage } from '../features/stats/StatsPage'
import { freshStorage, MS_PER_DAY } from './seed'
import { renderRoute } from './renderRoute'
import { shoot } from './screenshot'

const CODE_BACK = 'Use a guard:\n\n```ts\nfunction f(x: unknown) {\n  if (typeof x === "string") return x\n}\n```'

type Storage = ReturnType<typeof freshStorage>

async function pushToFuture(storage: Storage, cardId: string) {
  const card = await storage.getCard(cardId)
  if (!card) throw new Error('seed card missing')
  await storage.updateCard(cardId, { scheduling: { ...card.scheduling, due: Date.now() + 10 * MS_PER_DAY } })
}

const scenarios: { name: string; run: () => Promise<void> }[] = [
  {
    name: 'deck-list',
    run: async () => {
      const storage = freshStorage()
      await storage.createDeck('TypeScript')
      await storage.createDeck('Spanish vocabulary')
      await renderRoute({ storage, entry: '/', path: '/', element: <DeckListPage /> })
      await expect.element(page.getByText('Your decks')).toBeVisible()
    },
  },
  {
    name: 'deck-list-empty',
    run: async () => {
      const storage = freshStorage()
      await renderRoute({ storage, entry: '/', path: '/', element: <DeckListPage /> })
      await expect.element(page.getByText('No decks yet', { exact: false })).toBeVisible()
    },
  },
  {
    name: 'deck-detail',
    run: async () => {
      const storage = freshStorage()
      const deck = await storage.createDeck('TypeScript')
      await storage.createCard(deck.id, 'What is the `never` type?', 'The empty type.')
      await storage.createCard(deck.id, 'What does `satisfies` do?', 'Checks without widening.')
      await renderRoute({ storage, entry: `/decks/${deck.id}`, path: '/decks/:deckId', element: <DeckDetailPage /> })
      await expect.element(page.getByText('Study', { exact: false })).toBeVisible()
    },
  },
  {
    name: 'deck-detail-nothing-due',
    run: async () => {
      const storage = freshStorage()
      const deck = await storage.createDeck('TypeScript')
      const card = await storage.createCard(deck.id, 'Reviewed already', 'Yes')
      await pushToFuture(storage, card.id)
      await renderRoute({ storage, entry: `/decks/${deck.id}`, path: '/decks/:deckId', element: <DeckDetailPage /> })
      await expect.element(page.getByText('All caught up today', { exact: false })).toBeVisible()
    },
  },
  {
    name: 'deck-detail-empty',
    run: async () => {
      const storage = freshStorage()
      const deck = await storage.createDeck('Empty deck')
      await renderRoute({ storage, entry: `/decks/${deck.id}`, path: '/decks/:deckId', element: <DeckDetailPage /> })
      await expect.element(page.getByText('No cards yet', { exact: false })).toBeVisible()
    },
  },
  {
    name: 'settings',
    run: async () => {
      const storage = freshStorage()
      await storage.createDeck('TypeScript')
      await storage.createDeck('Spanish vocabulary')
      await renderRoute({ storage, entry: '/settings', path: '/settings', element: <SettingsPage /> })
      await expect.element(page.getByRole('heading', { name: 'Settings' })).toBeVisible()
      await expect.element(page.getByLabelText('Settings')).toBeVisible() // header gear link
    },
  },
  {
    name: 'stats',
    run: async () => {
      const storage = freshStorage()
      const typescript = await storage.createDeck('TypeScript')
      const spanish = await storage.createDeck('Spanish vocabulary')
      const tsCard = await storage.createCard(typescript.id, 'q', 'a')
      const esCard = await storage.createCard(spanish.id, 'q', 'a')
      const yesterday = new Date()
      yesterday.setDate(yesterday.getDate() - 1)
      await storage.commitReview({ cardId: tsCard.id, deckId: typescript.id, patch: {}, reviewedAt: yesterday.getTime(), fsrsGrade: 'again' })
      await storage.commitReview({ cardId: tsCard.id, deckId: typescript.id, patch: {}, reviewedAt: Date.now() - 1000, fsrsGrade: 'good' })
      await storage.commitReview({ cardId: esCard.id, deckId: spanish.id, patch: {}, reviewedAt: Date.now(), fsrsGrade: 'easy' })
      await renderRoute({ storage, entry: '/stats', path: '/stats', element: <StatsPage /> })
      await expect.element(page.getByLabelText('FSRS reviews')).toHaveTextContent('3')
    },
  },
  {
    name: 'card-editor-new',
    run: async () => {
      const storage = freshStorage()
      const deck = await storage.createDeck('TypeScript')
      await renderRoute({
        storage,
        entry: `/decks/${deck.id}`,
        path: '/decks/:deckId',
        element: <DeckDetailPage />,
        extraRoutes: [{ path: '/decks/:deckId/cards/new', element: <CardEditorPage /> }],
      })
      await page.getByRole('button', { name: 'Add your first card', exact: false }).click()
      await expect.element(page.getByText('New card')).toBeVisible()
    },
  },
  {
    name: 'card-editor-edit',
    run: async () => {
      const storage = freshStorage()
      const deck = await storage.createDeck('TypeScript')
      await storage.createCard(deck.id, 'How to narrow `unknown`?', CODE_BACK)
      await renderRoute({
        storage,
        entry: `/decks/${deck.id}`,
        path: '/decks/:deckId',
        element: <DeckDetailPage />,
        extraRoutes: [{ path: '/decks/:deckId/cards/:cardId/edit', element: <CardEditorPage /> }],
      })
      await page.getByText('How to narrow').click()
      await expect.element(page.getByText('Edit card')).toBeVisible()
    },
  },
  {
    name: 'review-question',
    run: async () => {
      const storage = freshStorage()
      const deck = await storage.createDeck('TypeScript')
      await storage.createCard(deck.id, 'What is the `never` type?', 'The empty type.')
      await renderRoute({ storage, entry: `/decks/${deck.id}/study`, path: '/decks/:deckId/study', element: <ReviewPage /> })
      await expect.element(page.getByRole('button', { name: 'Show answer', exact: false })).toBeVisible()
    },
  },
  {
    name: 'review-answer',
    run: async () => {
      const storage = freshStorage()
      const deck = await storage.createDeck('TypeScript')
      await storage.createCard(deck.id, 'What is the `never` type?', 'The empty type — no value is assignable to it.')
      await renderRoute({ storage, entry: `/decks/${deck.id}/study`, path: '/decks/:deckId/study', element: <ReviewPage /> })
      await page.getByRole('button', { name: 'Show answer', exact: false }).click()
      await expect.element(page.getByText('no value is assignable', { exact: false })).toBeVisible()
    },
  },
  {
    name: 'review-nothing-due',
    run: async () => {
      const storage = freshStorage()
      const deck = await storage.createDeck('TypeScript')
      const card = await storage.createCard(deck.id, 'Already reviewed', 'Yes')
      await pushToFuture(storage, card.id)
      await renderRoute({ storage, entry: `/decks/${deck.id}/study`, path: '/decks/:deckId/study', element: <ReviewPage /> })
      await expect.element(page.getByRole('heading', { name: 'Nothing due' })).toBeVisible()
    },
  },
  {
    name: 'review-complete',
    run: async () => {
      const storage = freshStorage()
      const deck = await storage.createDeck('TypeScript')
      await storage.createCard(deck.id, 'Only card', 'Done')
      await renderRoute({ storage, entry: `/decks/${deck.id}/study`, path: '/decks/:deckId/study', element: <ReviewPage /> })
      // Default learnSteps ('1m 10m') means a new card needs both learning steps graded before it graduates.
      await page.getByRole('button', { name: 'Show answer', exact: false }).click()
      await page.getByRole('button', { name: 'Good', exact: false }).click()
      await page.getByRole('button', { name: 'Show answer', exact: false }).click()
      await page.getByRole('button', { name: 'Good', exact: false }).click()
      await expect.element(page.getByText('Review complete', { exact: false })).toBeVisible()
    },
  },
]

afterEach(() => {
  delete document.documentElement.dataset.theme
})

for (const theme of ['light', 'dark'] as const) {
  for (const sc of scenarios) {
    test(`${sc.name} — ${theme}`, async () => {
      if (theme === 'dark') document.documentElement.dataset.theme = 'dark'
      await sc.run()
      await shoot('screen', theme === 'dark' ? `${sc.name}-dark` : sc.name)
    })
  }
}
