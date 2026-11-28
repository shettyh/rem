import { MS_PER_DAY } from '../domain/models'
import { MemoryStorage } from './memoryStorage'

/**
 * A fresh, isolated in-memory Storage stub for a single test.
 */
export function freshStorage(): MemoryStorage {
  return new MemoryStorage()
}

export { MemoryStorage, MS_PER_DAY }
