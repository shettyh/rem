import { test, expect, vi, afterEach } from 'vitest'
import { page } from 'vitest/browser'
import { ReviewPage } from './ReviewPage'
import { freshStorage } from '../../test/seed'
import { renderRoute } from '../../test/renderRoute'
import { MS_PER_DAY } from '../../domain/models'

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
  const storage = freshStorage()
  vi.spyOn(storage, 'revealStudy').mockRejectedValueOnce(new Error('boom'))

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
  const storage = freshStorage()
  vi.spyOn(storage, 'revealStudy').mockRejectedValueOnce(new Error('boom'))

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
  const spy = vi.spyOn(storage, 'gradeStudy')

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
      canUndo: false,
      showTimer: false,
      notice: null,
    },
  })
  await renderRoute({
    storage,
    entry: `/decks/${deck.id}/study`,
    path: '/decks/:deckId/study',
    element: <ReviewPage />,
  })

  await page.getByRole('button', { name: 'Show answer', exact: false }).click()
  await page.getByRole('button', { name: 'Good', exact: false }).click()

  await expect.element(page.getByRole('heading', { name: 'Card changed' })).toBeVisible()
  await expect.element(page.getByRole('alert')).toMatchTextContent(
    'skipped without recording a review',
  )
  expect((await storage.getCard(card.id))?.scheduling.reps).toBe(0)
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

test('custom study passes query params to startStudy and returns to options on completion', async () => {
  const storage = freshStorage()
  const startSpy = vi.spyOn(storage, 'startStudy')
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
  expect(startSpy).toHaveBeenCalledWith({
    deckId: deck.id,
    custom: { mode: 'study-ahead', amount: 1 },
  })
  await page.getByRole('button', { name: 'Show answer', exact: false }).click()
  await page.getByRole('button', { name: 'Good', exact: false }).click()

  await expect.element(page.getByText('Review complete')).toBeVisible()
  await expect.element(page.getByRole('link', { name: 'Back to options' })).toBeVisible()
})

test('preview new reveals cards without grading or persistence', async () => {
  const storage = freshStorage()
  const deck = await storage.createDeck('Preview')
  const card = await storage.createCard(deck.id, 'Preview question', 'Preview answer')
  const update = vi.spyOn(storage, 'updateCard')
  const gradeSpy = vi.spyOn(storage, 'gradeStudy')
  await renderRoute({
    storage,
    entry: `/decks/${deck.id}/study?custom=preview-new&amount=1`,
    path: '/decks/:deckId/study',
    element: <ReviewPage />,
  })

  await page.getByRole('button', { name: 'Show answer', exact: false }).click()
  await expect.element(page.getByText('Preview answer')).toBeVisible()
  await expect.element(page.getByRole('button', { name: 'Next card', exact: false })).toBeVisible()
  await expect.element(page.getByRole('button', { name: 'Good', exact: false })).not.toBeInTheDocument()
  await page.getByRole('button', { name: 'Next card', exact: false }).click()

  await expect.element(page.getByText('Preview complete')).toBeVisible()
  expect(update).not.toHaveBeenCalled()
  expect(gradeSpy).not.toHaveBeenCalled()
  expect((await storage.getCard(card.id))?.scheduling.state).toBe(0)
})

test('renders leech suspended notice from study view', async () => {
  const storage = freshStorage()
  const deck = await storage.createDeck('Leeches')
  await storage.createCard(deck.id, 'Hard question', 'Answer')
  storage.setNextGradeNotice('suspend')
  await renderRoute({
    storage,
    entry: `/decks/${deck.id}/study`,
    path: '/decks/:deckId/study',
    element: <ReviewPage />,
  })

  await page.getByRole('button', { name: 'Show answer', exact: false }).click()
  await page.getByRole('button', { name: 'Again', exact: false }).click()

  await expect.element(page.getByRole('status')).toMatchTextContent('Leech suspended')
  await expect.element(page.getByText('Review complete')).toBeVisible()
})

test('renders leech tagged notice while continuing the session', async () => {
  const storage = freshStorage()
  const deck = await storage.createDeck('Tagged leeches')
  await storage.createCard(deck.id, 'Hard question', 'Answer')
  await storage.createCard(deck.id, 'Next question', 'Next answer')
  storage.setNextGradeNotice('tag')
  await renderRoute({
    storage,
    entry: `/decks/${deck.id}/study`,
    path: '/decks/:deckId/study',
    element: <ReviewPage />,
  })

  await page.getByRole('button', { name: 'Show answer', exact: false }).click()
  await page.getByRole('button', { name: 'Again', exact: false }).click()

  await expect.element(page.getByRole('status')).toMatchTextContent('Leech tagged')
  await expect.element(page.getByRole('button', { name: 'Show answer', exact: false })).toBeVisible()
})

test('undo restores the last graded card (including from completion)', async () => {
  const storage = freshStorage()
  const deck = await storage.createDeck('Undo deck')
  const card = await storage.createCard(deck.id, 'What is RAII?', 'Resource Acquisition Is Initialization.')
  await storage.updateCard(card.id, {
    scheduling: { kind: 'fsrs', stability: 10, difficulty: 5, reps: 3, lapses: 0, state: 2, step: 0, lastReview: 1, due: 0 },
  })
  await renderRoute({
    storage,
    entry: `/decks/${deck.id}/study`,
    path: '/decks/:deckId/study',
    element: <ReviewPage />,
  })

  await expect.element(page.getByRole('button', { name: 'Undo', exact: true })).toBeDisabled()
  await page.getByRole('button', { name: 'Show answer', exact: false }).click()
  await page.getByRole('button', { name: 'Good', exact: false }).click()

  await expect.element(page.getByText('Review complete')).toBeVisible()

  await page.getByRole('button', { name: 'Undo last grade' }).click()
  await expect.element(page.getByText('What is RAII?')).toBeVisible()
  await expect.element(page.getByRole('status')).toMatchTextContent('Last grade undone.')
  expect((await storage.getCard(card.id))?.scheduling.reps).toBe(3)
})

test('suspend removes the current card from review and marks it suspended', async () => {
  const storage = freshStorage()
  const deck = await storage.createDeck('Suspend deck')
  const first = await storage.createCard(deck.id, 'First question', 'First answer')
  const second = await storage.createCard(deck.id, 'Second question', 'Second answer')
  await storage.updateCard(first.id, {
    scheduling: { kind: 'fsrs', stability: 10, difficulty: 5, reps: 1, lapses: 0, state: 2, step: 0, lastReview: 1, due: 0 },
  })
  await storage.updateCard(second.id, {
    scheduling: { kind: 'fsrs', stability: 10, difficulty: 5, reps: 1, lapses: 0, state: 2, step: 0, lastReview: 1, due: 1 },
  })
  await renderRoute({
    storage,
    entry: `/decks/${deck.id}/study`,
    path: '/decks/:deckId/study',
    element: <ReviewPage />,
  })

  await expect.element(page.getByText('First question')).toBeVisible()
  await page.getByRole('button', { name: 'Suspend', exact: true }).click()

  await expect.element(page.getByText('Second question')).toBeVisible()
  await expect.element(page.getByRole('status')).toMatchTextContent('Card suspended.')
  expect((await storage.getCard(first.id))?.suspended).toBe(true)
})

test('accepted draft provenance is inspectable when a card is revealed', async () => {
  const storage = freshStorage()
  const deck = await storage.createDeck('Provenance')
  const proposed = await storage.proposeDrafts(
    deck.id,
    [{
      front: 'Why use HLCs in rem-sync?',
      back: 'For deterministic, clock-skew-tolerant LWW ordering.',
      tags: ['sync'],
      rationale: 'Prevented conflicts between laptop and phone.',
      sources: [{ locator: 'src-tauri/crates/rem-core/src/hlc.rs', label: 'HLC module' }],
    }],
    { proposedBy: 'claude-code' },
  )
  const draft = proposed.outcomes[0].status === 'created' ? proposed.outcomes[0].value : null
  expect(draft).not.toBeNull()
  await storage.resolveDraft(draft!.id, draft!.revision, {
    decision: 'accept',
    deckId: deck.id,
    card: { front: draft!.front, back: draft!.back, tags: draft!.tags },
  })

  await renderRoute({
    storage,
    entry: `/decks/${deck.id}/study`,
    path: '/decks/:deckId/study',
    element: <ReviewPage />,
  })

  await expect.element(page.getByText('Source & rationale')).not.toBeInTheDocument()
  await page.getByRole('button', { name: 'Show answer', exact: false }).click()
  await expect.element(page.getByText('Source & rationale')).toBeVisible()
  await page.getByText('Source & rationale').click()
  await expect.element(page.getByText('Prevented conflicts between laptop and phone.')).toBeVisible()
  await expect.element(page.getByText('src-tauri/crates/rem-core/src/hlc.rs')).toBeVisible()
  await expect.element(page.getByText('Proposed by claude-code')).toBeVisible()
})
