import { describe, expect, it } from 'vitest'
import { DEFAULT_DECK_SETTINGS, type Deck } from '../../domain/models'
import {
  buildDeckTree,
  deckIsDescendantOrSelf,
  deckIsStrictDescendant,
  describeDeckDelete,
  flattenVisibleDeckTree,
  normalizeDeckName,
  pruneCollapsedDeckPaths,
  summarizeDeckSubtree,
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
    expect(() => normalizeDeckName('   ')).toThrow()
    expect(() => normalizeDeckName('::Rust')).toThrow()
    expect(() => normalizeDeckName('Rust::')).toThrow()
    expect(() => normalizeDeckName('CS::::Rust')).toThrow()
    expect(deckIsDescendantOrSelf('CS::Rust', 'CS')).toBe(true)
    expect(deckIsDescendantOrSelf('CSS', 'CS')).toBe(false)
  })

  it('preserves duplicate-named decks without overwriting them', () => {
    const tree = buildDeckTree([
      { deck: makeDeck('1', 'CS'), due: 2, newCount: 1, total: 5, studied: 4 },
      { deck: makeDeck('2', 'CS'), due: 3, newCount: 2, total: 6, studied: 4 },
    ])
    expect(tree).toHaveLength(2)
    expect(tree[0].item?.deck.id).toBe('1')
    expect(tree[1].item?.deck.id).toBe('2')
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

  it('checks strict descendants, summarizes subtrees, formats delete copy, and prunes collapsed paths', () => {
    expect(deckIsStrictDescendant('CS', 'CS')).toBe(false)
    expect(deckIsStrictDescendant('CS::Rust', 'CS')).toBe(true)
    expect(deckIsStrictDescendant('CSS', 'CS')).toBe(false)

    const tree = buildDeckTree([
      { deck: makeDeck('1', 'Anatomy::Unit 01::Cells'), due: 2, newCount: 1, total: 10, studied: 4 },
      { deck: makeDeck('2', 'Anatomy::Unit 02::Bones'), due: 3, newCount: 2, total: 15, studied: 5 },
      { deck: makeDeck('3', 'CS'), due: 1, newCount: 0, total: 4, studied: 4 },
      { deck: makeDeck('4', 'CS::Rust'), due: 2, newCount: 1, total: 6, studied: 2 },
      { deck: makeDeck('5', 'Solo'), due: 0, newCount: 0, total: 0, studied: 0 },
    ])

    // Virtual parent folder "Anatomy"
    const anatomy = tree[0]
    expect(summarizeDeckSubtree(anatomy)).toEqual({
      deckCount: 2,
      subdeckCount: 2,
      cardCount: 25,
      deckIds: ['1', '2'],
      paths: [
        'Anatomy',
        'Anatomy::Unit 01',
        'Anatomy::Unit 01::Cells',
        'Anatomy::Unit 02',
        'Anatomy::Unit 02::Bones',
      ],
    })
    expect(describeDeckDelete(anatomy)).toEqual({
      title: 'Delete "Anatomy"?',
      description: 'Permanently deletes 2 subdecks and 25 cards inside "Anatomy". This can\'t be undone.',
      confirmLabel: 'Delete all',
    })

    // Real parent deck "CS"
    const cs = tree[1]
    expect(summarizeDeckSubtree(cs)).toMatchObject({
      deckCount: 2,
      subdeckCount: 1,
      cardCount: 10,
      deckIds: ['3', '4'],
    })
    expect(describeDeckDelete(cs)).toEqual({
      title: 'Delete "CS" and subdecks?',
      description: 'Permanently deletes "CS", 1 subdeck, and 10 cards total. This can\'t be undone.',
      confirmLabel: 'Delete all',
    })

    // Empty leaf deck "Solo"
    expect(describeDeckDelete(tree[2])).toEqual({
      title: 'Delete "Solo"?',
      description: 'Permanently deletes the empty deck "Solo". This can\'t be undone.',
      confirmLabel: 'Delete deck',
    })

    const pruned = pruneCollapsedDeckPaths(
      new Set(['Anatomy', 'Anatomy::Unit 01', 'Anatomy2', 'CS']),
      ['Anatomy'],
    )
    expect([...pruned]).toEqual(['Anatomy2', 'CS'])
  })
})
