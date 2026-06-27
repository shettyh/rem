import '@testing-library/jest-dom'
import 'fake-indexeddb/auto'

// jsdom's structuredClone returns TypedArrays from a different VM realm, causing
// `instanceof Uint8Array` checks to fail in the test scope. Patch structuredClone
// to wrap TypedArray results in realm-native constructors so toEqual() works.
;(function patchStructuredCloneForJsdom() {
  const _orig = globalThis.structuredClone
  if (!_orig) return

  const NativeUint8Array = Uint8Array

  function fixTypedArrays(obj: unknown): unknown {
    if (obj === null || obj === undefined || typeof obj !== 'object') return obj
    if (ArrayBuffer.isView(obj) && !(obj instanceof DataView)) {
      const v = obj as { buffer: ArrayBuffer; byteOffset: number; byteLength: number; constructor: { name: string } }
      if (v.constructor.name === 'Uint8Array' && !(obj instanceof NativeUint8Array)) {
        return new NativeUint8Array(v.buffer, v.byteOffset, v.byteLength)
      }
      return obj
    }
    for (const key of Object.keys(obj as object)) {
      const rec = obj as Record<string, unknown>
      const patched = fixTypedArrays(rec[key])
      if (patched !== rec[key]) rec[key] = patched
    }
    return obj
  }

  globalThis.structuredClone = function <T>(value: T, options?: StructuredSerializeOptions): T {
    return fixTypedArrays(_orig(value, options)) as T
  }
})()

// jsdom in this Vitest setup does not provide localStorage; install a minimal,
// spec-correct in-memory implementation for unit tests that use it.
if (typeof globalThis.localStorage === 'undefined') {
  const store = new Map<string, string>()
  const localStorageMock: Storage = {
    getItem: (key) => (store.has(key) ? store.get(key)! : null),
    setItem: (key, value) => {
      store.set(key, String(value))
    },
    removeItem: (key) => {
      store.delete(key)
    },
    clear: () => {
      store.clear()
    },
    key: (index) => [...store.keys()][index] ?? null,
    get length() {
      return store.size
    },
  }
  Object.defineProperty(globalThis, 'localStorage', {
    value: localStorageMock,
    writable: true,
    configurable: true,
  })
}
