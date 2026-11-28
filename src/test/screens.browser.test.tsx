import { test, expect, afterEach } from 'vitest'
import { page, userEvent } from 'vitest/browser'
import { DeckListPage } from '../features/decks/DeckListPage'
import { DeckDetailPage } from '../features/cards/DeckDetailPage'
import { CardEditorPage } from '../features/cards/CardEditorPage'
import { ReviewPage } from '../features/review/ReviewPage'
import { SettingsPage } from '../features/settings/SettingsPage'
import { StatsPage } from '../features/stats/StatsPage'
import { DraftInboxPage } from '../features/drafts/DraftInboxPage'
import { freshStorage, MS_PER_DAY } from './seed'
import { renderRoute } from './renderRoute'
import { shoot } from './screenshot'

const CODE_BACK = 'Use a guard:\n\n```ts\nfunction f(x: unknown) {\n  if (typeof x === "string") return x\n}\n```'
const RICH_EDITOR_BACK = `## Readable answer

> Preserve hierarchy when an answer mixes several kinds of content.

- Keep prose at a comfortable measure.
- Keep grading controls available.
- Let wide content scroll locally.

\`\`\`ts
function normalize(input: string): string {
  return input.trim().toLowerCase()
}
\`\`\`

[Read the WCAG text-spacing guidance](https://www.w3.org/WAI/WCAG22/Understanding/text-spacing.html)`
const RICH_BACK = `${RICH_EDITOR_BACK}

| Content | Treatment |
| --- | --- |
| Prose | Comfortable reading size |
| Code | Locally scrollable block |`

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
    name: 'deck-list-due',
    run: async () => {
      const storage = freshStorage()
      const deck = await storage.createDeck('TypeScript')
      await storage.createCard(deck.id, 'What is the `never` type?', 'The empty type.')
      await renderRoute({ storage, entry: '/', path: '/', element: <DeckListPage /> })
      await expect.element(page.getByLabelText('Review summary')).toBeVisible()
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
      await storage.createCard(deck.id, 'What is the `never` type?', 'The empty type.', ['types'])
      await storage.createCard(deck.id, 'What does `satisfies` do?', 'Checks without widening.', ['syntax'])
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
      await expect.element(page.getByText('Caught up', { exact: false })).toBeVisible()
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
      const now = Date.now()
      storage.setStats({
        historyCount: 3,
        totalReviews: 3,
        recallRate: 2 / 3,
        currentStreakDays: 2,
        activeDays: 2,
        grades: { again: 1, hard: 0, good: 1, easy: 1 },
        daily: Array.from({ length: 30 }, (_, idx) => ({
          day: `day-${idx}`,
          timestampMs: now - (29 - idx) * MS_PER_DAY,
          count: idx === 29 ? 2 : idx === 28 ? 1 : 0,
        })),
        byDeck: [
          { deckId: typescript.id, deckName: typescript.name, deckColor: typescript.color, reviews: 2, recallRate: 0.5 },
          { deckId: spanish.id, deckName: spanish.name, deckColor: spanish.color, reviews: 1, recallRate: 1 },
        ],
        upcoming7d: [],
      })
      await renderRoute({ storage, entry: '/stats', path: '/stats', element: <StatsPage /> })
      await expect.element(page.getByLabelText('FSRS reviews')).toMatchTextContent('3')
    },
  },
  {
    name: 'draft-inbox-question',
    run: async () => {
      const storage = freshStorage()
      const deck = await storage.createDeck('TypeScript')
      await storage.proposeDrafts(deck.id, [{
        front: 'Why does optimistic concurrency protect review commits?',
        back: 'It prevents two open clients from grading the same stale card.',
        tags: ['concurrency', 'review'],
        rationale: 'This is a durable correctness invariant.',
        sources: [{ locator: 'src-tauri/crates/rem-core/src/review_store.rs', label: 'Review store' }],
      }], { proposedBy: 'pi' })
      await renderRoute({ storage, entry: '/drafts', path: '/drafts', element: <DraftInboxPage /> })
      await expect.element(page.getByRole('button', { name: 'Reveal proposal' })).toBeVisible()
    },
  },
  {
    name: 'draft-inbox-revealed',
    run: async () => {
      const storage = freshStorage()
      const deck = await storage.createDeck('TypeScript')
      await storage.proposeDrafts(deck.id, [{
        front: 'Why does optimistic concurrency protect review commits?',
        back: 'It prevents two open clients from grading the same stale card.',
        tags: ['concurrency', 'review'],
        rationale: 'This is a durable correctness invariant.',
        sources: [{ locator: 'src-tauri/crates/rem-core/src/review_store.rs', label: 'Review store' }],
      }], { proposedBy: 'pi' })
      await renderRoute({ storage, entry: '/drafts', path: '/drafts', element: <DraftInboxPage /> })
      await page.getByRole('button', { name: 'Reveal proposal' }).click()
      await expect.element(page.getByRole('button', { name: 'Accept draft' })).toBeVisible()
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
      await storage.createCard(deck.id, 'How to narrow `unknown`?', CODE_BACK, ['types', 'guards'])
      await renderRoute({
        storage,
        entry: `/decks/${deck.id}`,
        path: '/decks/:deckId',
        element: <DeckDetailPage />,
        extraRoutes: [{ path: '/decks/:deckId/cards/:cardId/edit', element: <CardEditorPage /> }],
      })
      await page.getByText('How to narrow', { exact: false }).click()
      await expect.element(page.getByText('Edit card')).toBeVisible()
    },
  },
  {
    name: 'card-editor-rich',
    run: async () => {
      const storage = freshStorage()
      const deck = await storage.createDeck('TypeScript')
      const card = await storage.createCard(deck.id, 'How should a rich answer read?', RICH_EDITOR_BACK)
      await renderRoute({
        storage,
        entry: `/decks/${deck.id}/cards/${card.id}/edit`,
        path: '/decks/:deckId/cards/:cardId/edit',
        element: <CardEditorPage />,
      })
      await expect.element(page.getByText('Preserve hierarchy', { exact: false })).toBeVisible()
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
    name: 'review-rich-answer',
    run: async () => {
      const storage = freshStorage()
      const deck = await storage.createDeck('TypeScript')
      await storage.createCard(deck.id, 'How should a rich answer read?', RICH_BACK)
      await renderRoute({ storage, entry: `/decks/${deck.id}/study`, path: '/decks/:deckId/study', element: <ReviewPage /> })
      await page.getByRole('button', { name: 'Show answer', exact: false }).click()
      await expect.element(page.getByRole('heading', { name: 'Readable answer' })).toBeVisible()
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
      await page.getByRole('button', { name: 'Show answer', exact: false }).click()
      await page.getByRole('button', { name: 'Good', exact: false }).click()
      await expect.element(page.getByText('Review complete', { exact: false })).toBeVisible()
    },
  },
  {
    name: 'search-modal',
    run: async () => {
      const storage = freshStorage()
      const rust = await storage.createDeck('Rust')
      const sql = await storage.createDeck('SQL::Internals')
      await storage.createCard(rust.id, 'Ownership Rules', 'Each value in Rust has an owner.', ['ownership'])
      await storage.createCard(rust.id, 'Cargo basics', 'cargo build', ['cargo'])
      await storage.createCard(sql.id, 'What is WAL mode?', 'Write-Ahead Logging.', ['sqlite'])
      await renderRoute({ storage, entry: `/decks/${rust.id}`, path: '/decks/:deckId', element: <DeckDetailPage /> })
      await expect.element(page.getByText('Ownership Rules')).toBeVisible()
      await page.getByRole('button', { name: 'Search', exact: false }).click()
      await expect.element(page.getByText('Jump to deck')).toBeVisible()
    },
  },
  {
    name: 'search-modal-results',
    run: async () => {
      const storage = freshStorage()
      const rust = await storage.createDeck('Rust')
      const sql = await storage.createDeck('SQL::Internals')
      await storage.createCard(rust.id, 'Ownership Rules', 'Each value in Rust has an owner.', ['ownership'])
      await storage.createCard(rust.id, 'Rust Pin<P> guarantee', 'The pointee cannot be moved in memory.', ['async', 'rust'])
      await storage.createCard(sql.id, 'What is WAL mode?', 'Write-Ahead Logging.', ['sqlite'])
      await renderRoute({ storage, entry: `/decks/${rust.id}`, path: '/decks/:deckId', element: <DeckDetailPage /> })
      await expect.element(page.getByText('Ownership Rules')).toBeVisible()
      await page.getByRole('button', { name: 'Search', exact: false }).click()
      const input = page.getByRole('combobox', { name: 'Search all cards' })
      await expect.element(input).toBeVisible()
      await userEvent.type(input.element(), 'Rust')
      const results = page.getByRole('listbox', { name: 'Search results' })
      await expect.element(results).toHaveAttribute('aria-busy', 'false')
      await expect.element(results.getByText('Rust Pin<P> guarantee')).toBeVisible()
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
