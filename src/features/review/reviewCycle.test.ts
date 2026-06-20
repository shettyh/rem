import { describe, it, expect, beforeEach } from 'vitest'
import Dexie from 'dexie'
import { RemDB } from '../../data/dexie/db'
import { DexieStorage } from '../../data/dexie/DexieStorage'
import { SM2Scheduler, MS_PER_DAY } from '../../domain/scheduler/sm2'

/**
 * Composes storage + scheduler exactly as ReviewPage does, guarding the
 * end-to-end loop the UI depends on.
 */
describe('review cycle', () => {
  const DB = 'rem-review-test'
  const scheduler = new SM2Scheduler()
  let storage: DexieStorage

  beforeEach(async () => {
    await Dexie.delete(DB)
    storage = new DexieStorage(new RemDB(DB), scheduler)
  })

  it('grading "good" clears the card today and brings it back after the interval', async () => {
    const deck = await storage.createDeck('Deck')
    const card = await storage.createCard(deck.id, 'q', 'a')

    const t0 = Date.now()
    expect(await storage.countDue(deck.id, t0)).toBe(1)

    const next = scheduler.next(card.scheduling, 'good', t0)
    await storage.updateCard(card.id, { scheduling: next })

    expect(await storage.countDue(deck.id, t0)).toBe(0)
    expect(await storage.countDue(deck.id, t0 + MS_PER_DAY)).toBe(1)
  })
})
