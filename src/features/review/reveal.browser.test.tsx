import { test, expect } from 'vitest'
import { page } from 'vitest/browser'
import { ReviewPage } from './ReviewPage'
import { freshStorage } from '../../test/seed'
import { renderRoute } from '../../test/renderRoute'

test('show-answer button does not overlap tall front content', async () => {
  const storage = freshStorage()
  const deck = await storage.createDeck('Long')
  const longFront =
    '# Heading\n\n' +
    Array.from({ length: 40 }, (_, i) => `Paragraph ${i} with some content to make the front tall.`).join('\n\n')
  await storage.createCard(deck.id, longFront, 'answer')
  await renderRoute({
    storage,
    entry: `/decks/${deck.id}/study`,
    path: '/decks/:deckId/study',
    element: <ReviewPage />,
  })

  await expect.element(page.getByTestId('flip')).toBeVisible()
  const showBtn = page.getByRole('button', { name: 'Show answer', exact: false }).element()
  const faceInner = document.querySelector('.face-front .face-inner') as HTMLElement

  // The content is taller than its box (so without clipping it would spill
  // onto the button)...
  expect(faceInner.scrollHeight).toBeGreaterThan(faceInner.clientHeight)
  // ...so the front pane must clip/scroll its overflow rather than letting the
  // content render on top of the Show-answer button.
  expect(getComputedStyle(faceInner).overflowY).not.toBe('visible')
  // And its box sits at/above the button.
  expect(faceInner.getBoundingClientRect().bottom).toBeLessThanOrEqual(
    showBtn.getBoundingClientRect().top + 1,
  )
})

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
