import { test, expect, vi, afterEach } from 'vitest'
import { page } from 'vitest/browser'
import { Link } from 'react-router-dom'
import { ReviewPage } from './ReviewPage'
import { freshStorage } from '../../test/seed'
import { renderRoute } from '../../test/renderRoute'
import { getScheduler, MS_PER_DAY } from '../../domain/scheduler'
import { DEFAULT_DECK_SETTINGS } from '../../domain/models'
import { localDay } from './day'

test('revealing shows the answer and grade buttons', async () => {
  const storage = freshStorage()
  const deck = await storage.createDeck('TypeScript')
  await storage.createCard(deck.id, 'Q?', 'A — the answer.')
  await renderRoute({
    storage,
    entry: `/decks/${deck.id}/study`,
    path: '/decks/:deckId/study',
    element: <ReviewPage />,
  })

  await expect.element(page.getByRole('button', { name: 'Show answer', exact: false })).toBeVisible()
  await expect.element(page.getByText('A — the answer.')).not.toBeInTheDocument()

  await page.getByRole('button', { name: 'Show answer', exact: false }).click()

  await expect.element(page.getByText('A — the answer.')).toBeVisible()
  await expect.element(page.getByRole('button', { name: 'Good', exact: false })).toBeVisible()
})

afterEach(() => {
  vi.restoreAllMocks()
})

test('scheduling rejection shows recoverable error and no grade buttons', async () => {
  const sched = getScheduler()
  vi.spyOn(sched, 'previewNextStates').mockRejectedValueOnce(new Error('boom'))

  const storage = freshStorage()
  const deck = await storage.createDeck('TypeScript')
  await storage.createCard(deck.id, 'Q?', 'A — the answer.')
  await renderRoute({
    storage,
    entry: `/decks/${deck.id}/study`,
    path: '/decks/:deckId/study',
    element: <ReviewPage />,
  })

  await page.getByRole('button', { name: 'Show answer', exact: false }).click()

  await expect.element(page.getByText("Couldn't schedule this card.")).toBeVisible()
  await expect.element(page.getByRole('button', { name: 'Good', exact: false })).not.toBeInTheDocument()
})

test('retry after scheduling failure resolves grade buttons', async () => {
  const sched = getScheduler()
  vi.spyOn(sched, 'previewNextStates').mockRejectedValueOnce(new Error('boom'))

  const storage = freshStorage()
  const deck = await storage.createDeck('TypeScript')
  await storage.createCard(deck.id, 'Q?', 'A — the answer.')
  await renderRoute({
    storage,
    entry: `/decks/${deck.id}/study`,
    path: '/decks/:deckId/study',
    element: <ReviewPage />,
  })

  await page.getByRole('button', { name: 'Show answer', exact: false }).click()
  await expect.element(page.getByRole('button', { name: 'Retry' })).toBeVisible()

  await page.getByRole('button', { name: 'Retry' }).click()

  await expect.element(page.getByRole('button', { name: 'Good', exact: false })).toBeVisible()
  await expect.element(page.getByText("Couldn't schedule this card.")).not.toBeInTheDocument()
})

test('grading twice before the first persist resolves applies only one grade', async () => {
  const storage = freshStorage()
  const deck = await storage.createDeck('TypeScript')
  await storage.createCard(deck.id, 'Q?', 'A — the answer.')
  const spy = vi.spyOn(storage, 'updateCard')

  await renderRoute({
    storage,
    entry: `/decks/${deck.id}/study`,
    path: '/decks/:deckId/study',
    element: <ReviewPage />,
  })

  await page.getByRole('button', { name: 'Show answer', exact: false }).click()
  const good = page.getByRole('button', { name: 'Good', exact: false })
  await expect.element(good).toBeVisible()

  // Two synchronous clicks land in the same tick, before the first grade's async
  // persist resolves and re-renders. Only the first should take effect.
  const el = good.element() as HTMLElement
  el.click()
  el.click()

  await vi.waitFor(() => expect(spy).toHaveBeenCalled())
  expect(spy).toHaveBeenCalledTimes(1)
})

test('a long answer renders after reveal', async () => {
  const storage = freshStorage()
  const deck = await storage.createDeck('Long')
  const longBack =
    Array.from({ length: 40 }, (_, i) => `Paragraph ${i} of the answer with some content.`).join(
      '\n\n',
    ) + '\n\nThe final distinctive closing phrase.'
  await storage.createCard(deck.id, 'Q?', longBack)
  await renderRoute({
    storage,
    entry: `/decks/${deck.id}/study`,
    path: '/decks/:deckId/study',
    element: <ReviewPage />,
  })

  await page.getByRole('button', { name: 'Show answer', exact: false }).click()

  await expect.element(page.getByText('The final distinctive closing phrase.')).toBeVisible()
})

test('enforces newPerDay: only the day\'s new allowance enters the session', async () => {
  const storage = freshStorage()
  const deck = await storage.createDeck('Caps')
  await storage.updateDeck(deck.id, {
    settings: { ...DEFAULT_DECK_SETTINGS, newPerDay: 1, learnSteps: '1m' },
  })
  await storage.createCard(deck.id, 'Q1', 'A1')
  await storage.createCard(deck.id, 'Q2', 'A2')
  await renderRoute({
    storage,
    entry: `/decks/${deck.id}/study`,
    path: '/decks/:deckId/study',
    element: <ReviewPage />,
    extraRoutes: [{
      path: '/decks/:deckId',
      element: <Link to={`/decks/${deck.id}/study`}>Reopen study</Link>,
    }],
  })

  // Two new cards are due, but only one enters: position reads "1 / 1", not "1 / 2".
  await expect.element(page.getByText('1 / 1', { exact: false })).toBeVisible()

  // Grading it out of New bumps newIntroduced and exhausts this session.
  await page.getByRole('button', { name: 'Show answer', exact: false }).click()
  await page.getByRole('button', { name: 'Good', exact: false }).click()
  await vi.waitFor(async () => {
    const stat = await storage.getDailyStat(deck.id, localDay(Date.now()))
    expect(stat.newIntroduced).toBe(1)
  })
  await expect.element(page.getByText('Review complete')).toBeVisible()

  // Reopening on the same day does not grant allowance for the second new card.
  await page.getByRole('link', { name: 'Back to deck' }).click()
  await page.getByRole('link', { name: 'Reopen study' }).click()
  await expect.element(page.getByRole('heading', { name: 'Nothing due' })).toBeVisible()
})

test('grading a Review-state card bumps reviewsDone, not newIntroduced', async () => {
  const storage = freshStorage()
  const deck = await storage.createDeck('Reviews')
  const c = await storage.createCard(deck.id, 'Q?', 'A.')
  await storage.updateCard(c.id, {
    scheduling: { kind: 'fsrs', stability: 10, difficulty: 5, reps: 3, lapses: 0, state: 2, step: 0, lastReview: null, due: 0 },
  })
  await renderRoute({
    storage,
    entry: `/decks/${deck.id}/study`,
    path: '/decks/:deckId/study',
    element: <ReviewPage />,
  })

  await page.getByRole('button', { name: 'Show answer', exact: false }).click()
  await page.getByRole('button', { name: 'Good', exact: false }).click()

  await vi.waitFor(async () => {
    const stat = await storage.getDailyStat(deck.id, localDay(Date.now()))
    expect(stat.reviewsDone).toBe(1)
  })
  const stat = await storage.getDailyStat(deck.id, localDay(Date.now()))
  expect(stat.newIntroduced).toBe(0)
})

test('an Again grade records when the card was forgotten', async () => {
  const storage = freshStorage()
  const deck = await storage.createDeck('Forgotten')
  const card = await storage.createCard(deck.id, 'Q?', 'A.')
  await storage.updateCard(card.id, {
    scheduling: { kind: 'fsrs', stability: 10, difficulty: 5, reps: 3, lapses: 0, state: 2, step: 0, lastReview: 1, due: 0 },
  })
  const before = Date.now()
  await renderRoute({
    storage,
    entry: `/decks/${deck.id}/study`,
    path: '/decks/:deckId/study',
    element: <ReviewPage />,
  })

  await page.getByRole('button', { name: 'Show answer', exact: false }).click()
  await page.getByRole('button', { name: 'Again', exact: false }).click()

  await vi.waitFor(async () => {
    expect((await storage.getCard(card.id))?.lastAgainAt).toBeGreaterThanOrEqual(before)
  })
})

test('custom study can grade a future review card', async () => {
  const storage = freshStorage()
  const deck = await storage.createDeck('Ahead')
  const card = await storage.createCard(deck.id, 'Future question', 'Future answer')
  const originalDue = Date.now() + MS_PER_DAY
  await storage.updateCard(card.id, {
    scheduling: { kind: 'fsrs', stability: 10, difficulty: 5, reps: 3, lapses: 0, state: 2, step: 0, lastReview: 1, due: originalDue },
  })
  await renderRoute({
    storage,
    entry: `/decks/${deck.id}/study?custom=study-ahead&amount=1`,
    path: '/decks/:deckId/study',
    element: <ReviewPage />,
  })

  await expect.element(page.getByText('Future question')).toBeVisible()
  await page.getByRole('button', { name: 'Show answer', exact: false }).click()
  await page.getByRole('button', { name: 'Good', exact: false }).click()

  await vi.waitFor(async () => {
    expect((await storage.getCard(card.id))?.scheduling.due).not.toBe(originalDue)
  })
  await expect.element(page.getByText('Review complete')).toBeVisible()
  await expect.element(page.getByRole('link', { name: 'Back to options' })).toBeVisible()
})

test('preview new reveals cards without scheduling or persistence', async () => {
  const storage = freshStorage()
  const deck = await storage.createDeck('Preview')
  const card = await storage.createCard(deck.id, 'Preview question', 'Preview answer')
  const update = vi.spyOn(storage, 'updateCard')
  const schedule = vi.spyOn(getScheduler(), 'previewNextStates')
  await renderRoute({
    storage,
    entry: `/decks/${deck.id}/study?custom=preview-new&amount=1`,
    path: '/decks/:deckId/study',
    element: <ReviewPage />,
  })

  await page.getByRole('button', { name: 'Show answer', exact: false }).click()
  await expect.element(page.getByText('Preview answer')).toBeVisible()
  await expect.element(page.getByRole('button', { name: 'Next card' })).toBeVisible()
  await expect.element(page.getByRole('button', { name: 'Good', exact: false })).not.toBeInTheDocument()
  await page.getByRole('button', { name: 'Next card' }).click()

  await expect.element(page.getByText('Preview complete')).toBeVisible()
  expect(update).not.toHaveBeenCalled()
  expect(schedule).not.toHaveBeenCalled()
  expect((await storage.getCard(card.id))?.scheduling.state).toBe(0)
  expect(await storage.getDailyStat(deck.id, localDay(Date.now()))).toEqual({ newIntroduced: 0, reviewsDone: 0 })
})

test('suspend leech action removes the lapsed card from this and later sessions', async () => {
  const storage = freshStorage()
  const deck = await storage.createDeck('Leeches')
  await storage.updateDeck(deck.id, {
    settings: { ...DEFAULT_DECK_SETTINGS, leechThreshold: 1, leechAction: 'suspend', relearnSteps: '1m' },
  })
  const card = await storage.createCard(deck.id, 'Hard question', 'Answer')
  await storage.updateCard(card.id, {
    scheduling: { kind: 'fsrs', stability: 10, difficulty: 8, reps: 3, lapses: 0, state: 2, step: 0, lastReview: 1, due: 0 },
  })
  await renderRoute({
    storage,
    entry: `/decks/${deck.id}/study`,
    path: '/decks/:deckId/study',
    element: <ReviewPage />,
    extraRoutes: [{
      path: '/decks/:deckId',
      element: <Link to={`/decks/${deck.id}/study`}>Reopen study</Link>,
    }],
  })

  await page.getByRole('button', { name: 'Show answer', exact: false }).click()
  await page.getByRole('button', { name: 'Again', exact: false }).click()

  await expect.element(page.getByRole('status')).toHaveTextContent('Leech suspended')
  await expect.element(page.getByText('Review complete')).toBeVisible()
  await vi.waitFor(async () => {
    expect(await storage.getCard(card.id)).toMatchObject({ tags: ['leech'], suspended: true })
    expect(await storage.getDailyStat(deck.id, localDay(Date.now()))).toMatchObject({ reviewsDone: 1 })
  })

  await page.getByRole('link', { name: 'Back to deck' }).click()
  await page.getByRole('link', { name: 'Reopen study' }).click()
  await expect.element(page.getByRole('heading', { name: 'Nothing due' })).toBeVisible()
})

test('tag leech action keeps the card active for relearning', async () => {
  const storage = freshStorage()
  const deck = await storage.createDeck('Tagged leeches')
  await storage.updateDeck(deck.id, {
    settings: { ...DEFAULT_DECK_SETTINGS, leechThreshold: 1, leechAction: 'tag', relearnSteps: '1m' },
  })
  const card = await storage.createCard(deck.id, 'Hard question', 'Answer')
  await storage.updateCard(card.id, {
    scheduling: { kind: 'fsrs', stability: 10, difficulty: 8, reps: 3, lapses: 0, state: 2, step: 0, lastReview: 1, due: 0 },
  })
  await renderRoute({
    storage,
    entry: `/decks/${deck.id}/study`,
    path: '/decks/:deckId/study',
    element: <ReviewPage />,
  })

  await page.getByRole('button', { name: 'Show answer', exact: false }).click()
  await page.getByRole('button', { name: 'Again', exact: false }).click()

  await expect.element(page.getByRole('status')).toHaveTextContent('Leech tagged')
  await expect.element(page.getByRole('button', { name: 'Show answer', exact: false })).toBeVisible()
  expect(await storage.getCard(card.id)).toMatchObject({ tags: ['leech'], suspended: false })
})

test('grading a learning-step card persists but bumps no counter', async () => {
  const storage = freshStorage()
  const deck = await storage.createDeck('Learning')
  const c = await storage.createCard(deck.id, 'Q?', 'A.')
  await storage.updateCard(c.id, {
    scheduling: { kind: 'fsrs', stability: 0, difficulty: 0, reps: 0, lapses: 0, state: 1, step: 0, lastReview: null, due: 0 },
  })
  // Spy AFTER the state-setup update so only the grade's persist is counted.
  const updateSpy = vi.spyOn(storage, 'updateCard')
  const bumpSpy = vi.spyOn(storage, 'bumpDailyStat')
  await renderRoute({
    storage,
    entry: `/decks/${deck.id}/study`,
    path: '/decks/:deckId/study',
    element: <ReviewPage />,
  })

  await page.getByRole('button', { name: 'Show answer', exact: false }).click()
  await page.getByRole('button', { name: 'Good', exact: false }).click()

  // The grade persisted the card (updateCard fired) but bumped no daily counter.
  await vi.waitFor(() => expect(updateSpy).toHaveBeenCalled())
  expect(bumpSpy).not.toHaveBeenCalled()
})
