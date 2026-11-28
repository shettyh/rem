import { createContext, useContext, type ReactNode } from 'react'
import type { Storage } from './Storage'
import { TauriStorage } from './TauriStorage'

/** Native SQLite storage over Tauri IPC; tests inject a stub via StorageProvider. */
export const defaultStorage: Storage = new TauriStorage()

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
