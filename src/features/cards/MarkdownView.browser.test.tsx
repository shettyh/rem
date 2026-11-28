// src/features/cards/MarkdownView.browser.test.tsx
import { describe, it, expect, beforeEach } from 'vitest'
import { render } from 'vitest-browser-react'
import Dexie from 'dexie'
import { RemDB } from '../../data/dexie/db'
import { DexieStorage } from '../../data/dexie/DexieStorage'
import { StorageProvider } from '../../data/StorageContext'
import { MarkdownView } from './MarkdownView'

beforeEach(async () => {
  await Dexie.delete('rem-mdview')
})

describe('MarkdownView assets', () => {
  it('renders an asset image as a blob-backed <img>', async () => {
    const storage = new DexieStorage(new RemDB('rem-mdview'))
    const asset = await storage.putAsset(new Uint8Array([137, 80, 78, 71]), 'image/png')
    const { container } = await render(
      <StorageProvider storage={storage}>
        <MarkdownView source={`![pic](asset:${asset.hash})`} />
      </StorageProvider>,
    )
    await expect.poll(() => container.querySelector('img')?.getAttribute('src')).toMatch(/^blob:/)
  })

  it('renders an audio asset link as a blob-backed <audio> player', async () => {
    const storage = new DexieStorage(new RemDB('rem-mdview'))
    const asset = await storage.putAsset(new Uint8Array([0x49, 0x44, 0x33]), 'audio/mpeg')
    const { container } = await render(
      <StorageProvider storage={storage}>
        <MarkdownView source={`[Audio: hola.mp3](asset:${asset.hash})`} />
      </StorageProvider>,
    )
    await expect.poll(() => container.querySelector('audio')?.getAttribute('src')).toMatch(/^blob:/)
  })

  it('renders KaTeX math and cloze blank/revealed states safely', async () => {
    const storage = new DexieStorage(new RemDB('rem-mdview'))
    const source = 'Formula {{c1::$E = mc^2$::Einstein}} and \\(a^2 + b^2\\).'

    const unrevealed = await render(
      <StorageProvider storage={storage}>
        <MarkdownView source={source} clozeIndex={1} clozeRevealed={false} />
      </StorageProvider>,
    )
    expect(unrevealed.container.querySelector('.cloze-blank')?.textContent).toBe('[Einstein]')
    expect(unrevealed.container.querySelector('.katex')).not.toBeNull()

    const revealed = await render(
      <StorageProvider storage={storage}>
        <MarkdownView source={source} clozeIndex={1} clozeRevealed={true} />
      </StorageProvider>,
    )
    const revealSpan = revealed.container.querySelector('.cloze-reveal')
    expect(revealSpan).not.toBeNull()
    expect(revealSpan?.querySelector('.katex')).not.toBeNull()
  })

  it('keeps currency like $5 and $10 as plain text while rendering multiline display math', async () => {
    const storage = new DexieStorage(new RemDB('rem-mdview'))
    const currency = await render(
      <StorageProvider storage={storage}>
        <MarkdownView source="Costs $5 and $10 today (between $5-$10)." />
      </StorageProvider>,
    )
    expect(currency.container.querySelector('.katex')).toBeNull()
    expect(currency.container.textContent).toContain('$5 and $10')

    const multiline = await render(
      <StorageProvider storage={storage}>
        <MarkdownView source={'\\[\nE = mc^2\n\\]'} />
      </StorageProvider>,
    )
    expect(multiline.container.querySelector('.katex-display')).not.toBeNull()
  })
})
