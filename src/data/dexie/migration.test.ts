import { describe, it, expect, afterEach } from 'vitest'
import Dexie from 'dexie'
import { RemDB } from './db'

const NAME = 'rem-migration-test'

afterEach(async () => {
  await Dexie.delete(NAME)
})

describe('v2 migration', () => {
  it('stamps schedulerKind/kind onto legacy records', async () => {
    // Write v1-shaped data with a bare v1 Dexie instance.
    const v1 = new Dexie(NAME)
    v1.version(1).stores({ decks: 'id, createdAt', cards: 'id, deckId, createdAt' })
    await v1.open()
    await v1.table('decks').add({ id: 'd1', name: 'Old', createdAt: 1 })
    await v1.table('cards').add({
      id: 'c1', deckId: 'd1', front: 'q', back: 'a', createdAt: 1, updatedAt: 1,
      scheduling: { repetitions: 1, intervalDays: 3, easeFactor: 2.5, due: 9 },
    })
    v1.close()

    // Reopen through RemDB (declares v2) to trigger the upgrade.
    const db = new RemDB(NAME)
    const deck = await db.decks.get('d1')
    const card = await db.cards.get('c1')
    expect(deck?.schedulerKind).toBe('sm2')
    expect(card?.scheduling?.kind).toBe('sm2')
    db.close()
  })
})
