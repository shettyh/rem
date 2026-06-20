import { useEffect } from 'react'
import { useEditor, EditorContent } from '@tiptap/react'
import { BubbleMenu } from '@tiptap/react/menus'
import { createEditorExtensions } from './editorExtensions'
import 'highlight.js/styles/github-dark.css'

/** A single inline WYSIWYG-markdown editor. Markdown is the value in and out. */
export function RichMarkdownEditor({
  value,
  onChange,
  placeholder,
  ariaLabel,
}: {
  value: string
  onChange: (markdown: string) => void
  placeholder?: string
  ariaLabel?: string
}) {
  const editor = useEditor({
    extensions: createEditorExtensions(placeholder),
    content: value,
    immediatelyRender: false,
    editorProps: {
      attributes: {
        class: 'rich-editor-content',
        ...(ariaLabel ? { 'aria-label': ariaLabel } : {}),
      },
    },
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    onUpdate: ({ editor }) => onChange((editor.storage as any).markdown.getMarkdown()),
  })

  // Re-sync when the external value changes (e.g. async card load on Edit).
  // Guard prevents cursor-jump / feedback loops while typing.
  useEffect(() => {
    if (!editor) return
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    if (value !== (editor.storage as any).markdown.getMarkdown()) {
      editor.commands.setContent(value, { emitUpdate: false })
    }
  }, [value, editor])

  return (
    <div className="rich-editor">
      {editor && (
        <BubbleMenu editor={editor} className="bubble-menu">
          <button type="button" onMouseDown={(e) => e.preventDefault()}
            onClick={() => editor.chain().focus().toggleBold().run()}
            className={editor.isActive('bold') ? 'active' : ''}>B</button>
          <button type="button" onMouseDown={(e) => e.preventDefault()}
            onClick={() => editor.chain().focus().toggleItalic().run()}
            className={editor.isActive('italic') ? 'active' : ''}>i</button>
          <button type="button" onMouseDown={(e) => e.preventDefault()}
            onClick={() => editor.chain().focus().toggleCode().run()}
            className={editor.isActive('code') ? 'active' : ''}>{'</>'}</button>
        </BubbleMenu>
      )}
      <EditorContent editor={editor} />
    </div>
  )
}
