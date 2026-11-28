import { describe, expect, it } from 'vitest'
import {
  CLOZE_ANSWER_HREF,
  CLOZE_BLANK_HREF,
  extractClozeIndices,
  nextClozeIndex,
  parseClozeSpans,
  renderClozeMarkdown,
  stripClozeMarkup,
} from './cloze'
import { normalizeMathDelimiters } from './mathPreprocess'

describe('cloze', () => {
  it('parses hints and nested LaTeX braces accurately', () => {
    const src = 'Derivative is {{c1::$\\frac{1}{2}$::half}} and {{c2::constant}}.'
    const spans = parseClozeSpans(src)
    expect(spans).toHaveLength(2)
    expect(spans[0].answer).toBe('$\\frac{1}{2}$')
    expect(spans[0].hint).toBe('half')
    expect(extractClozeIndices(src)).toEqual([1, 2])
    expect(nextClozeIndex(src)).toBe(3)
    expect(stripClozeMarkup(src)).toBe('Derivative is $\\frac{1}{2}$ and constant.')

    const q1 = renderClozeMarkdown(src, 1, false)
    expect(q1).toContain(CLOZE_BLANK_HREF)
    expect(q1).toContain('half')
    expect(q1).toContain('constant')

    const a1 = renderClozeMarkdown(src, 1, true)
    expect(a1).toContain(CLOZE_ANSWER_HREF)
    expect(a1).toContain('$\\frac{1}{2}$')
  })

  it('normalizes Anki LaTeX delimiters outside code blocks and inline code', () => {
    const md = [
      'Inline \\(x^2 + y^2\\) and block \\[E = mc^2\\].',
      'Code `\\(not math\\)` stays literal.',
      '```tex',
      '\\(also literal\\)',
      '```',
    ].join('\n')
    const out = normalizeMathDelimiters(md)
    expect(out).toContain('Inline $x^2 + y^2$ and block $$E = mc^2$$.')
    expect(out).toContain('Code `\\(not math\\)` stays literal.')
    expect(out).toContain('\\(also literal\\)')
  })
})
