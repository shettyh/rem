import { describe, expect, it } from 'vitest'
import {
  composeOcclusionDocument,
  extractOcclusionDocument,
  extractOcclusionIndices,
  extractSiblingCardIndices,
  hasImageOcclusion,
  nextOcclusionMaskId,
  parseOcclusionBlocks,
  parseOcclusionJson,
  serializeOcclusionBlock,
  splitMarkdownOcclusion,
  stripOcclusionMarkup,
  type OcclusionBlock,
} from './occlusion'
import { cardSnippet } from './DeckDetailPage'

const HASH = 'a'.repeat(64)

describe('image occlusion markdown helpers', () => {
  it('parses, sanitizes, and round-trips valid rem-occlusion blocks', () => {
    const block: OcclusionBlock = {
      v: 1,
      image: `asset:${HASH}`,
      mode: 'hide-all',
      alt: 'CPU Pipeline',
      masks: [
        { id: 2, type: 'ellipse', x: 0.5, y: 0.4, w: 0.25, h: 0.15 },
        { id: 1, type: 'rect', x: 0.123456, y: 0.2, w: 0.3, h: 0.1 },
        { id: 1, type: 'rect', x: 0.1, y: 0.35, w: 0.2, h: 0.1 },
      ],
    }

    const md = composeOcclusionDocument('CPU Pipeline', block, 'See Hennessy & Patterson')
    expect(hasImageOcclusion(md)).toBe(true)
    expect(extractOcclusionIndices(md)).toEqual([1, 2])
    expect(extractSiblingCardIndices(md)).toEqual([1, 2])
    expect(nextOcclusionMaskId(block.masks)).toBe(3)

    const doc = extractOcclusionDocument(md)
    expect(doc).not.toBeNull()
    expect(doc?.header).toBe('CPU Pipeline')
    expect(doc?.footer).toBe('See Hennessy & Patterson')
    expect(doc?.block.masks[1].x).toBe(0.1235)

    const segments = splitMarkdownOcclusion(md)
    expect(segments.map((s) => s.kind)).toEqual(['markdown', 'occlusion', 'markdown'])
  })

  it('rejects non-asset URLs, unsupported versions, and tiny/invalid masks', () => {
    expect(
      parseOcclusionJson(
        JSON.stringify({
          v: 1,
          image: 'https://example.com/x.png',
          masks: [{ id: 1, x: 0.1, y: 0.1, w: 0.2, h: 0.2 }],
        }),
      ),
    ).toBeNull()

    expect(
      parseOcclusionJson(
        JSON.stringify({
          v: 2,
          image: `asset:${HASH}`,
          masks: [{ id: 1, x: 0.1, y: 0.1, w: 0.2, h: 0.2 }],
        }),
      ),
    ).toBeNull()

    const sanitized = parseOcclusionJson(
      JSON.stringify({
        v: 1,
        image: `asset:${HASH}`,
        masks: [
          { id: 0, x: 0.1, y: 0.1, w: 0.2, h: 0.2 },
          { id: 1, x: -0.5, y: 0.85, w: 0.4, h: 0.4 },
          { id: 2, x: 0.1, y: 0.1, w: 0.0005, h: 0.2 },
        ],
      }),
    )
    expect(sanitized?.masks).toEqual([
      { id: 1, type: 'rect', x: 0, y: 0.85, w: 0.4, h: 0.15 },
    ])
  })

  it('produces readable previews and strips raw JSON from card snippets', () => {
    const block: OcclusionBlock = {
      v: 1,
      image: `asset:${HASH}`,
      mode: 'hide-one',
      masks: [
        { id: 1, type: 'rect', x: 0.1, y: 0.1, w: 0.2, h: 0.2 },
        { id: 2, type: 'ellipse', x: 0.5, y: 0.5, w: 0.2, h: 0.2 },
      ],
    }
    const withoutHeader = serializeOcclusionBlock(block)
    expect(stripOcclusionMarkup(withoutHeader)).toBe('[Image occlusion · 2 masks]')
    expect(cardSnippet(withoutHeader)).toEqual({ text: '[Image occlusion · 2 masks]', lang: null })

    const withHeader = composeOcclusionDocument('## TCP State Machine', block)
    expect(cardSnippet(withHeader)).toEqual({ text: 'TCP State Machine', lang: null })
    expect(parseOcclusionBlocks(withHeader)).toHaveLength(1)
  })
})
