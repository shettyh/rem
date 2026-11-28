import { mapOutsideCode } from './mathPreprocess'

export interface RubyToken {
  kind: 'text' | 'ruby'
  text: string
  reading?: string
}

/**
 * Split a plain text run into alternating text and `{base|reading}` ruby tokens.
 * Rejects empty base/reading, nested braces/pipes, and whitespace-only segments.
 */
export function splitRubyTokens(text: string): RubyToken[] {
  if (!text.includes('{') || !text.includes('|')) {
    return [{ kind: 'text', text }]
  }

  const tokens: RubyToken[] = []
  let plain = ''
  let i = 0
  const len = text.length

  const flushPlain = () => {
    if (plain.length > 0) {
      tokens.push({ kind: 'text', text: plain })
      plain = ''
    }
  }

  while (i < len) {
    if (text[i] === '{') {
      const close = text.indexOf('}', i + 1)
      if (close !== -1) {
        const inner = text.slice(i + 1, close)
        if (!inner.includes('{') && !inner.includes('\n')) {
          const pipe = inner.indexOf('|')
          if (pipe !== -1 && inner.indexOf('|', pipe + 1) === -1) {
            const base = inner.slice(0, pipe).trim()
            const reading = inner.slice(pipe + 1).trim()
            if (base.length > 0 && reading.length > 0) {
              flushPlain()
              tokens.push({ kind: 'ruby', text: base, reading })
              i = close + 1
              continue
            }
          }
        }
      }
    }
    plain += text[i]
    i += 1
  }

  flushPlain()
  return tokens
}

/** Strip `{base|reading}` ruby syntax outside code spans into plain `base` for previews/snippets. */
export function stripRubyMarkup(markdown: string): string {
  if (!markdown.includes('{') || !markdown.includes('|')) return markdown
  return mapOutsideCode(markdown, (segment) =>
    splitRubyTokens(segment)
      .map((tok) => tok.text)
      .join(''),
  )
}

interface MdastNode {
  type: string
  value?: string
  data?: Record<string, unknown>
  children?: MdastNode[]
}

const SKIP_NODE_TYPES = new Set(['code', 'inlineCode', 'math', 'inlineMath', 'html'])

function transformChildren(node: MdastNode): void {
  if (!node.children || SKIP_NODE_TYPES.has(node.type)) return
  const next: MdastNode[] = []
  let changed = false

  for (const child of node.children) {
    if (child.type === 'text' && typeof child.value === 'string' && child.value.includes('{') && child.value.includes('|')) {
      const tokens = splitRubyTokens(child.value)
      if (tokens.some((t) => t.kind === 'ruby')) {
        changed = true
        for (const tok of tokens) {
          if (tok.kind === 'text') {
            next.push({ type: 'text', value: tok.text })
          } else {
            next.push({
              type: 'ruby',
              data: { hName: 'ruby' },
              children: [
                { type: 'text', value: tok.text },
                { type: 'rubyParen', data: { hName: 'rp' }, children: [{ type: 'text', value: '(' }] },
                { type: 'rubyText', data: { hName: 'rt' }, children: [{ type: 'text', value: tok.reading ?? '' }] },
                { type: 'rubyParen', data: { hName: 'rp' }, children: [{ type: 'text', value: ')' }] },
              ],
            })
          }
        }
        continue
      }
    }
    transformChildren(child)
    next.push(child)
  }

  if (changed) {
    node.children = next
  }
}

/**
 * Remark plugin that transforms `{base|reading}` in non-code/non-math text nodes
 * into semantic `<ruby>base<rp>(</rp><rt>reading</rt><rp>)</rp></ruby>` elements
 * without requiring `rehype-raw`.
 */
export function remarkRuby() {
  return (tree: MdastNode) => {
    transformChildren(tree)
  }
}
