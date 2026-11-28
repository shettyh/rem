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

  it('handles escaped braces, code-span cloze skipping, trailing backslashes, multiline math, and currency', () => {
    const withSet = 'Set {{c1::\\left\\{ x \\right\\}}} and {{c2::end}}'
    const spans = parseClozeSpans(withSet)
    expect(spans).toHaveLength(2)
    expect(spans[0].answer).toBe('\\left\\{ x \\right\\}')

    // Code spans are not rewritten into cloze links
    const codeCloze = renderClozeMarkdown('Use `{{c1::async}}` and {{c1::await}}', 1, false)
    expect(codeCloze).toContain('`{{c1::async}}`')
    expect(codeCloze).toContain(`[…](${CLOZE_BLANK_HREF})`)

    // Trailing backslash falls back safely without breaking CommonMark link syntax
    const trailingBs = renderClozeMarkdown('Path {{c1::C:\\}}', 1, true)
    expect(trailingBs).not.toContain('](#rem-cloze-answer)')

    // Currency dollars are escaped while real math and multiline \[...\] stay intact
    expect(normalizeMathDelimiters('Costs $5 and $10 today')).toBe('Costs \\$5 and \\$10 today')
    expect(normalizeMathDelimiters('Range $5-$10')).toBe('Range \\$5-\\$10')
    expect(normalizeMathDelimiters('\\[\na + b\n\\]')).toContain('$$\na + b\n$$')
  })
})
