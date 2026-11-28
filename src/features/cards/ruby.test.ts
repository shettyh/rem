import { describe, it, expect } from 'vitest'
import { splitRubyTokens, stripRubyMarkup } from './ruby'

describe('ruby furigana helpers', () => {
  it('splits {base|reading} tokens while leaving plain text untouched', () => {
    expect(splitRubyTokens('{答|こた}えに{丸|まる}をつける')).toEqual([
      { kind: 'ruby', text: '答', reading: 'こた' },
      { kind: 'text', text: 'えに' },
      { kind: 'ruby', text: '丸', reading: 'まる' },
      { kind: 'text', text: 'をつける' },
    ])
  })

  it('ignores malformed braces, multiple pipes, and empty halves', () => {
    expect(splitRubyTokens('{|reading} {base|} {a|b|c} {plain}')).toEqual([
      { kind: 'text', text: '{|reading} {base|} {a|b|c} {plain}' },
    ])
  })

  it('strips ruby readings outside code spans and preserves code blocks', () => {
    expect(stripRubyMarkup('{答|こた}えに **{丸|まる}** をつける')).toBe('答えに **丸** をつける')
    expect(stripRubyMarkup('Keep `{答|こた}` in code and {丸|まる} outside')).toBe(
      'Keep `{答|こた}` in code and 丸 outside',
    )
  })
})
