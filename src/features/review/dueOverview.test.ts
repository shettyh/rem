import { describe, it, expect } from 'vitest'
import { freshStorage } from '../../test/seed'
import { isNew, loadDueOverview } from './dueOverview'
import { initialFsrsState } from '../../domain/models'

describe('isNew', () => {
  it('is true only for first-time cards', () => {
    expect(isNew(initialFsrsState(0))).toBe(true)
    expect(isNew({ ...initialFsrsState(0), state: 2, reps: 2 })).toBe(false)
  })
})

describe('loadDueOverview', () => {
  it('maps getTodaySummary counts onto per-deck overviews', async () => {
    const storage = freshStorage()
    const a = await storage.createDeck('Alpha')
    const b = await storage.createDeck('Beta')
    const c1 = await storage.createCard(a.id, 'Q1', 'A1')
    await storage.createCard(a.id, 'Q2', 'A2')
    await storage.updateCard(c1.id, { scheduling: { ...c1.scheduling, state: 2, reps: 1, due: 0 } })

    storage.setTodaySummary({
      totalDue: 2,
      newDue: 1,
      reviewDue: 1,
      estimatedMinutes: 1,
      reviewedToday: 4,
      recallTodayPercent: 90,
      currentStreakDays: 3,
      decks: [
        {
          id: a.id,
          name: a.name,
          color: a.color,
          totalCards: 2,
          dueCount: 2,
          newDueCount: 1,
          masteredRatio: 0.5,
          desiredRetention: 0.9,
        },
        {
          id: b.id,
          name: b.name,
          color: b.color,
          totalCards: 0,
          dueCount: 0,
          newDueCount: 0,
          masteredRatio: 0,
          desiredRetention: 0.9,
        },
      ],
    })

    const ov = await loadDueOverview(storage, Date.now())
    expect(ov.totalDue).toBe(2)
    expect(ov.totalNew).toBe(1)
    expect(ov.totalReview).toBe(1)
    expect(ov.reviewedToday).toBe(4)
    expect(ov.decks.find((d) => d.deck.id === a.id)).toMatchObject({
      due: 2,
      newCount: 1,
      total: 2,
      studied: 1,
    })
    expect(ov.decks.find((d) => d.deck.id === b.id)).toMatchObject({
      due: 0,
      newCount: 0,
      total: 0,
      studied: 0,
    })
  })
})
