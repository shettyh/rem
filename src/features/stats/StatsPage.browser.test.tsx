import { expect, test } from 'vitest'
import { page } from 'vitest/browser'
import { freshStorage } from '../../test/seed'
import { renderRoute } from '../../test/renderRoute'
import { StatsPage } from './StatsPage'

function daysAgo(days: number): number {
  const date = new Date()
  date.setDate(date.getDate() - days)
  date.setHours(10, 0, 0, 0)
  return date.getTime()
}

test('shows an honest empty state before review history exists', async () => {
  const storage = freshStorage()
  await storage.createDeck('TypeScript')
  await renderRoute({ storage, entry: '/stats', path: '/stats', element: <StatsPage /> })

  await expect.element(page.getByRole('heading', { name: 'No review history yet' })).toBeVisible()
  await expect.element(page.getByText('fixed learning-step clicks', { exact: false })).toBeVisible()
  await expect.element(page.getByLabelText('Deck filter')).toBeVisible()
  await expect.element(page.getByRole('link', { name: 'Stats' })).toHaveAttribute('aria-current', 'page')
})

test('renders review metrics and responds to deck and range filters', async () => {
  const storage = freshStorage()
  const alpha = await storage.createDeck('Alpha')
  const beta = await storage.createDeck('Beta')
  const a = await storage.createCard(alpha.id, 'a', 'a')
  const b = await storage.createCard(beta.id, 'b', 'b')

  await storage.commitReview({ cardId: a.id, deckId: alpha.id, patch: {}, reviewedAt: daysAgo(0), fsrsGrade: 'good' })
  await storage.commitReview({ cardId: a.id, deckId: alpha.id, patch: {}, reviewedAt: daysAgo(1), fsrsGrade: 'again' })
  await storage.commitReview({ cardId: b.id, deckId: beta.id, patch: {}, reviewedAt: daysAgo(0) + 1, fsrsGrade: 'easy' })

  await renderRoute({ storage, entry: '/stats', path: '/stats', element: <StatsPage /> })

  await expect.element(page.getByLabelText('FSRS reviews')).toHaveTextContent('3')
  await expect.element(page.getByLabelText('Recall rate')).toHaveTextContent('67%')
  await expect.element(page.getByLabelText('Current streak')).toHaveTextContent('2 days')
  await expect.element(page.getByLabelText('Active days')).toHaveTextContent('2')
  await expect.element(page.getByLabelText('Daily review activity')).toHaveAttribute(
    'aria-label',
    'Daily review activity: 3 reviews over 30 days',
  )
  await expect.element(page.getByLabelText('Again grade')).toHaveTextContent('1')
  await expect.element(page.getByLabelText('Alpha deck stats')).toBeVisible()
  await expect.element(page.getByLabelText('Beta deck stats')).toBeVisible()

  await page.getByLabelText('Deck filter').selectOptions(alpha.id)
  await expect.element(page.getByLabelText('FSRS reviews')).toHaveTextContent('2')
  await expect.element(page.getByLabelText('Recall rate')).toHaveTextContent('50%')
  await expect.element(page.getByLabelText('Beta deck stats')).not.toBeInTheDocument()

  await page.getByRole('button', { name: '7D' }).click()
  await expect.element(page.getByRole('button', { name: '7D' })).toHaveAttribute('aria-pressed', 'true')
})
