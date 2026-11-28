import { describe, expect, it } from 'vitest'
import { forAll, pick, randBool, randInt, randString } from '../../test/prop'
import {
  CLOZE_BLANK_HREF,
  extractClozeIndices,
  hasClozeDeletions,
  parseClozeSpans,
  renderClozeMarkdown,
  stripClozeMarkup,
} from './cloze'
import { buildAssetSrc, isAssetSrc, parseAssetSrc, type ImageAlign } from './imageSrc'
import {
  normalizeMathDelimiters,
  protectMathForEditorLoad,
  unescapeMathFromEditorSave,
} from './mathPreprocess'

const HEX = '0123456789abcdef'
const ALIGNS: readonly ImageAlign[] = ['left', 'center', 'right']

describe('cloze, mathPreprocess & imageSrc property tests', () => {
  it('extractClozeIndices and stripClozeMarkup ignore clozes inside inline code and fenced code blocks', () => {
    const inlineOnly = 'Use `{{c1::answer}}` syntax in Anki.'
    expect(extractClozeIndices(inlineOnly)).toEqual([])
    expect(hasClozeDeletions(inlineOnly)).toBe(false)
    expect(stripClozeMarkup(inlineOnly)).toBe(inlineOnly)

    const fencedOnly = '```md\n{{c1::inside fence}}\n```'
    expect(extractClozeIndices(fencedOnly)).toEqual([])
    expect(stripClozeMarkup(fencedOnly)).toBe(fencedOnly)

    const mixed = 'Code `{{c1::code}}` and real {{c2::text}}.'
    expect(extractClozeIndices(mixed)).toEqual([2])
    expect(stripClozeMarkup(mixed)).toBe('Code `{{c1::code}}` and real text.')
    expect(renderClozeMarkdown(mixed, 2, false)).toContain('`{{c1::code}}`')
    expect(renderClozeMarkdown(mixed, 2, false)).toContain(`[…](${CLOZE_BLANK_HREF})`)
  })

  it('property: extractClozeIndices agrees exactly with renderClozeMarkdown blanks and preserves code spans', () => {
    const tokens = [
      'plain ',
      '{{c1::alpha}}',
      '{{c2::beta::hint}}',
      '{{c3::$\\frac{1}{2}$}}',
      '{{c0::zero}}',
      '{{c99999999999::overflow}}',
      '{{c1::unclosed',
      '`{{c4::inline code}}`',
      '`\\(x^2\\)`',
      '\\(a + b\\)',
      '\\[E = mc^2\\]',
      '$5 and $10',
      '\\left\\{x\\right\\}',
      '日本語 ',
      '\n',
    ]

    forAll(150, 0xc102e1, (rng) => {
      const codeBlock = `\`\`\`ts\nconst x = "{{c5::fenced}}" + "\\(y\\)"\n\`\`\``
      const parts: string[] = []
      const len = randInt(rng, 1, 10)
      for (let i = 0; i < len; i++) {
        parts.push(pick(rng, tokens))
      }
      if (randBool(rng, 0.4)) {
        parts.push(`\n${codeBlock}\n`)
      }
      const source = parts.join('')

      // Never throws on arbitrary combinations.
      expect(() => parseClozeSpans(source)).not.toThrow()
      expect(() => normalizeMathDelimiters(source)).not.toThrow()
      expect(() => stripClozeMarkup(source)).not.toThrow()

      // Code block content stays byte-for-byte invariant across transforms.
      if (source.includes(codeBlock)) {
        expect(normalizeMathDelimiters(source)).toContain(codeBlock)
        expect(renderClozeMarkdown(source, 5, false)).toContain(codeBlock)
        expect(stripClozeMarkup(source)).toContain(codeBlock)
      }

      // Creation-rendering agreement: every extracted index produces a blank marker,
      // and indices only in code spans (like 4 and 5) are never extracted.
      const indices = extractClozeIndices(source)
      expect(indices).not.toContain(4)
      expect(indices).not.toContain(5)
      for (const idx of indices) {
        const rendered = renderClozeMarkdown(source, idx, false)
        expect(rendered).toContain(CLOZE_BLANK_HREF)
      }
      for (const candidate of [1, 2, 3, 4, 5]) {
        if (!indices.includes(candidate)) {
          expect(renderClozeMarkdown(source, candidate, false)).not.toContain(CLOZE_BLANK_HREF)
        }
      }
    })
  })

  it('property: mathPreprocess preserves currency, protects code, and round-trips LaTeX load/save', () => {
    const latexSamples = [
      '$x_1^2 + y_2^2$',
      '$\\frac{a_1}{b_2}$',
      '$$\\sum_{i=1}^{n} a_i * b_i$$',
      '\\(f(x_1) + g(x_2)\\)',
      '\\[\\alpha_1 + \\beta_2\\]',
    ]
    const currencySamples = [
      'Costs $5 and $10 today.',
      'Range is $5-$10 per year.',
      'Only $99.',
    ]

    forAll(100, 0x3a7401, (rng) => {
      const currency = pick(rng, currencySamples)
      const normalized = normalizeMathDelimiters(currency)
      // Every bare currency dollar sign is escaped.
      expect(normalized).not.toMatch(/(?<!\\)\$\d/)

      const math = pick(rng, latexSamples)
      const inlineCode = `\`${math}\``
      const doc = `Intro ${math} and ${inlineCode}.`
      const loaded = protectMathForEditorLoad(doc)
      expect(loaded).toContain(inlineCode)
      const saved = unescapeMathFromEditorSave(loaded)
      expect(saved).toContain(inlineCode)
      // Plain `$...$` and `$$...$$` round-trip cleanly.
      if (math.startsWith('$')) {
        expect(saved).toBe(doc)
      }
    })
  })

  it('property: imageSrc round-trips valid 64-hex hashes and rejects invalid ones', () => {
    forAll(100, 0xa55e71, (rng) => {
      const h1 = randString(rng, HEX, 64, 64)
      const align = pick(rng, ALIGNS)
      const src = buildAssetSrc(h1, align)
      expect(isAssetSrc(src)).toBe(true)
      expect(parseAssetSrc(src)).toEqual({ hash: h1, align })

      // Short / uppercase / non-hex hashes are rejected.
      expect(isAssetSrc(`asset:${h1.slice(0, 63)}`)).toBe(false)
      expect(isAssetSrc(`asset:${h1.toUpperCase()}`)).toBe(false)
      expect(parseAssetSrc(`asset:${h1}#invalid`)).toBeNull()
    })
  })
})
