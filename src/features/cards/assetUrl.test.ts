import { describe, it, expect, vi } from 'vitest'
import { freshStorage } from '../../test/seed'
import { loadAssetUrl } from './assetUrl'

describe('loadAssetUrl', () => {
  it('returns an object URL built from the asset blob', async () => {
    const storage = freshStorage()
    const asset = await storage.putAsset(new Uint8Array([1, 2, 3]), 'image/png')
    const createObjectURL = vi.fn(() => 'blob:fake')
    vi.stubGlobal('URL', { ...URL, createObjectURL })

    const url = await loadAssetUrl(storage, asset.hash)

    expect(url).toBe('blob:fake')
    const blobArg = (createObjectURL.mock.calls as unknown as [[Blob]])[0][0]
    expect(blobArg.type).toBe('image/png')
    expect(blobArg.size).toBe(3)
    vi.unstubAllGlobals()
  })

  it('returns null for a missing asset', async () => {
    const storage = freshStorage()
    expect(await loadAssetUrl(storage, 'd'.repeat(64))).toBeNull()
  })
})
