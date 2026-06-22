import { DexieStorage } from '../data/dexie/DexieStorage'
import { RemDB } from '../data/dexie/db'
import { MS_PER_DAY } from '../domain/scheduler'
import type { Storage } from '../data/Storage'

let counter = 0

/**
 * A fresh, isolated IndexedDB-backed Storage for a single test. Each call uses a
 * unique database name so tests never share state.
 */
export function freshStorage(): Storage {
  counter += 1
  const name = `rem-test-${Date.now()}-${counter}`
  return new DexieStorage(new RemDB(name))
}

export { MS_PER_DAY }
