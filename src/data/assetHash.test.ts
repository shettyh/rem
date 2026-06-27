import { describe, it, expect } from 'vitest'
import { hashBytes } from './assetHash'

describe('hashBytes', () => {
  it('hashes bytes to the known SHA-256 hex of "abc"', async () => {
    const bytes = new TextEncoder().encode('abc')
    expect(await hashBytes(bytes)).toBe(
      'ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad',
    )
  })

  it('is stable for identical bytes', async () => {
    const a = await hashBytes(new Uint8Array([1, 2, 3]))
    const b = await hashBytes(new Uint8Array([1, 2, 3]))
    expect(a).toBe(b)
  })
})
