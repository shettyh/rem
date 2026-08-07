import { describe, expect, it } from 'vitest'
import { mergeUserTags, parseUserTags, userTags } from './cardTags'

describe('card tags', () => {
  it('parses comma-separated tags and removes empty and case-insensitive duplicates', () => {
    expect(parseUserTags(' grammar, Chapter 1, grammar, , CHAPTER 1 ')).toEqual([
      'grammar',
      'Chapter 1',
    ])
  })

  it('reserves the leech system tag', () => {
    expect(parseUserTags('hard, leech, Leech')).toEqual(['hard'])
  })

  it('replaces user tags without removing system tags', () => {
    expect(mergeUserTags(['old', 'leech'], 'new, important')).toEqual([
      'leech',
      'new',
      'important',
    ])
    expect(userTags(['leech', 'new'])).toEqual(['new'])
  })
})
