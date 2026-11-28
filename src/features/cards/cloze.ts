import { mapOutsideCode } from './mathPreprocess'

export interface ClozeSpan {
  start: number
  end: number
  index: number
  answer: string
  hint: string | null
}

const MAX_CLOZE_INDEX = 4_294_967_295 // matches Rust u32::MAX

function isEscaped(text: string, pos: number): boolean {
  let backslashes = 0
  let k = pos - 1
  while (k >= 0 && text[k] === '\\') {
    backslashes += 1
    k -= 1
  }
  return backslashes % 2 === 1
}

/**
 * Parse all valid Anki cloze deletions (`{{cN::answer}}` or `{{cN::answer::hint}}`)
 * in `text`, respecting nested `{...}` pairs such as LaTeX `\frac{1}{2}` and
 * escaped braces `\{` / `\}`.
 */
export function parseClozeSpans(text: string): ClozeSpan[] {
  const lastClose = text.lastIndexOf('}}')
  if (lastClose === -1) return []
  const spans: ClozeSpan[] = []
  const len = text.length
  let i = 0

  while (i + 6 < len && i < lastClose) {
    if (
      text[i] === '{'
      && text[i + 1] === '{'
      && (text[i + 2] === 'c' || text[i + 2] === 'C')
    ) {
      let numEnd = i + 3
      while (numEnd < len && text[numEnd] >= '0' && text[numEnd] <= '9') {
        numEnd += 1
      }
      if (
        numEnd > i + 3
        && numEnd + 1 < len
        && text[numEnd] === ':'
        && text[numEnd + 1] === ':'
      ) {
        const index = Number.parseInt(text.slice(i + 3, numEnd), 10)
        if (Number.isInteger(index) && index >= 1 && index <= MAX_CLOZE_INDEX) {
          const parsed = scanClozeBody(text, numEnd + 2, lastClose + 2)
          if (parsed) {
            spans.push({
              start: i,
              end: parsed.closeEnd,
              index,
              answer: parsed.answer,
              hint: parsed.hint,
            })
            i = parsed.closeEnd
            continue
          }
        }
      }
    }
    i += 1
  }

  return spans
}

function scanClozeBody(
  text: string,
  bodyStart: number,
  scanLimit: number,
): { answer: string; hint: string | null; closeEnd: number } | null {
  const limit = Math.min(scanLimit, text.length)
  let depth = 0
  let hintSep = -1
  for (let j = bodyStart; j < limit; j += 1) {
    const ch = text[j]
    if (ch === '{' && !isEscaped(text, j)) {
      depth += 1
      continue
    }
    if (ch === '}' && !isEscaped(text, j)) {
      if (depth === 0 && j + 1 < limit && text[j + 1] === '}') {
        if (hintSep !== -1) {
          const rawHint = text.slice(hintSep + 2, j).trim()
          return {
            answer: text.slice(bodyStart, hintSep),
            hint: rawHint || null,
            closeEnd: j + 2,
          }
        }
        return {
          answer: text.slice(bodyStart, j),
          hint: null,
          closeEnd: j + 2,
        }
      }
      depth = Math.max(0, depth - 1)
      continue
    }
    if (depth === 0 && hintSep === -1 && ch === ':' && j + 1 < limit && text[j + 1] === ':') {
      hintSep = j
      j += 1
    }
  }
  return null
}

function collectClozeSpansOutsideCode(text: string): ClozeSpan[] {
  if (!text.includes('{{')) return []
  const spans: ClozeSpan[] = []
  mapOutsideCode(text, (segment) => {
    spans.push(...parseClozeSpans(segment))
    return segment
  })
  return spans
}

/** Sorted unique cloze indices (`1, 2, ...`) found outside code fences and inline code spans. */
export function extractClozeIndices(text: string): number[] {
  const set = new Set<number>()
  for (const span of collectClozeSpansOutsideCode(text)) {
    set.add(span.index)
  }
  return [...set].sort((a, b) => a - b)
}

export function hasClozeDeletions(text: string): boolean {
  return extractClozeIndices(text).length > 0
}

/** Strip `{{cN::answer(::hint)?}}` wrappers outside code spans so lists/search show clean readable text. */
export function stripClozeMarkup(text: string): string {
  if (!text.includes('{{')) return text
  return mapOutsideCode(text, (segment) => {
    const spans = parseClozeSpans(segment)
    if (spans.length === 0) return segment
    let out = ''
    let cursor = 0
    for (const span of spans) {
      out += segment.slice(cursor, span.start)
      out += span.answer
      cursor = span.end
    }
    out += segment.slice(cursor)
    return out
  })
}

export const CLOZE_BLANK_HREF = '#rem-cloze-blank'
export const CLOZE_ANSWER_HREF = '#rem-cloze-answer'

function isUnsafeForMarkdownLinkLabel(content: string): boolean {
  return (
    content.includes('[')
    || content.includes(']')
    || content.includes('\n')
    || content.endsWith('\\')
  )
}

/**
 * Transform cloze markdown outside code fences and inline code spans into safe Markdown
 * tokens rendered by `MarkdownView`:
 * - Active unrevealed clozes become `[…](#rem-cloze-blank)` or `[hint](#rem-cloze-blank)`
 * - Active revealed clozes become `[answer](#rem-cloze-answer)` (preserving inline math/formatting)
 * - Inactive clozes render as plain text.
 */
export function renderClozeMarkdown(
  text: string,
  activeIndex: number,
  revealed: boolean,
): string {
  if (!text.includes('{{')) return text
  return mapOutsideCode(text, (segment) => renderClozeSegment(segment, activeIndex, revealed))
}

function renderClozeSegment(
  text: string,
  activeIndex: number,
  revealed: boolean,
): string {
  const spans = parseClozeSpans(text)
  if (spans.length === 0) return text
  let out = ''
  let cursor = 0
  for (const span of spans) {
    out += text.slice(cursor, span.start)
    if (span.index === activeIndex) {
      if (revealed) {
        const trimmed = span.answer.trim()
        if (!trimmed) {
          // Empty cloze (e.g. Anki Cloze Overlapping placeholder) reveals to empty text rather than a stuck blank.
        } else if (isUnsafeForMarkdownLinkLabel(span.answer)) {
          out += `**${trimmed}**`
        } else {
          out += `[${span.answer}](${CLOZE_ANSWER_HREF})`
        }
      } else {
        const rawHint = span.hint ?? '…'
        const safeLabel = rawHint
          .replace(/\\/g, '\\\\')
          .replace(/\[/g, '&#91;')
          .replace(/\]/g, '&#93;')
          .replace(/\r?\n/g, ' ')
          .trim()
        out += `[${safeLabel || '…'}](${CLOZE_BLANK_HREF})`
      }
    } else {
      out += span.answer
    }
    cursor = span.end
  }
  out += text.slice(cursor)
  return out
}

/** Next cloze index to insert in the editor (`max + 1`, or `1` when none exist). */
export function nextClozeIndex(text: string): number {
  const indices = extractClozeIndices(text)
  return indices.length > 0 ? indices[indices.length - 1] + 1 : 1
}
