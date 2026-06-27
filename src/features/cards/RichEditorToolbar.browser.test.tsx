// src/features/cards/RichEditorToolbar.browser.test.tsx
import { describe, it, expect, vi } from 'vitest'
import { render } from 'vitest-browser-react'
import { userEvent } from 'vitest/browser'
import { useState } from 'react'
import type { Editor } from '@tiptap/react'
import { RichMarkdownEditor } from './RichMarkdownEditor'
import { EditorToolbar } from './EditorToolbar'

/** Mirrors the real wiring: one shared toolbar driving a focused editor. */
function Harness({ onImage = () => {} }: { onImage?: () => void }) {
  const [v, setV] = useState('')
  const [editor, setEditor] = useState<Editor | null>(null)
  return (
    <>
      <EditorToolbar editor={editor} onImage={onImage} />
      <RichMarkdownEditor value={v} onChange={setV} ariaLabel="Front" onReady={(h) => setEditor(h.editor)} />
      <pre data-testid="md">{v}</pre>
    </>
  )
}

/** The toolbar is wired to the editor once its dropdown trigger is enabled. */
async function ready(container: HTMLElement) {
  await expect
    .poll(() => container.querySelector('[aria-label="Text style"]')?.hasAttribute('disabled'))
    .toBe(false)
  return container.querySelector('.rich-editor-content') as HTMLElement
}

describe('shared editor toolbar', () => {
  it('bold turns typed text bold', async () => {
    const { container } = await render(<Harness />)
    const content = await ready(container)
    content.focus()
    await userEvent.click(container.querySelector('[aria-label="Bold"]')!)
    await userEvent.type(content, 'x')
    await expect.poll(() => container.querySelector('[data-testid="md"]')?.textContent).toContain('**x**')
  })

  it('strikethrough wraps typed text', async () => {
    const { container } = await render(<Harness />)
    const content = await ready(container)
    content.focus()
    await userEvent.click(container.querySelector('[aria-label="Strikethrough"]')!)
    await userEvent.type(content, 'y')
    await expect.poll(() => container.querySelector('[data-testid="md"]')?.textContent).toContain('~~y~~')
  })

  it('heading dropdown promotes the current block to H1', async () => {
    const { container } = await render(<Harness />)
    const content = await ready(container)
    content.focus()
    await userEvent.type(content, 'Title')
    await userEvent.click(container.querySelector('[aria-label="Text style"]')!)
    await userEvent.click(container.querySelector('.md-head-opt--h1')!)
    await expect.poll(() => container.querySelector('[data-testid="md"]')?.textContent).toContain('# Title')
  })

  it('exposes an image button that requests an image', async () => {
    const onImage = vi.fn()
    const { container } = await render(<Harness onImage={onImage} />)
    await ready(container)
    const button = container.querySelector('[aria-label="Image"]')
    expect(button).toBeTruthy()
    await userEvent.click(button!)
    expect(onImage).toHaveBeenCalled()
  })
})
