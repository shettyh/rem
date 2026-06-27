import { describe, it, expect, vi, beforeEach } from 'vitest'
import Dexie from 'dexie'
import { RemDB } from '../../data/dexie/db'
import { DexieStorage } from '../../data/dexie/DexieStorage'
import { loadAssetUrl } from './assetUrl'

beforeEach(async () => {
  await Dexie.delete('rem-asseturl')
})

describe('loadAssetUrl', () => {
  it('returns an object URL built from the asset blob', async () => {
    const storage = new DexieStorage(new RemDB('rem-asseturl'))
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
    const storage = new DexieStorage(new RemDB('rem-asseturl'))
    expect(await loadAssetUrl(storage, 'd'.repeat(64))).toBeNull()
  })
})
