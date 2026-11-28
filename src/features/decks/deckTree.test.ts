import { describe, expect, it } from 'vitest'
import { DEFAULT_DECK_SETTINGS, type Deck } from '../../domain/models'
import {
  buildDeckTree,
  deckIsDescendantOrSelf,
  flattenVisibleDeckTree,
  normalizeDeckName,
} from './deckTree'

function makeDeck(id: string, name: string): Deck {
  return {
    id,
    name,
    createdAt: 0,
    updatedAt: 0,
    color: '#7e6cff',
    schedulerKind: 'fsrs',
    settings: { ...DEFAULT_DECK_SETTINGS },
  }
}

describe('deckTree', () => {
  it('normalizes :: paths and checks descendant membership safely', () => {
    expect(normalizeDeckName(' CS :: Systems :: Rust ')).toBe('CS::Systems::Rust')
    expect(deckIsDescendantOrSelf('CS::Rust', 'CS')).toBe(true)
    expect(deckIsDescendantOrSelf('CSS', 'CS')).toBe(false)
  })

  it('builds a nested tree with virtual folders and rolled-up counts', () => {
    const tree = buildDeckTree([
      { deck: makeDeck('1', 'CS'), due: 2, newCount: 1, total: 5, studied: 4 },
      { deck: makeDeck('2', 'CS::Rust'), due: 3, newCount: 2, total: 8, studied: 6 },
      { deck: makeDeck('3', 'Languages::Japanese'), due: 4, newCount: 1, total: 10, studied: 3 },
    ])

    expect(tree).toHaveLength(2)
    expect(tree[0].path).toBe('CS')
    expect(tree[0].due).toBe(5)
    expect(tree[0].total).toBe(13)
    expect(tree[0].children).toHaveLength(1)
    expect(tree[0].children[0].label).toBe('Rust')

    // Virtual parent folder synthesized for Languages::Japanese
    expect(tree[1].path).toBe('Languages')
    expect(tree[1].item).toBeNull()
    expect(tree[1].due).toBe(4)
    expect(tree[1].children[0].label).toBe('Japanese')

    const expanded = flattenVisibleDeckTree(tree, new Set())
    expect(expanded.map((n) => n.path)).toEqual(['CS', 'CS::Rust', 'Languages', 'Languages::Japanese'])

    const collapsed = flattenVisibleDeckTree(tree, new Set(['CS']))
    expect(collapsed.map((n) => n.path)).toEqual(['CS', 'Languages', 'Languages::Japanese'])
  })
})
