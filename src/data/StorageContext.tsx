import { isTauri } from '@tauri-apps/api/core'
import { createContext, useContext, type ReactNode } from 'react'
import type { Storage } from './Storage'
import { DexieStorage } from './dexie/DexieStorage'
import { RemDB } from './dexie/db'
import { TauriStorage } from './TauriStorage'

/** SQLite in packaged native execution; Dexie only supports browser tests. */
export const defaultStorage: Storage = isTauri()
  ? new TauriStorage()
  : new DexieStorage(new RemDB())

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
