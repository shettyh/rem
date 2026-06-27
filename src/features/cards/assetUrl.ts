import { useEffect, useState } from 'react'
import type { Storage } from '../../data/Storage'
import { useStorage } from '../../data/StorageContext'

/** Resolve an asset hash to an object URL, or null if absent. Caller owns revocation. */
export async function loadAssetUrl(storage: Storage, hash: string): Promise<string | null> {
  const asset = await storage.getAsset(hash)
  if (!asset) return null
  return URL.createObjectURL(new Blob([asset.bytes as Uint8Array<ArrayBuffer>], { type: asset.mime }))
}

/** Object URL for an asset hash, revoked on unmount or when the hash changes. */
export function useAssetUrl(hash: string | undefined): string | null {
  const storage = useStorage()
  const [url, setUrl] = useState<string | null>(null)

  useEffect(() => {
    if (!hash) {
      setUrl(null)
      return
    }
    let revoked = false
    let current: string | null = null
    void loadAssetUrl(storage, hash).then((resolved) => {
      if (revoked) {
        if (resolved) URL.revokeObjectURL(resolved)
        return
      }
      current = resolved
      setUrl(resolved)
    })
    return () => {
      revoked = true
      if (current) URL.revokeObjectURL(current)
    }
  }, [storage, hash])

  return url
}
