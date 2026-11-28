import { extractClozeIndices } from './cloze'

export const OCCLUSION_FENCE_LANG = 'rem-occlusion'
export const MAX_OCCLUSION_MASKS = 200
export const MIN_MASK_EXTENT = 0.002

export type OcclusionMode = 'hide-all' | 'hide-one'
export type OcclusionShapeType = 'rect' | 'ellipse'

export interface OcclusionMask {
  /** Positive 1-based group/card index (multiple shapes with the same id form one group). */
  id: number
  type: OcclusionShapeType
  /** Normalized 0..1 coordinates relative to the image bounds. */
  x: number
  y: number
  w: number
  h: number
  label?: string
}

export interface OcclusionBlock {
  v: 1
  image: string
  mode: OcclusionMode
  alt?: string
  masks: OcclusionMask[]
}

export interface OcclusionBlockSpan {
  start: number
  end: number
  block: OcclusionBlock
}

export type MarkdownOcclusionSegment =
  | { kind: 'markdown'; text: string }
  | { kind: 'occlusion'; block: OcclusionBlock }

const ASSET_REF_RE = /^asset:[0-9a-f]{64}$/

function round4(n: number): number {
  return Math.round(n * 10_000) / 10_000
}

function clamp(v: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, v))
}

export function sanitizeOcclusionMask(raw: unknown): OcclusionMask | null {
  if (!raw || typeof raw !== 'object') return null
  const obj = raw as Record<string, unknown>
  const id = typeof obj.id === 'number' ? obj.id : Number.NaN
  if (!Number.isInteger(id) || id < 1 || id > 4_294_967_295) return null

  const shapeType: OcclusionShapeType = obj.type === 'ellipse' ? 'ellipse' : 'rect'
  const rawX = typeof obj.x === 'number' ? obj.x : Number.NaN
  const rawY = typeof obj.y === 'number' ? obj.y : Number.NaN
  const rawW = typeof obj.w === 'number' ? obj.w : Number.NaN
  const rawH = typeof obj.h === 'number' ? obj.h : Number.NaN

  if (![rawX, rawY, rawW, rawH].every(Number.isFinite)) return null
  if (rawW < MIN_MASK_EXTENT || rawH < MIN_MASK_EXTENT) return null

  const x = clamp(rawX, 0, 1 - MIN_MASK_EXTENT)
  const y = clamp(rawY, 0, 1 - MIN_MASK_EXTENT)
  const w = clamp(rawW, MIN_MASK_EXTENT, Math.max(MIN_MASK_EXTENT, 1 - x))
  const h = clamp(rawH, MIN_MASK_EXTENT, Math.max(MIN_MASK_EXTENT, 1 - y))

  const label = typeof obj.label === 'string' && obj.label.trim() ? obj.label.trim() : undefined

  const mask: OcclusionMask = {
    id,
    type: shapeType,
    x: round4(x),
    y: round4(y),
    w: round4(w),
    h: round4(h),
  }
  if (label) mask.label = label
  return mask
}

export function parseOcclusionJson(raw: string): OcclusionBlock | null {
  let parsed: unknown
  try {
    parsed = JSON.parse(raw.trim())
  } catch {
    return null
  }
  if (!parsed || typeof parsed !== 'object') return null
  const obj = parsed as Record<string, unknown>
  if (obj.v !== 1) return null

  const image = typeof obj.image === 'string' ? obj.image.trim() : ''
  if (!ASSET_REF_RE.test(image)) return null

  const mode: OcclusionMode = obj.mode === 'hide-one' ? 'hide-one' : 'hide-all'
  const alt = typeof obj.alt === 'string' && obj.alt.trim() ? obj.alt.trim() : undefined

  const rawMasks = Array.isArray(obj.masks) ? obj.masks : []
  const masks: OcclusionMask[] = []
  for (const item of rawMasks) {
    if (masks.length >= MAX_OCCLUSION_MASKS) break
    const sanitized = sanitizeOcclusionMask(item)
    if (sanitized) masks.push(sanitized)
  }

  const block: OcclusionBlock = { v: 1, image, mode, masks }
  if (alt) block.alt = alt
  return block
}

export function serializeOcclusionBlock(block: OcclusionBlock): string {
  const masks: OcclusionMask[] = []
  for (const m of block.masks) {
    if (masks.length >= MAX_OCCLUSION_MASKS) break
    const sanitized = sanitizeOcclusionMask(m)
    if (sanitized) masks.push(sanitized)
  }
  const payload: OcclusionBlock = {
    v: 1,
    image: block.image.trim(),
    mode: block.mode === 'hide-one' ? 'hide-one' : 'hide-all',
    ...(block.alt?.trim() ? { alt: block.alt.trim() } : {}),
    masks,
  }
  return `\`\`\`${OCCLUSION_FENCE_LANG}\n${JSON.stringify(payload)}\n\`\`\``
}

function parseFenceOpener(line: string): { ch: string; len: number; info: string } | null {
  let i = 0
  while (i < line.length && i < 3 && line[i] === ' ') i += 1
  const ch = line[i]
  if (ch !== '`' && ch !== '~') return null
  let count = 0
  while (i + count < line.length && line[i + count] === ch) count += 1
  if (count < 3) return null
  const rest = line.slice(i + count).trim()
  if (ch === '`' && rest.includes('`')) return null
  const info = rest.split(/\s+/)[0] ?? ''
  return { ch, len: count, info }
}

function isFenceCloser(line: string, ch: string, minLen: number): boolean {
  let i = 0
  while (i < line.length && i < 3 && line[i] === ' ') i += 1
  let count = 0
  while (i + count < line.length && line[i + count] === ch) count += 1
  if (count < minLen) return false
  return line.slice(i + count).trim().length === 0
}

export function parseOcclusionBlocks(markdown: string): OcclusionBlockSpan[] {
  const spans: OcclusionBlockSpan[] = []
  const len = markdown.length
  let pos = 0

  while (pos < len) {
    const lineStart = pos
    const nl = markdown.indexOf('\n', pos)
    const lineEnd = nl === -1 ? len : nl
    const nextPos = nl === -1 ? len : nl + 1
    const line = markdown.slice(lineStart, lineEnd).replace(/\r$/, '')

    const opener = parseFenceOpener(line)
    if (opener) {
      const bodyStart = nextPos
      let scan = nextPos
      let closeBodyEnd = -1
      let closeBlockEnd = -1

      while (scan < len) {
        const cStart = scan
        const cNl = markdown.indexOf('\n', scan)
        const cEnd = cNl === -1 ? len : cNl
        const cNext = cNl === -1 ? len : cNl + 1
        const cLine = markdown.slice(cStart, cEnd).replace(/\r$/, '')
        if (isFenceCloser(cLine, opener.ch, opener.len)) {
          closeBodyEnd = cStart
          closeBlockEnd = cNext
          break
        }
        scan = cNext
      }

      if (closeBodyEnd !== -1 && closeBlockEnd !== -1) {
        if (opener.info.toLowerCase() === OCCLUSION_FENCE_LANG) {
          const block = parseOcclusionJson(markdown.slice(bodyStart, closeBodyEnd))
          if (block) {
            spans.push({ start: lineStart, end: closeBlockEnd, block })
          }
        }
        pos = closeBlockEnd
        continue
      } else {
        break
      }
    }

    pos = nextPos
  }

  return spans
}

export function extractOcclusionIndices(markdown: string): number[] {
  const set = new Set<number>()
  for (const span of parseOcclusionBlocks(markdown)) {
    for (const mask of span.block.masks) {
      set.add(mask.id)
    }
  }
  return [...set].sort((a, b) => a - b)
}

/** Combined sorted unique positive sibling indices from text cloze and/or image occlusion. */
export function extractSiblingCardIndices(markdown: string): number[] {
  const set = new Set<number>(extractClozeIndices(markdown))
  for (const idx of extractOcclusionIndices(markdown)) {
    set.add(idx)
  }
  return [...set].sort((a, b) => a - b)
}

export function hasImageOcclusion(markdown: string): boolean {
  return parseOcclusionBlocks(markdown).length > 0
}

export function splitMarkdownOcclusion(markdown: string): MarkdownOcclusionSegment[] {
  const spans = parseOcclusionBlocks(markdown)
  if (spans.length === 0) {
    return [{ kind: 'markdown', text: markdown }]
  }
  const segments: MarkdownOcclusionSegment[] = []
  let cursor = 0
  for (const span of spans) {
    if (span.start > cursor) {
      const text = markdown.slice(cursor, span.start)
      if (text.trim()) segments.push({ kind: 'markdown', text })
    }
    segments.push({ kind: 'occlusion', block: span.block })
    cursor = span.end
  }
  if (cursor < markdown.length) {
    const tail = markdown.slice(cursor)
    if (tail.trim()) segments.push({ kind: 'markdown', text: tail })
  }
  return segments
}

export function extractOcclusionDocument(
  markdown: string,
): { header: string; block: OcclusionBlock; footer: string } | null {
  const spans = parseOcclusionBlocks(markdown)
  if (spans.length === 0) return null
  const first = spans[0]
  const header = markdown.slice(0, first.start).trim()
  const footer = markdown.slice(first.end).trim()
  return { header, block: first.block, footer }
}

export function composeOcclusionDocument(
  header: string,
  block: OcclusionBlock,
  footer = '',
): string {
  const parts: string[] = []
  const trimmedHeader = header.trim()
  const trimmedFooter = footer.trim()
  const blockWithAlt: OcclusionBlock = {
    ...block,
    alt: trimmedHeader ? trimmedHeader.split('\n')[0].replace(/^#+\s*/, '').trim() : block.alt,
  }
  if (trimmedHeader) parts.push(trimmedHeader)
  parts.push(serializeOcclusionBlock(blockWithAlt))
  if (trimmedFooter) parts.push(trimmedFooter)
  return parts.join('\n\n')
}

export function stripOcclusionMarkup(markdown: string): string {
  const spans = parseOcclusionBlocks(markdown)
  if (spans.length === 0) return markdown
  let out = ''
  let cursor = 0
  for (const span of spans) {
    out += markdown.slice(cursor, span.start)
    const groups = new Set(span.block.masks.map((m) => m.id)).size
    const label = span.block.alt
      ? `[Image occlusion: ${span.block.alt}]`
      : groups === 1
        ? '[Image occlusion · 1 mask]'
        : groups > 1
          ? `[Image occlusion · ${groups} masks]`
          : '[Image occlusion]'
    out += `${label}\n`
    cursor = span.end
  }
  out += markdown.slice(cursor)
  return out.trim()
}

export function nextOcclusionMaskId(masks: OcclusionMask[]): number {
  let max = 0
  for (const m of masks) {
    if (m.id > max) max = m.id
  }
  return max + 1
}
