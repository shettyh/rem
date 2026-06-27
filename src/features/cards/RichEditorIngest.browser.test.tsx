// src/features/cards/RichEditorIngest.browser.test.tsx
import { describe, it, expect } from 'vitest'
import { render } from 'vitest-browser-react'
import { userEvent } from 'vitest/browser'
import { useState } from 'react'
import { RichMarkdownEditor } from './RichMarkdownEditor'

const HASH = 'a'.repeat(64)

function Harness() {
  const [v, setV] = useState('')
  const ingestImage = async (_file: File) => ({ hash: HASH, mime: 'image/png' })
  return (
    <>
      <RichMarkdownEditor value={v} onChange={setV} ariaLabel="Front" ingestImage={ingestImage} />
      <pre data-testid="md">{v}</pre>
    </>
  )
}

describe('image ingestion', () => {
  it('inserts an asset image when a file is chosen', async () => {
    const { container } = await render(<Harness />)
    const input = container.querySelector('input[type="file"]') as HTMLInputElement
    const file = new File([new Uint8Array([1, 2, 3])], 'p.png', { type: 'image/png' })
    await userEvent.upload(input, file)
    await expect.poll(() => container.querySelector('[data-testid="md"]')?.textContent).toContain(`asset:${HASH}`)
  })
})
