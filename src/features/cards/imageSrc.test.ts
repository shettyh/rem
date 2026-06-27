import { describe, it, expect } from 'vitest'
import { parseAssetSrc, buildAssetSrc, isAssetSrc } from './imageSrc'

const HASH = 'a'.repeat(64)

describe('imageSrc', () => {
  it('parses a plain asset src as centered', () => {
    expect(parseAssetSrc(`asset:${HASH}`)).toEqual({ hash: HASH, align: 'center' })
  })

  it('parses an aligned asset src', () => {
    expect(parseAssetSrc(`asset:${HASH}#left`)).toEqual({ hash: HASH, align: 'left' })
    expect(parseAssetSrc(`asset:${HASH}#right`)).toEqual({ hash: HASH, align: 'right' })
  })

  it('returns null for non-asset srcs', () => {
    expect(parseAssetSrc('https://example.com/cat.png')).toBeNull()
    expect(parseAssetSrc('')).toBeNull()
    expect(parseAssetSrc(undefined)).toBeNull()
  })

  it('builds clean markdown for the centered default', () => {
    expect(buildAssetSrc(HASH, 'center')).toBe(`asset:${HASH}`)
  })

  it('builds a fragment only for off-center placement', () => {
    expect(buildAssetSrc(HASH, 'left')).toBe(`asset:${HASH}#left`)
    expect(buildAssetSrc(HASH, 'right')).toBe(`asset:${HASH}#right`)
  })

  it('round-trips through parse/build', () => {
    for (const align of ['left', 'center', 'right'] as const) {
      expect(parseAssetSrc(buildAssetSrc(HASH, align))).toEqual({ hash: HASH, align })
    }
  })

  it('recognises asset srcs with and without a fragment', () => {
    expect(isAssetSrc(`asset:${HASH}`)).toBe(true)
    expect(isAssetSrc(`asset:${HASH}#right`)).toBe(true)
    expect(isAssetSrc('http://x/y.png')).toBe(false)
  })
})
