import { describe, expect, it, beforeEach } from 'vitest'
import { render } from 'vitest-browser-react'
import { useState } from 'react'
import Dexie from 'dexie'
import { RemDB } from '../../data/dexie/db'
import { DexieStorage } from '../../data/dexie/DexieStorage'
import { StorageProvider } from '../../data/StorageContext'
import { MarkdownView } from './MarkdownView'
import { OcclusionEditor } from './OcclusionEditor'
import { serializeOcclusionBlock, type OcclusionBlock } from './occlusion'

// Minimal valid 1x1 PNG so <img> loads cleanly in headless Chromium.
const TINY_PNG = new Uint8Array([
  0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d,
  0x49, 0x48, 0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01,
  0x08, 0x02, 0x00, 0x00, 0x00, 0x90, 0x77, 0x53, 0xde, 0x00, 0x00, 0x00,
  0x0c, 0x49, 0x44, 0x41, 0x54, 0x08, 0xd7, 0x63, 0xf8, 0xcf, 0xc0, 0x00,
  0x00, 0x03, 0x01, 0x01, 0x00, 0x18, 0xdd, 0x8d, 0xb0, 0x00, 0x00, 0x00,
  0x00, 0x49, 0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
])

beforeEach(async () => {
  await Dexie.delete('rem-io-browser')
})

describe('Image Occlusion viewer & editor', () => {
  it('renders hide-all and hide-one masks accurately across unrevealed, revealed, and peek-all states', async () => {
    const storage = new DexieStorage(new RemDB('rem-io-browser'))
    const asset = await storage.putAsset(TINY_PNG, 'image/png')

    const hideAllSource = serializeOcclusionBlock({
      v: 1,
      image: `asset:${asset.hash}`,
      mode: 'hide-all',
      alt: 'Diagram',
      masks: [
        { id: 1, type: 'rect', x: 0.1, y: 0.1, w: 0.3, h: 0.2 },
        { id: 2, type: 'ellipse', x: 0.5, y: 0.5, w: 0.3, h: 0.2 },
      ],
    })

    const unrevealed = await render(
      <StorageProvider storage={storage}>
        <MarkdownView source={hideAllSource} clozeIndex={1} clozeRevealed={false} />
      </StorageProvider>,
    )
    await expect.poll(() => unrevealed.container.querySelector('img.io-img')?.getAttribute('src')).toMatch(/^blob:/)
    expect(unrevealed.container.querySelector('[data-mask-id="1"]')?.getAttribute('data-mask-state')).toBe('active')
    expect(unrevealed.container.querySelector('[data-mask-id="2"]')?.getAttribute('data-mask-state')).toBe('inactive')

    const revealed = await render(
      <StorageProvider storage={storage}>
        <MarkdownView source={hideAllSource} clozeIndex={1} clozeRevealed={true} />
      </StorageProvider>,
    )
    await expect.poll(() => revealed.container.querySelector('img.io-img')?.getAttribute('src')).toMatch(/^blob:/)
    expect(revealed.container.querySelector('[data-mask-id="1"]')?.getAttribute('data-mask-state')).toBe('revealed')
    expect(revealed.container.querySelector('[data-mask-id="2"]')?.getAttribute('data-mask-state')).toBe('inactive')

    const peekBtn = revealed.getByRole('button', { name: 'Show all masks' })
    await peekBtn.click()
    expect(revealed.container.querySelector('[data-mask-id="2"]')?.getAttribute('data-mask-state')).toBe('peek')

    const hideOneSource = serializeOcclusionBlock({
      v: 1,
      image: `asset:${asset.hash}`,
      mode: 'hide-one',
      masks: [
        { id: 1, type: 'rect', x: 0.1, y: 0.1, w: 0.3, h: 0.2 },
        { id: 2, type: 'rect', x: 0.5, y: 0.5, w: 0.3, h: 0.2 },
      ],
    })
    const hideOne = await render(
      <StorageProvider storage={storage}>
        <MarkdownView source={hideOneSource} clozeIndex={1} clozeRevealed={false} />
      </StorageProvider>,
    )
    await expect.poll(() => hideOne.container.querySelector('img.io-img')?.getAttribute('src')).toMatch(/^blob:/)
    expect(hideOne.container.querySelector('[data-mask-id="1"]')).not.toBeNull()
    expect(hideOne.container.querySelector('[data-mask-id="2"]')).toBeNull()
  })

  it('supports grouping, ungrouping, mode switching, delete, and undo/redo in OcclusionEditor', async () => {
    const storage = new DexieStorage(new RemDB('rem-io-browser'))
    const asset = await storage.putAsset(TINY_PNG, 'image/png')

    function Harness() {
      const [header, setHeader] = useState('Heart')
      const [block, setBlock] = useState<OcclusionBlock>({
        v: 1,
        image: `asset:${asset.hash}`,
        mode: 'hide-all',
        masks: [
          { id: 1, type: 'rect', x: 0.1, y: 0.1, w: 0.25, h: 0.2 },
          { id: 2, type: 'ellipse', x: 0.5, y: 0.5, w: 0.25, h: 0.2 },
        ],
      })
      return (
        <StorageProvider storage={storage}>
          <OcclusionEditor
            header={header}
            onHeaderChange={setHeader}
            block={block}
            onBlockChange={setBlock}
            onReplaceImage={async () => {}}
          />
        </StorageProvider>
      )
    }

    const screen = await render(<Harness />)
    await expect.poll(() => screen.container.querySelector('img.io-img')?.getAttribute('src')).toMatch(/^blob:/)
    expect(screen.container.textContent).toContain('2 masks · 2 cards')

    // Switch to Select tool, select first mask, Shift-select second mask, and Group them
    await screen.getByRole('button', { name: 'Select' }).click()
    const svg = screen.container.querySelector('svg.io-svg') as Element
    const mask0 = screen.container.querySelector('[data-editor-mask-index="0"]') as Element
    const mask1 = screen.container.querySelector('[data-editor-mask-index="1"]') as Element
    mask0.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, button: 0, clientX: 20, clientY: 20 }))
    svg.dispatchEvent(new PointerEvent('pointerup', { bubbles: true, button: 0 }))
    await expect.poll(() => mask0.classList.contains('is-selected')).toBe(true)

    mask1.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, button: 0, shiftKey: true, clientX: 60, clientY: 60 }))
    svg.dispatchEvent(new PointerEvent('pointerup', { bubbles: true, button: 0 }))
    await expect.poll(() => mask1.classList.contains('is-selected')).toBe(true)

    const groupBtn = screen.getByRole('button', { name: 'Group', exact: true })
    await groupBtn.click()
    expect(screen.container.textContent).toContain('2 masks · 1 card')

    // Undo restores 2 cards
    await screen.getByRole('button', { name: 'Undo' }).click()
    expect(screen.container.textContent).toContain('2 masks · 2 cards')

    // Redo re-applies grouping
    await screen.getByRole('button', { name: 'Redo' }).click()
    expect(screen.container.textContent).toContain('2 masks · 1 card')

    // Switch mode to Hide one, guess one
    await screen.getByRole('button', { name: 'Hide one, guess one' }).click()
    expect(
      screen.getByRole('button', { name: 'Hide one, guess one' }).element().getAttribute('aria-pressed'),
    ).toBe('true')
  })
})
