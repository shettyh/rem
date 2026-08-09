import { useEffect, useRef, useState, type DependencyList } from 'react'
import { useStorage } from './StorageContext'

/** Storage-backed query that re-runs after mutations through the current adapter. */
export function useStorageQuery<T>(
  query: () => T | Promise<T>,
  dependencies: DependencyList,
): T | undefined {
  const storage = useStorage()
  const [value, setValue] = useState<T>()
  const generation = useRef(0)

  useEffect(() => {
    let active = true

    const run = () => {
      const current = ++generation.current
      void Promise.resolve()
        .then(query)
        .then((next) => {
          if (active && generation.current === current) setValue(next)
        })
        .catch((error: unknown) => {
          if (active) console.error('Storage query failed', error)
        })
    }

    run()
    const unsubscribe = storage.subscribe(run)
    return () => {
      active = false
      unsubscribe()
    }
    // The caller owns query dependencies, matching React's effect interfaces.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [storage, ...dependencies])

  return value
}
