// src/features/cards/RichEditorIngest.browser.test.tsx
import { describe, it, expect } from 'vitest'
import { render } from 'vitest-browser-react'
import { userEvent } from 'vitest/browser'
import { useState } from 'react'
import { RichMarkdownEditor } from './RichMarkdownEditor'

const HASH = 'a'.repeat(64)

function Harness({ initial = '' }: { initial?: string }) {
  const [v, setV] = useState(initial)
  const ingestAsset = async (_file: File, mimeOverride?: string) => ({
    hash: HASH,
    mime: mimeOverride ?? 'image/png',
  })
  return (
    <>
      <RichMarkdownEditor value={v} onChange={setV} ariaLabel="Front" ingestAsset={ingestAsset} />
      <pre data-testid="md">{v}</pre>
    </>
  )
}

describe('media & math editor round-trip', () => {
  it('inserts an asset image when a file is chosen', async () => {
    const { container } = await render(<Harness />)
    const input = container.querySelector('input[type="file"][accept="image/*"]') as HTMLInputElement
    const file = new File([new Uint8Array([1, 2, 3])], 'p.png', { type: 'image/png' })
    await userEvent.upload(input, file)
    await expect.poll(() => container.querySelector('[data-testid="md"]')?.textContent).toContain(`asset:${HASH}`)
  })

  it('inserts audio clips as valid asset links and preserves existing audio and LaTeX on edit', async () => {
    const { container } = await render(<Harness />)
    const audioInput = container.querySelector('input[type="file"][accept*="audio/"]') as HTMLInputElement
    const audioFile = new File([new Uint8Array([0x49, 0x44, 0x33])], 'hola.mp3', { type: 'audio/mpeg' })
    await userEvent.upload(audioInput, audioFile)
    await expect
      .poll(() => container.querySelector('[data-testid="md"]')?.textContent)
      .toContain(`[Audio: hola.mp3](asset:${HASH})`)

    // Round-trip existing card containing both an audio asset link and LaTeX formulas
    const initial = `Formula $x_1 + \\frac{1}{2}$ and \\(a^2\\) [Audio: clip.mp3](asset:${HASH})`
    const roundTrip = await render(<Harness initial={initial} />)
    const editorEl = roundTrip.container.querySelector('.rich-editor-content') as HTMLElement
    await userEvent.click(editorEl)
    await userEvent.keyboard('!')
    await expect.poll(() => roundTrip.container.querySelector('[data-testid="md"]')?.textContent).toContain(`\\frac{1}{2}`)
    const out = roundTrip.container.querySelector('[data-testid="md"]')?.textContent ?? ''
    expect(out).toContain(`(asset:${HASH})`)
    expect(out).toContain(`\\(a^2\\)`)
    expect(out).not.toContain(`\\\\frac`)
  })
})
