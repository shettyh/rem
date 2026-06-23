import { describe, it, expect, beforeEach, afterEach } from 'vitest'
import Dexie from 'dexie'
import { RemDB } from './db'

const DB_NAME = 'rem-tombstones-test'
let db: RemDB

beforeEach(async () => {
  await Dexie.delete(DB_NAME)
  db = new RemDB(DB_NAME)
})
afterEach(() => db.close())

describe('tombstones table', () => {
  it('stores and reads a tombstone by id', async () => {
    await db.tombstones.put({ id: 'card-1', kind: 'card', deletedAt: 42 })
    const t = await db.tombstones.get('card-1')
    expect(t).toEqual({ id: 'card-1', kind: 'card', deletedAt: 42 })
  })
})
