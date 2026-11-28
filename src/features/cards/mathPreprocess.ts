/**
 * Normalize Anki-style LaTeX delimiters (`\(...\)` and `\[...\]`) into standard
 * Markdown math delimiters (`$...$` and `$$...$$`) while preserving fenced code
 * blocks and inline code spans untouched.
 */
export function normalizeMathDelimiters(markdown: string): string {
  if (!markdown.includes('\\(') && !markdown.includes('\\[')) {
    return markdown
  }
  const lines = markdown.split('\n')
  const out: string[] = []
  let inFence = false
  let fenceChar = ''

  for (const line of lines) {
    const trimmed = line.trimStart()
    const fenceMatch = trimmed.match(/^(`{3,}|~{3,})/)
    if (fenceMatch) {
      const ch = fenceMatch[1][0]
      if (!inFence) {
        inFence = true
        fenceChar = ch
      } else if (ch === fenceChar) {
        inFence = false
        fenceChar = ''
      }
      out.push(line)
      continue
    }
    if (inFence) {
      out.push(line)
      continue
    }
    out.push(normalizeLineOutsideCodeSpans(line))
  }
  return out.join('\n')
}

function normalizeLineOutsideCodeSpans(line: string): string {
  let out = ''
  let i = 0
  let inCode = false
  let ticks = 0

  while (i < line.length) {
    if (line[i] === '`') {
      let j = i
      while (j < line.length && line[j] === '`') j += 1
      const count = j - i
      if (!inCode) {
        inCode = true
        ticks = count
      } else if (count === ticks) {
        inCode = false
        ticks = 0
      }
      out += line.slice(i, j)
      i = j
      continue
    }

    if (!inCode && line[i] === '\\' && i + 1 < line.length) {
      const next = line[i + 1]
      if (next === '[') {
        const close = line.indexOf('\\]', i + 2)
        if (close !== -1) {
          const inner = line.slice(i + 2, close).trim()
          out += `$$${inner}$$`
          i = close + 2
          continue
        }
      } else if (next === '(') {
        const close = line.indexOf('\\)', i + 2)
        if (close !== -1) {
          const inner = line.slice(i + 2, close).trim()
          out += `$${inner}$`
          i = close + 2
          continue
        }
      }
    }

    out += line[i]
    i += 1
  }
  return out
}
