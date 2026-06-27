/** Image placement within a card. `center` is the default and is stored without
 *  a fragment so the common case stays clean markdown (`![](asset:<hash>)`). */
export type ImageAlign = 'left' | 'center' | 'right'

/** `asset:<hash>` with an optional `#left|#center|#right` placement fragment. */
const ASSET_SRC = /^asset:([0-9a-f]{64})(?:#(left|center|right))?$/

/** Parse an `asset:` src into its hash + placement, or null if not an asset src. */
export function parseAssetSrc(src: string | null | undefined): { hash: string; align: ImageAlign } | null {
  const m = src ? ASSET_SRC.exec(src) : null
  if (!m) return null
  return { hash: m[1], align: (m[2] as ImageAlign) ?? 'center' }
}

/** Build an `asset:` src for a hash + placement. Center omits the fragment. */
export function buildAssetSrc(hash: string, align: ImageAlign): string {
  return align === 'center' ? `asset:${hash}` : `asset:${hash}#${align}`
}

/** Whether a URL is one of our `asset:` srcs (used to let it past url sanitizing). */
export function isAssetSrc(url: string): boolean {
  return parseAssetSrc(url) !== null
}
