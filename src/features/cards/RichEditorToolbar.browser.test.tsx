// src/features/cards/RichEditorToolbar.browser.test.tsx
import { describe, it, expect } from 'vitest'
import { render } from 'vitest-browser-react'
import { userEvent } from '@vitest/browser/context'
import { useState } from 'react'
import { RichMarkdownEditor } from './RichMarkdownEditor'

function Harness() {
  const [v, setV] = useState('')
  return (
    <>
      <RichMarkdownEditor value={v} onChange={setV} ariaLabel="Front" />
      <pre data-testid="md">{v}</pre>
    </>
  )
}

describe('editor toolbar', () => {
  it('bold turns typed text bold via the toolbar', async () => {
    const { container } = await render(<Harness />)
    const content = container.querySelector('.rich-editor-content') as HTMLElement
    content.focus()
    // Toggle bold on (collapsed selection sets the stored mark), then type.
    await userEvent.click(container.querySelector('[aria-label="Bold"]')!)
    await userEvent.type(content, 'x')
    await expect.poll(() => container.querySelector('[data-testid="md"]')?.textContent).toContain('**x**')
  })

  it('exposes an image button', async () => {
    const { container } = await render(<RichMarkdownEditor value="" onChange={() => {}} />)
    expect(container.querySelector('[aria-label="Image"]')).toBeTruthy()
  })
})
