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
})
