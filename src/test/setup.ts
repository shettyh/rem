import '@testing-library/jest-dom'
import 'fake-indexeddb/auto'

// Polyfill localStorage for jsdom
declare global {
  var localStorage: Storage
}

if (typeof globalThis !== 'undefined' && !globalThis.localStorage) {
  const store: Record<string, string> = {}
  globalThis.localStorage = {
    getItem: (key: string) => store[key] || null,
    setItem: (key: string, value: string) => {
      store[key] = value
    },
    removeItem: (key: string) => {
      delete store[key]
    },
    clear: () => {
      for (const key in store) {
        delete store[key]
      }
    },
    key: (index: number) => {
      const keys = Object.keys(store)
      return keys[index] || null
    },
    length: 0,
  } as Storage
  Object.defineProperty(globalThis.localStorage, 'length', {
    get: () => Object.keys(store).length,
  })
}
