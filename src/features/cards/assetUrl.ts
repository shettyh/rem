import { useEffect, useState } from 'react'
import type { Storage } from '../../data/Storage'
import { useStorage } from '../../data/StorageContext'

export interface ResolvedAsset {
  url: string
  mime: string
}

/** Resolve an asset hash to an object URL, or null if absent. Caller owns revocation. */
export async function loadAssetUrl(storage: Storage, hash: string): Promise<string | null> {
  const asset = await storage.getAsset(hash)
  if (!asset) return null
  return URL.createObjectURL(new Blob([asset.bytes as Uint8Array<ArrayBuffer>], { type: asset.mime }))
}

/** Resolve an asset hash to `{ url, mime }`, revoking the object URL on unmount or hash change. */
export function useAsset(hash: string | undefined): ResolvedAsset | null {
  const storage = useStorage()
  const [resolved, setResolved] = useState<ResolvedAsset | null>(null)

  useEffect(() => {
    if (!hash) {
      setResolved(null)
      return
    }
    let revoked = false
    let currentUrl: string | null = null
    void storage.getAsset(hash).then((asset) => {
      if (revoked) return
      if (!asset) {
        setResolved(null)
        return
      }
      const url = URL.createObjectURL(
        new Blob([asset.bytes as Uint8Array<ArrayBuffer>], { type: asset.mime }),
      )
      currentUrl = url
      setResolved({ url, mime: asset.mime })
    })
    return () => {
      revoked = true
      if (currentUrl) URL.revokeObjectURL(currentUrl)
    }
  }, [storage, hash])

  return resolved
}

/** Object URL for an asset hash, revoked on unmount or when the hash changes. */
export function useAssetUrl(hash: string | undefined): string | null {
  return useAsset(hash)?.url ?? null
}
