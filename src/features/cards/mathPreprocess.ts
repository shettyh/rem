/**
 * Split markdown into alternating raw-code (fenced code blocks and inline code spans)
 * and normal-text segments so transforms never touch code blocks or inline code.
 */
export function mapOutsideCode(
  markdown: string,
  transform: (segment: string) => string,
): string {
  const out: string[] = []
  const lines = markdown.split('\n')
  let textBuf: string[] = []
  let inFence = false
  let fenceChar = ''
  let fenceLen = 0

  const flushText = () => {
    if (textBuf.length === 0) return
    out.push(mapOutsideInlineCodeSpans(textBuf.join('\n'), transform))
    textBuf = []
  }

  for (const line of lines) {
    const trimmed = line.trimStart()
    const fenceMatch = trimmed.match(/^(`{3,}|~{3,})/)
    if (fenceMatch) {
      const marker = fenceMatch[1]
      const ch = marker[0]
      if (!inFence) {
        flushText()
        inFence = true
        fenceChar = ch
        fenceLen = marker.length
        out.push(line)
        continue
      } else if (ch === fenceChar && marker.length >= fenceLen && trimmed.slice(marker.length).trim() === '') {
        inFence = false
        fenceChar = ''
        fenceLen = 0
        out.push(line)
        continue
      }
    }
    if (inFence) {
      out.push(line)
    } else {
      textBuf.push(line)
    }
  }
  flushText()
  return out.join('\n')
}

function mapOutsideInlineCodeSpans(
  text: string,
  transform: (segment: string) => string,
): string {
  if (!text.includes('`')) return transform(text)
  let out = ''
  let cursor = 0
  let i = 0
  while (i < text.length) {
    if (text[i] === '`') {
      let j = i
      while (j < text.length && text[j] === '`') j += 1
      const tickCount = j - i
      // Find matching closing run of exact length `tickCount`
      let k = j
      let foundClose = -1
      while (k < text.length) {
        if (text[k] === '`') {
          let m = k
          while (m < text.length && text[m] === '`') m += 1
          if (m - k === tickCount) {
            foundClose = m
            break
          }
          k = m
          continue
        }
        k += 1
      }
      if (foundClose !== -1) {
        if (i > cursor) out += transform(text.slice(cursor, i))
        out += text.slice(i, foundClose)
        i = foundClose
        cursor = foundClose
        continue
      }
      i = j
      continue
    }
    i += 1
  }
  if (cursor < text.length) out += transform(text.slice(cursor))
  return out
}

function isEscaped(text: string, pos: number): boolean {
  let backslashes = 0
  let k = pos - 1
  while (k >= 0 && text[k] === '\\') {
    backslashes += 1
    k -= 1
  }
  return backslashes % 2 === 1
}

function findUnescapedDelimiter(text: string, delim: string, from: number): number {
  let idx = text.indexOf(delim, from)
  while (idx !== -1) {
    if (!isEscaped(text, idx)) return idx
    idx = text.indexOf(delim, idx + 1)
  }
  return -1
}

/**
 * Normalize Anki-style LaTeX delimiters (`\(...\)` and `\[...\]`, single-line or
 * multiline) into `$...$` and `$$...$$`, and escape currency dollar signs (such as
 * `$5 and $10` or `$5-$10`) that fail Pandoc's inline-math flanking rules so
 * `remark-math` leaves currency untouched.
 */
export function normalizeMathDelimiters(markdown: string): string {
  if (!markdown.includes('\\(') && !markdown.includes('\\[') && !markdown.includes('$')) {
    return markdown
  }
  return mapOutsideCode(markdown, (segment) => {
    const withAnkiConverted = convertAnkiDelimitersInText(segment)
    return protectCurrencyDollars(withAnkiConverted)
  })
}

function convertAnkiDelimitersInText(text: string): string {
  if (!text.includes('\\(') && !text.includes('\\[')) return text
  let out = ''
  let i = 0
  while (i < text.length) {
    if (text[i] === '\\' && i + 1 < text.length && !isEscaped(text, i)) {
      const next = text[i + 1]
      if (next === '[') {
        const close = findUnescapedDelimiter(text, '\\]', i + 2)
        if (close !== -1) {
          const rawInner = text.slice(i + 2, close)
          const inner = rawInner.trim()
          if (rawInner.includes('\n')) {
            out += `\n$$\n${inner}\n$$\n`
          } else {
            out += `$$${inner}$$`
          }
          i = close + 2
          continue
        }
      } else if (next === '(') {
        const close = findUnescapedDelimiter(text, '\\)', i + 2)
        if (close !== -1) {
          const inner = text.slice(i + 2, close).trim()
          out += `$${inner}$`
          i = close + 2
          continue
        }
      }
    }
    out += text[i]
    i += 1
  }
  return out
}

function isSpaceChar(ch: string | undefined): boolean {
  return ch === ' ' || ch === '\t' || ch === '\n' || ch === '\r'
}

function isDigitChar(ch: string | undefined): boolean {
  return ch !== undefined && ch >= '0' && ch <= '9'
}

/**
 * Escape single `$` signs that are NOT valid Pandoc inline math delimiters (e.g.,
 * currency `$5 and $10`, `$5-$10`, or lone `$100`) so `remark-math` renders them
 * literally while valid `$x^2$` and `$$...$$` pass through intact.
 */
function protectCurrencyDollars(text: string): string {
  if (!text.includes('$')) return text
  let out = ''
  let i = 0
  const len = text.length

  while (i < len) {
    if (text[i] !== '$' || isEscaped(text, i)) {
      out += text[i]
      i += 1
      continue
    }

    // Preserve display math `$$...$$` untouched.
    if (i + 1 < len && text[i + 1] === '$') {
      const close = findUnescapedDelimiter(text, '$$', i + 2)
      if (close !== -1) {
        out += text.slice(i, close + 2)
        i = close + 2
        continue
      }
      out += '$$'
      i += 2
      continue
    }

    // Single `$`: Pandoc rule — opening `$` cannot be followed by whitespace,
    // and closing `$` cannot be preceded by whitespace or followed by a digit.
    const nextCh = text[i + 1]
    if (!nextCh || isSpaceChar(nextCh)) {
      out += '\\$'
      i += 1
      continue
    }

    let close = -1
    for (let j = i + 1; j < len; j += 1) {
      if (text[j] === '\n' && text[j + 1] === '\n') break
      if (text[j] === '$' && !isEscaped(text, j)) {
        if (text[j + 1] === '$') {
          j += 1
          continue
        }
        const prevCh = text[j - 1]
        const afterCh = text[j + 1]
        if (!isSpaceChar(prevCh) && !isDigitChar(afterCh)) {
          close = j
          break
        }
      }
    }

    if (close !== -1) {
      out += text.slice(i, close + 1)
      i = close + 1
    } else {
      out += '\\$'
      i += 1
    }
  }

  return out
}

const MATH_LOAD_RE =
  /(\$\$[\s\S]+?\$\$|\\\[[\s\S]+?\\\]|\\\([\s\S]+?\\\)|(?<!\\)\$(?!\s)([^$\n]+?)(?<!\s)\$(?!\d))/g
const MATH_SAVE_RE =
  /(\$\$[\s\S]+?\$\$|\\\\\[[\s\S]+?\\\\\]|\\\\\([\s\S]+?\\\\\)|(?<!\\)\$(?!\s)([^$\n]+?)(?<!\s)\$(?!\d))/g

/** Escape LaTeX punctuation inside math spans before loading Markdown into TipTap
 *  so `markdown-it` preserves `\frac`, `\(`, `\[`, `_`, and `*` intact. */
export function protectMathForEditorLoad(markdown: string): string {
  if (!markdown.includes('$') && !markdown.includes('\\(') && !markdown.includes('\\[')) {
    return markdown
  }
  return mapOutsideCode(markdown, (seg) =>
    seg.replace(MATH_LOAD_RE, (m) => m.replace(/([\\_*[\]()~`])/g, '\\$1')),
  )
}

/** Undo `prosemirror-markdown`'s escaping inside math spans when reading Markdown
 *  back out of TipTap so editing cards with LaTeX round-trips cleanly. */
export function unescapeMathFromEditorSave(markdown: string): string {
  if (!markdown.includes('$') && !markdown.includes('\\\\(') && !markdown.includes('\\\\[')) {
    return markdown
  }
  return mapOutsideCode(markdown, (seg) =>
    seg.replace(MATH_SAVE_RE, (m) => m.replace(/\\([\\_*[\]()~`])/g, '$1')),
  )
}
