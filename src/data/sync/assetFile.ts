import type { AssetBlob } from './snapshot'

const MIME_EXT: Record<string, string> = {
  'image/png': 'png',
  'image/jpeg': 'jpg',
  'image/gif': 'gif',
  'image/webp': 'webp',
}
const EXT_MIME: Record<string, string> = Object.fromEntries(
  Object.entries(MIME_EXT).map(([mime, ext]) => [ext, mime]),
)

/** On-disk filename for an asset: `<hash>.<ext>`. Unknown mime → `bin`. */
export function assetFileName(a: AssetBlob): string {
  return `${a.hash}.${MIME_EXT[a.mime] ?? 'bin'}`
}

/** Inverse of {@link assetFileName} + base64 payload → an AssetBlob. */
export function assetFileToBlob(name: string, base64: string): AssetBlob {
  const dot = name.lastIndexOf('.')
  const hash = dot >= 0 ? name.slice(0, dot) : name
  const ext = dot >= 0 ? name.slice(dot + 1) : ''
  return { hash, mime: EXT_MIME[ext] ?? 'application/octet-stream', bytes: base64ToBytes(base64) }
}

export function base64FromBytes(bytes: Uint8Array): string {
  let bin = ''
  for (const b of bytes) bin += String.fromCharCode(b)
  return btoa(bin)
}

export function base64ToBytes(b64: string): Uint8Array {
  const bin = atob(b64)
  const out = new Uint8Array(bin.length)
  for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i)
  return out
}
