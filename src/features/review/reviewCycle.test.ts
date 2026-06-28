import { describe, it, expect, beforeEach } from 'vitest'
import Dexie from 'dexie'
import { RemDB } from '../../data/dexie/db'
import { DexieStorage } from '../../data/dexie/DexieStorage'
import { getScheduler } from '../../domain/scheduler'

/**
 * Composes storage + scheduler exactly as ReviewPage does, guarding the
 * end-to-end loop the UI depends on. Uses the fake scheduler (non-Tauri).
 */
describe('review cycle', () => {
  const DB = 'rem-review-test'
  let storage: DexieStorage

  beforeEach(async () => {
    await Dexie.delete(DB)
    storage = new DexieStorage(new RemDB(DB))
  })

  it('grading "good" clears the card today and brings it back after the interval', async () => {
    const deck = await storage.createDeck('Deck')
    const card = await storage.createCard(deck.id, 'q', 'a')

    const t0 = Date.now()
    expect(await storage.countDue(deck.id, t0)).toBe(1)

    const nexts = await getScheduler().previewNextStates(card.scheduling, t0)
    await storage.updateCard(card.id, { scheduling: nexts.good })

    expect(await storage.countDue(deck.id, t0)).toBe(0)
    expect(nexts.good.due).toBeGreaterThan(t0)
    expect(await storage.countDue(deck.id, nexts.good.due)).toBe(1)
  })
})
