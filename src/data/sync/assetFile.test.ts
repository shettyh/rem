import { describe, it, expect } from 'vitest'
import { assetFileName, assetFileToBlob, base64FromBytes, base64ToBytes } from './assetFile'

const H = 'a'.repeat(64)

describe('assetFile', () => {
  it('names a file <hash>.<ext> from mime', () => {
    expect(assetFileName({ hash: H, mime: 'image/png', bytes: new Uint8Array() })).toBe(`${H}.png`)
    expect(assetFileName({ hash: H, mime: 'image/jpeg', bytes: new Uint8Array() })).toBe(`${H}.jpg`)
  })

  it('round-trips bytes through base64', () => {
    const bytes = new Uint8Array([0, 1, 254, 255, 128])
    expect(base64ToBytes(base64FromBytes(bytes))).toEqual(bytes)
  })

  it('reconstructs a blob from filename + base64', () => {
    const bytes = new Uint8Array([10, 20, 30])
    const blob = assetFileToBlob(`${H}.gif`, base64FromBytes(bytes))
    expect(blob).toEqual({ hash: H, mime: 'image/gif', bytes })
  })
})
