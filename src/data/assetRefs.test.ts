import { describe, it, expect } from 'vitest'
import { assetRefs } from './assetRefs'

const H = 'a'.repeat(64)
const H2 = 'b'.repeat(64)

describe('assetRefs', () => {
  it('extracts the hash from an image reference', () => {
    expect(assetRefs(`text ![pic](asset:${H}) more`)).toEqual([H])
  })

  it('deduplicates repeated references', () => {
    expect(assetRefs(`![a](asset:${H}) ![b](asset:${H})`)).toEqual([H])
  })

  it('returns distinct hashes and ignores non-asset urls', () => {
    expect(assetRefs(`![a](asset:${H}) ![b](asset:${H2}) [x](https://e.com)`).sort()).toEqual(
      [H, H2].sort(),
    )
  })

  it('returns empty for markdown with no assets', () => {
    expect(assetRefs('plain **markdown**')).toEqual([])
  })
})
