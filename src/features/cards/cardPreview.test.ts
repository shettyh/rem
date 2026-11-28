import { describe, it, expect } from 'vitest'
import { cardPreview, cardSnippet } from './DeckDetailPage'

describe('cardPreview', () => {
  it('strips leading heading markers', () => {
    expect(cardPreview('## what is the first init function in golang ?')).toBe(
      'what is the first init function in golang ?',
    )
  })

  it('uses the first non-empty line', () => {
    expect(cardPreview('\n\n# Title\nbody')).toBe('Title')
  })

  it('strips inline emphasis and code markers', () => {
    expect(cardPreview('Use **bold**, _italic_ and `code` here')).toBe(
      'Use bold, italic and code here',
    )
  })

  it('keeps only the link text', () => {
    expect(cardPreview('See [the docs](https://example.com)')).toBe('See the docs')
  })

  it('strips list and blockquote markers', () => {
    expect(cardPreview('- first item')).toBe('first item')
    expect(cardPreview('> a quote')).toBe('a quote')
  })

  it('skips code-fence openers and returns the first code line', () => {
    const md = '```rust\nfn main() {\n  println!("hi");\n}\n```'
    expect(cardPreview(md)).toBe('fn main() {')
    expect(cardSnippet(md)).toEqual({ text: 'fn main() {', lang: 'rust' })
  })

  it('handles untagged code fences', () => {
    expect(cardSnippet('```\nplain code\n```')).toEqual({ text: 'plain code', lang: null })
  })

  it('returns an empty string for blank input', () => {
    expect(cardPreview('   \n  ')).toBe('')
  })

  it('cleans image, audio asset links, and ruby furigana syntax', () => {
    expect(cardPreview('![diagram](asset:0123456789abcdef)')).toBe('diagram')
    expect(cardPreview('![](asset:0123456789abcdef)')).toBe('[image]')
    expect(cardPreview('[Audio: hola.mp3](asset:0123456789abcdef)')).toBe('Audio: hola.mp3')
    expect(cardPreview('{答|こた}えに **{丸|まる}** をつける')).toBe('答えに 丸 をつける')
  })
})
