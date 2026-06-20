import { describe, it, expect } from 'vitest'
import { Editor } from '@tiptap/core'
import { createEditorExtensions } from './editorExtensions'

/** Parse markdown into the editor, then serialize it straight back out. */
function roundTrip(markdown: string): string {
  const editor = new Editor({
    extensions: createEditorExtensions(),
    content: markdown,
  })
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const out = (editor.storage as any).markdown.getMarkdown()
  editor.destroy()
  return out
}

describe('editor markdown round-trip', () => {
  it('preserves bold text', () => {
    expect(roundTrip('hello **world**')).toContain('**world**')
  })

  it('preserves a fenced code block with its language', () => {
    const out = roundTrip('```js\nconst x = 1\n```')
    expect(out).toContain('```js')
    expect(out).toContain('const x = 1')
  })

  it('preserves bullet list items', () => {
    const out = roundTrip('- one\n- two')
    expect(out).toMatch(/[-*]\s+one/)
    expect(out).toContain('two')
  })
})
