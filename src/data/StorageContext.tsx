import { createContext, useContext, type ReactNode } from 'react'
import type { Storage } from './Storage'
import { DexieStorage } from './dexie/DexieStorage'
import { RemDB } from './dexie/db'

/** The single app-wide storage instance (IndexedDB via Dexie). */
export const defaultStorage: Storage = new DexieStorage(new RemDB())

const StorageContext = createContext<Storage>(defaultStorage)

export function StorageProvider({
  children,
  storage = defaultStorage,
}: {
  children: ReactNode
  /** Override the storage instance — used by tests to inject seeded data. */
  storage?: Storage
}) {
  return <StorageContext.Provider value={storage}>{children}</StorageContext.Provider>
}

/** Access the app-wide {@link Storage}. */
export function useStorage(): Storage {
  return useContext(StorageContext)
}
