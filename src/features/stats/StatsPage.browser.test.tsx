import { expect, test } from 'vitest'
import { page } from 'vitest/browser'
import type { CollectionStats } from '../../domain/models'
import { freshStorage } from '../../test/seed'
import { renderRoute } from '../../test/renderRoute'
import { StatsPage } from './StatsPage'

function daysAgo(days: number): number {
  const date = new Date()
  date.setDate(date.getDate() - days)
  date.setHours(0, 0, 0, 0)
  return date.getTime()
}

function makeDaily(days: number, todayCount: number, yesterdayCount: number) {
  return Array.from({ length: days }, (_, idx) => {
    const offset = days - 1 - idx
    const ts = daysAgo(offset)
    const count = offset === 0 ? todayCount : offset === 1 ? yesterdayCount : 0
    return { day: `day-${offset}`, timestampMs: ts, count }
  })
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

  storage.setStats((deckId, days): CollectionStats => {
    if (deckId === alpha.id) {
      return {
        historyCount: 2,
        totalReviews: 2,
        recallRate: 0.5,
        currentStreakDays: 2,
        activeDays: 2,
        grades: { again: 1, hard: 0, good: 1, easy: 0 },
        daily: makeDaily(days, 1, 1),
        byDeck: [{ deckId: alpha.id, deckName: alpha.name, deckColor: alpha.color, reviews: 2, recallRate: 0.5 }],
        upcoming7d: [],
      }
    }
    return {
      historyCount: 3,
      totalReviews: 3,
      recallRate: 2 / 3,
      currentStreakDays: 2,
      activeDays: 2,
      grades: { again: 1, hard: 0, good: 1, easy: 1 },
      daily: makeDaily(days, 2, 1),
      byDeck: [
        { deckId: alpha.id, deckName: alpha.name, deckColor: alpha.color, reviews: 2, recallRate: 0.5 },
        { deckId: beta.id, deckName: beta.name, deckColor: beta.color, reviews: 1, recallRate: 1 },
      ],
      upcoming7d: [],
    }
  })

  await renderRoute({ storage, entry: '/stats', path: '/stats', element: <StatsPage /> })

  await expect.element(page.getByLabelText('FSRS reviews')).toMatchTextContent('3')
  await expect.element(page.getByLabelText('Recall rate')).toMatchTextContent('67%')
  await expect.element(page.getByLabelText('Current streak')).toMatchTextContent('2 days')
  await expect.element(page.getByLabelText('Active days')).toMatchTextContent('2')

  const reviewsKpi = page.getByLabelText('FSRS reviews').element()
  const kpiValue = reviewsKpi.querySelector<HTMLElement>('strong')!
  const kpiLabel = reviewsKpi.querySelector<HTMLElement>('span')!
  const activityTotal = document.querySelector<HTMLElement>('.stats-head-readout > strong')!
  expect(getComputedStyle(kpiValue).fontFamily).toBe(getComputedStyle(document.body).fontFamily)
  expect(getComputedStyle(kpiValue).fontWeight).toBe('600')
  expect(getComputedStyle(kpiLabel).fontFamily).toBe(getComputedStyle(document.body).fontFamily)
  expect(getComputedStyle(activityTotal).fontWeight).toBe('600')

  await expect
    .element(page.getByLabelText('Daily review activity', { exact: false }))
    .toHaveAttribute('aria-label', 'Daily review activity: 3 reviews over 30 days')
  await expect.element(page.getByLabelText('Again grade')).toMatchTextContent('1')
  await expect.element(page.getByLabelText('Alpha deck stats')).toBeVisible()
  await expect.element(page.getByLabelText('Beta deck stats')).toBeVisible()
  expect(document.querySelector('.stats-deck-dot')).toBeNull()

  await page.getByLabelText('Deck filter').selectOptions(alpha.id)
  await expect.element(page.getByLabelText('FSRS reviews')).toMatchTextContent('2')
  await expect.element(page.getByLabelText('Recall rate')).toMatchTextContent('50%')
  await expect.element(page.getByLabelText('Beta deck stats')).not.toBeInTheDocument()

  await page.getByRole('button', { name: '7D' }).click()
  await expect.element(page.getByRole('button', { name: '7D' })).toHaveAttribute('aria-pressed', 'true')
})
