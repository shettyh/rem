export interface ClozeSpan {
  start: number
  end: number
  index: number
  answer: string
  hint: string | null
}

/**
 * Parse all valid Anki cloze deletions (`{{cN::answer}}` or `{{cN::answer::hint}}`)
 * in `text`, respecting nested `{...}` pairs such as LaTeX `\frac{1}{2}`.
 */
export function parseClozeSpans(text: string): ClozeSpan[] {
  const spans: ClozeSpan[] = []
  const len = text.length
  let i = 0

  while (i + 6 < len) {
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
        if (Number.isInteger(index) && index >= 1) {
          const parsed = scanClozeBody(text, numEnd + 2)
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
): { answer: string; hint: string | null; closeEnd: number } | null {
  let depth = 0
  let hintSep = -1
  for (let j = bodyStart; j < text.length; j += 1) {
    const ch = text[j]
    if (ch === '{') {
      depth += 1
      continue
    }
    if (ch === '}') {
      if (depth === 0 && j + 1 < text.length && text[j + 1] === '}') {
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
    if (depth === 0 && hintSep === -1 && ch === ':' && j + 1 < text.length && text[j + 1] === ':') {
      hintSep = j
      j += 1
    }
  }
  return null
}

/** Sorted unique cloze indices (`1, 2, ...`) found in `text`. */
export function extractClozeIndices(text: string): number[] {
  const set = new Set<number>()
  for (const span of parseClozeSpans(text)) {
    set.add(span.index)
  }
  return [...set].sort((a, b) => a - b)
}

export function hasClozeDeletions(text: string): boolean {
  return parseClozeSpans(text).length > 0
}

/** Strip `{{cN::answer(::hint)?}}` wrappers so lists/search show clean readable text. */
export function stripClozeMarkup(text: string): string {
  const spans = parseClozeSpans(text)
  if (spans.length === 0) return text
  let out = ''
  let cursor = 0
  for (const span of spans) {
    out += text.slice(cursor, span.start)
    out += span.answer
    cursor = span.end
  }
  out += text.slice(cursor)
  return out
}

export const CLOZE_BLANK_HREF = '#rem-cloze-blank'
export const CLOZE_ANSWER_HREF = '#rem-cloze-answer'

/**
 * Transform cloze markdown into safe Markdown tokens rendered by `MarkdownView`:
 * - Active unrevealed clozes become `[[...]](#rem-cloze-blank)` or `[[hint]](#rem-cloze-blank)`
 * - Active revealed clozes become `[answer](#rem-cloze-answer)` (preserving inline math/formatting)
 * - Inactive clozes render as plain text.
 */
export function renderClozeMarkdown(
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
        // Avoid nesting markdown links or unescaped brackets inside link text; fall back to bold emphasis when needed.
        const unsafeForLinkText = span.answer.includes('[') || span.answer.includes(']')
        if (unsafeForLinkText) {
          out += `**${span.answer}**`
        } else {
          out += `[${span.answer}](${CLOZE_ANSWER_HREF})`
        }
      } else {
        const label = (span.hint ? span.hint : '…').replace(/[[\]]/g, '')
        out += `[${label || '…'}](${CLOZE_BLANK_HREF})`
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
