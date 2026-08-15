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
  await expect.element(page.getByRole('complementary')).not.toBeInTheDocument()

  const shell = page.getByTestId('screen').element().querySelector('.app')
  expect(shell).toHaveClass('is-reviewing')
  const progress = page.getByRole('progressbar', { name: 'Review progress' })
  await expect.element(progress).toHaveAttribute('aria-valuetext', 'Card 1 of 1')

  const questionCard = page.getByText('Q?').element().closest<HTMLElement>('.review-card')!
  const showAnswerLocator = page.getByRole('button', { name: 'Show answer', exact: false })
  const showAnswer = showAnswerLocator.element()
  const endSession = page.getByRole('link', { name: 'End session' }).element()
  const questionAlign = getComputedStyle(questionCard.querySelector<HTMLElement>('.review-q')!).textAlign
  expect(showAnswer.getBoundingClientRect().width).toBeLessThan(
    questionCard.getBoundingClientRect().width / 2,
  )
  expect(showAnswer.getBoundingClientRect().height).toBeLessThanOrEqual(
    endSession.getBoundingClientRect().height + 8,
  )
  expect(showAnswer).toHaveClass('btn-primary')
  await showAnswerLocator.click()

  await expect.element(page.getByText('A — the answer.')).toBeVisible()
  const good = page.getByRole('button', { name: 'Good', exact: false })
  await expect.element(good).toBeVisible()
  const answerCard = page.getByText('A — the answer.').element().closest<HTMLElement>('.review-card')!
  expect(getComputedStyle(answerCard.querySelector<HTMLElement>('.review-q')!).textAlign).toBe(questionAlign)

  const hard = page.getByRole('button', { name: 'Hard', exact: false }).element()
  const easy = page.getByRole('button', { name: 'Easy', exact: false }).element()
  const again = page.getByRole('button', { name: 'Again', exact: false }).element()
  const gradeRow = good.element().closest<HTMLElement>('.grade-row')!
  expect(getComputedStyle(gradeRow).borderTopWidth).toBe('0px')
  expect(getComputedStyle(gradeRow).columnGap).toBe('8px')
  expect(getComputedStyle(hard).flexDirection).toBe('row')
  expect(hard.querySelector('.grade-key')).toBeTruthy()
  expect(getComputedStyle(easy).backgroundColor).toBe(getComputedStyle(hard).backgroundColor)
  expect(getComputedStyle(good.element()).backgroundColor)
    .not.toBe(getComputedStyle(hard).backgroundColor)
  expect(
    getComputedStyle(again.querySelector<HTMLElement>('.grade-label')!).color,
  ).not.toBe(getComputedStyle(hard.querySelector<HTMLElement>('.grade-label')!).color)
})

afterEach(async () => {
  vi.restoreAllMocks()
  delete document.documentElement.dataset.theme
  await page.viewport(1280, 800)
})

function relativeLuminance(color: string): number {
  const [red, green, blue] = color.match(/[\d.]+/g)!.slice(0, 3).map(Number)
    .map((channel) => {
      const value = channel / 255
      return value <= 0.04045 ? value / 12.92 : ((value + 0.055) / 1.055) ** 2.4
    })
  return 0.2126 * red + 0.7152 * green + 0.0722 * blue
}

function contrastRatio(foreground: string, background: string): number {
  const lighter = Math.max(relativeLuminance(foreground), relativeLuminance(background))
  const darker = Math.min(relativeLuminance(foreground), relativeLuminance(background))
  return (lighter + 0.05) / (darker + 0.05)
}

test('dark primary review action has readable text contrast', async () => {
  document.documentElement.dataset.theme = 'dark'
  const storage = freshStorage()
  const deck = await storage.createDeck('Contrast')
  await storage.createCard(deck.id, 'Question', 'Answer')
  await renderRoute({
    storage,
    entry: `/decks/${deck.id}/study`,
    path: '/decks/:deckId/study',
    element: <ReviewPage />,
  })

  const showAnswer = page.getByRole('button', { name: 'Show answer', exact: false })
  await expect.element(showAnswer).toBeVisible()
  const style = getComputedStyle(showAnswer.element())

  expect(contrastRatio(style.color, style.backgroundColor)).toBeGreaterThanOrEqual(4.5)
})

test('revealed content uses readable question and answer regions', async () => {
  const storage = freshStorage()
  const deck = await storage.createDeck('TypeScript')
  await storage.createCard(
    deck.id,
    'Why does `never` represent an impossible value?',
    'Because no runtime value can be assigned to the `never` type.',
  )
  await renderRoute({
    storage,
    entry: `/decks/${deck.id}/study`,
    path: '/decks/:deckId/study',
    element: <ReviewPage />,
  })

  await page.getByRole('button', { name: 'Show answer', exact: false }).click()

  const question = page.getByRole('region', { name: 'Question' })
  const answer = page.getByRole('region', { name: 'Answer' })
  await expect.element(question).toBeVisible()
  await expect.element(answer).toBeVisible()

  const answerContent = answer.element().querySelector<HTMLElement>('.review-a')!
  const typography = getComputedStyle(answerContent)
  expect(typography.fontFamily).toBe(getComputedStyle(document.body).fontFamily)
  expect(Number.parseFloat(typography.fontSize)).toBeLessThanOrEqual(24)
  expect(Number.parseFloat(typography.lineHeight) / Number.parseFloat(typography.fontSize))
    .toBeGreaterThanOrEqual(1.5)
})

test('review labels and grading controls remain legible', async () => {
  const storage = freshStorage()
  const deck = await storage.createDeck('Readable controls')
  await storage.createCard(deck.id, 'Question', 'Answer')
  await renderRoute({
    storage,
    entry: `/decks/${deck.id}/study`,
    path: '/decks/:deckId/study',
    element: <ReviewPage />,
  })

  const showAnswer = page.getByRole('button', { name: 'Show answer', exact: false })
  await expect.element(showAnswer).toBeVisible()
  const question = page.getByRole('region', { name: 'Question' }).element()
  const questionLabel = question.querySelector<HTMLElement>('.review-label')!
  expect(Number.parseFloat(getComputedStyle(questionLabel).fontSize)).toBeGreaterThanOrEqual(12)

  await showAnswer.click()
  const good = page.getByRole('button', { name: 'Good', exact: false }).element()
  expect(Number.parseFloat(getComputedStyle(good.querySelector<HTMLElement>('.grade-label')!).fontSize))
    .toBeGreaterThanOrEqual(14)
  expect(Number.parseFloat(getComputedStyle(good.querySelector<HTMLElement>('.grade-hint')!).fontSize))
    .toBeGreaterThanOrEqual(13)
  expect(Number.parseFloat(getComputedStyle(good.querySelector<HTMLElement>('.grade-key')!).fontSize))
    .toBeGreaterThanOrEqual(11)
})

test('progress advances through a multi-card session', async () => {
  const storage = freshStorage()
  const deck = await storage.createDeck('TypeScript')
  for (const front of ['First', 'Second']) {
    const card = await storage.createCard(deck.id, front, 'Answer')
    await storage.updateCard(card.id, {
      scheduling: {
        ...card.scheduling,
        state: 2,
        reps: 3,
        stability: 10,
        difficulty: 5,
        due: 0,
      },
    })
  }
  await renderRoute({
    storage,
    entry: `/decks/${deck.id}/study`,
    path: '/decks/:deckId/study',
    element: <ReviewPage />,
  })

  const progress = page.getByRole('progressbar', { name: 'Review progress' })
  await expect.element(progress).toHaveAttribute('aria-valuetext', 'Card 1 of 2')
  await page.getByRole('button', { name: 'Show answer', exact: false }).click()
  await page.getByRole('button', { name: 'Good', exact: false }).click()
  await expect.element(progress).toHaveAttribute('aria-valuetext', 'Card 2 of 2')
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
  const spy = vi.spyOn(storage, 'commitReview')

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

test('a stale native grade conflict is explained and not counted as a review', async () => {
  const storage = freshStorage()
  const deck = await storage.createDeck('Concurrency')
  const card = await storage.createCard(deck.id, 'Q?', 'A.')
  vi.spyOn(storage, 'gradeStudy').mockResolvedValueOnce({
    status: 'conflict',
    cardId: card.id,
    view: {
      current: null,
      revealed: false,
      nextStates: null,
      reviewed: 0,
      remaining: 0,
      preview: false,
      notice: null,
    },
  })
  const commit = vi.spyOn(storage, 'commitReview')
  await renderRoute({
    storage,
    entry: `/decks/${deck.id}/study`,
    path: '/decks/:deckId/study',
    element: <ReviewPage />,
  })

  await page.getByRole('button', { name: 'Show answer', exact: false }).click()
  await page.getByRole('button', { name: 'Good', exact: false }).click()

  await expect.element(page.getByRole('heading', { name: 'Card changed' })).toBeVisible()
  await expect.element(page.getByRole('alert')).toHaveTextContent(
    'skipped without recording a review',
  )
  expect(commit).not.toHaveBeenCalled()
})

test('a long question remains reachable from its opening through its closing line', async () => {
  await page.viewport(760, 540)
  const storage = freshStorage()
  const deck = await storage.createDeck('Long question')
  const paragraphs = Array.from(
    { length: 12 },
    (_, index) => `Prompt detail ${index + 1}: explain how this condition affects the result and why it matters.`,
  )
  await storage.createCard(
    deck.id,
    ['Opening instruction for the question.', ...paragraphs, 'Final condition for the question.'].join('\n\n'),
    'Answer',
  )
  await renderRoute({
    storage,
    entry: `/decks/${deck.id}/study`,
    path: '/decks/:deckId/study',
    element: <ReviewPage />,
  })

  await expect.element(page.getByRole('button', { name: 'Show answer', exact: false })).toBeVisible()
  const reviewScroll = document.querySelector<HTMLElement>('.review-scroll')!
  const opening = page.getByText('Opening instruction for the question.', { exact: true }).element()
  const closing = page.getByText('Final condition for the question.', { exact: true }).element()
  const scrollBounds = reviewScroll.getBoundingClientRect()

  expect(reviewScroll.scrollHeight).toBeGreaterThan(reviewScroll.clientHeight)
  expect(opening.getBoundingClientRect().top).toBeGreaterThanOrEqual(scrollBounds.top)

  closing.scrollIntoView({ block: 'end' })
  await new Promise((resolve) => requestAnimationFrame(resolve))
  expect(closing.getBoundingClientRect().bottom).toBeLessThanOrEqual(scrollBounds.bottom)
})

test('a large rich answer stays readable while its grading controls remain available', async () => {
  await page.viewport(760, 540)
  document.documentElement.dataset.theme = 'dark'
  const storage = freshStorage()
  const deck = await storage.createDeck('Rich answer')
  const explanation = Array.from(
    { length: 10 },
    (_, index) => `Explanation ${index + 1} connects the example to the principle being recalled.`,
  ).join('\n\n')
  const richBack = `## Readable answer\n\n` +
    `> Readable cards preserve hierarchy even when an answer contains several content types.\n\n` +
    `- Keep prose at a comfortable measure.\n- Keep controls available.\n- Let wide content scroll locally.\n\n` +
    '```ts\nfunction normalize(input: string): string {\n  return input.trim().toLowerCase()\n}\n```\n\n' +
    `[WCAG text spacing guidance](https://www.w3.org/WAI/WCAG22/Understanding/text-spacing.html)\n\n` +
    `| Typography consideration | Reading behavior | Validation approach | Narrow-window result | Theme result | Control behavior |\n` +
    `| --- | --- | --- | --- | --- | --- |\n` +
    `| Comfortable body text | Lines remain easy to follow | Inspect at minimum width | Scrolls locally | Passes in both themes | Dock remains fixed |\n\n` +
    `${explanation}\n\nThe final distinctive closing phrase.`
  await storage.createCard(deck.id, 'How should rich study content behave?', richBack)
  await renderRoute({
    storage,
    entry: `/decks/${deck.id}/study`,
    path: '/decks/:deckId/study',
    element: <ReviewPage />,
  })

  await page.getByRole('button', { name: 'Show answer', exact: false }).click()

  const grades = page.getByRole('group', { name: 'Grade answer' })
  await expect.element(grades).toBeVisible()
  const dockTop = grades.element().getBoundingClientRect().top
  expect(grades.element().getBoundingClientRect().bottom).toBeLessThanOrEqual(window.innerHeight)

  const answer = page.getByRole('region', { name: 'Answer' }).element()
  const reviewScroll = document.querySelector<HTMLElement>('.review-scroll')!
  const quote = answer.querySelector<HTMLElement>('blockquote')!
  const code = answer.querySelector<HTMLElement>('pre')!
  const table = answer.querySelector<HTMLElement>('table')!
  const link = page.getByRole('link', { name: 'WCAG text spacing guidance' }).element()

  expect(Number.parseFloat(getComputedStyle(quote).borderLeftWidth)).toBeGreaterThanOrEqual(2)
  expect(Number.parseFloat(getComputedStyle(code).fontSize)).toBeGreaterThanOrEqual(16)
  expect(contrastRatio(getComputedStyle(link).color, getComputedStyle(document.body).backgroundColor))
    .toBeGreaterThanOrEqual(4.5)
  expect(getComputedStyle(table).overflowX).toBe('auto')
  expect(table.scrollWidth).toBeGreaterThan(table.clientWidth)
  expect(reviewScroll.scrollWidth).toBe(reviewScroll.clientWidth)
  table.scrollLeft = table.scrollWidth
  expect(table.scrollLeft).toBeGreaterThan(0)

  const closingPhrase = page.getByText('The final distinctive closing phrase.')
  closingPhrase.element().scrollIntoView({ block: 'end' })
  await new Promise((resolve) => requestAnimationFrame(resolve))

  expect(closingPhrase.element().getBoundingClientRect().bottom).toBeLessThanOrEqual(dockTop)
  expect(grades.element().getBoundingClientRect().top).toBe(dockTop)
})

test('grading controls stay available while a long plain answer scrolls', async () => {
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

  const grades = page.getByRole('group', { name: 'Grade answer' })
  await expect.element(grades).toBeVisible()
  const dockTop = grades.element().getBoundingClientRect().top
  expect(grades.element().getBoundingClientRect().bottom).toBeLessThanOrEqual(window.innerHeight)

  const closingPhrase = page.getByText('The final distinctive closing phrase.')
  closingPhrase.element().scrollIntoView({ block: 'end' })
  await new Promise((resolve) => requestAnimationFrame(resolve))

  expect(closingPhrase.element().getBoundingClientRect().bottom).toBeLessThanOrEqual(window.innerHeight)
  expect(grades.element().getBoundingClientRect().top).toBe(dockTop)
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
  await expect.element(page.getByRole('heading', { level: 1 })).toHaveTextContent('Review')
  await expect.element(page.getByRole('link', { name: 'Close' })).toBeVisible()

  // Reopening on the same day does not grant allowance for the second new card.
  await page.getByRole('link', { name: 'Back to deck' }).click()
  await page.getByRole('link', { name: 'Reopen study' }).click()
  await expect.element(page.getByRole('heading', { name: 'Nothing due' })).toBeVisible()
  await expect.element(page.getByRole('heading', { level: 1 })).toHaveTextContent('Review')
  await expect.element(page.getByRole('link', { name: 'Close' })).toBeVisible()
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
  expect(await storage.listReviewLogs(deck.id)).toEqual([
    expect.objectContaining({ cardId: c.id, deckId: deck.id, grade: 'good' }),
  ])
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
  const commit = vi.spyOn(storage, 'commitReview')
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
  expect(commit).not.toHaveBeenCalled()
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
  const commitSpy = vi.spyOn(storage, 'commitReview')
  await renderRoute({
    storage,
    entry: `/decks/${deck.id}/study`,
    path: '/decks/:deckId/study',
    element: <ReviewPage />,
  })

  await page.getByRole('button', { name: 'Show answer', exact: false }).click()
  await page.getByRole('button', { name: 'Good', exact: false }).click()

  // The fixed-step grade persists, but carries no optimizer event or daily counter.
  await vi.waitFor(() => expect(commitSpy).toHaveBeenCalled())
  expect(commitSpy).toHaveBeenCalledWith(expect.objectContaining({
    cardId: c.id,
    fsrsGrade: undefined,
    daily: undefined,
  }))
  expect(await storage.listReviewLogs(deck.id)).toEqual([])
})
