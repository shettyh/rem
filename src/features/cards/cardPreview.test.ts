import { describe, it, expect } from 'vitest'
import { cardPreview } from './DeckDetailPage'

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

  it('returns an empty string for blank input', () => {
    expect(cardPreview('   \n  ')).toBe('')
  })
})
