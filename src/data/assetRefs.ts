const ASSET_REF = /asset:([0-9a-f]{64})/g

/** Hashes referenced as `asset:<hash>` in markdown (e.g. `![alt](asset:<hash>)`). Deduplicated. */
export function assetRefs(markdown: string): string[] {
  const out = new Set<string>()
  for (const m of markdown.matchAll(ASSET_REF)) out.add(m[1])
  return [...out]
}
