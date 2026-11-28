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

export interface AssetLoadState {
  asset: ResolvedAsset | null
  loading: boolean
}

/** Resolve an asset hash to `{ asset, loading }`, resetting immediately on hash change
 *  and revoking the object URL on unmount or change. */
export function useAssetState(hash: string | undefined): AssetLoadState {
  const storage = useStorage()
  const [state, setState] = useState<AssetLoadState>(() => ({
    asset: null,
    loading: Boolean(hash),
  }))

  useEffect(() => {
    if (!hash) {
      setState({ asset: null, loading: false })
      return
    }
    let revoked = false
    let currentUrl: string | null = null
    setState({ asset: null, loading: true })
    void storage
      .getAsset(hash)
      .then((asset) => {
        if (revoked) return
        if (!asset) {
          setState({ asset: null, loading: false })
          return
        }
        const url = URL.createObjectURL(
          new Blob([asset.bytes as Uint8Array<ArrayBuffer>], { type: asset.mime }),
        )
        currentUrl = url
        setState({ asset: { url, mime: asset.mime }, loading: false })
      })
      .catch(() => {
        if (!revoked) setState({ asset: null, loading: false })
      })
    return () => {
      revoked = true
      if (currentUrl) URL.revokeObjectURL(currentUrl)
    }
  }, [storage, hash])

  return state
}

/** Resolve an asset hash to `{ url, mime }`, revoking the object URL on unmount or hash change. */
export function useAsset(hash: string | undefined): ResolvedAsset | null {
  return useAssetState(hash).asset
}

/** Object URL for an asset hash, revoked on unmount or when the hash changes. */
export function useAssetUrl(hash: string | undefined): string | null {
  return useAsset(hash)?.url ?? null
}
