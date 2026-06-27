import { useEffect, useRef } from 'react'
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
  resolveAsset,
}: {
  value: string
  onChange: (markdown: string) => void
  placeholder?: string
  ariaLabel?: string
  resolveAsset?: (hash: string) => Promise<string | null>
}) {
  const onChangeRef = useRef(onChange)
  useEffect(() => {
    onChangeRef.current = onChange
  }, [onChange])

  const editor = useEditor({
    extensions: createEditorExtensions(placeholder, resolveAsset),
    content: value,
    immediatelyRender: false,
    editorProps: {
      attributes: {
        class: 'rich-editor-content',
        ...(ariaLabel ? { 'aria-label': ariaLabel } : {}),
      },
    },
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    onUpdate: ({ editor }) => onChangeRef.current((editor.storage as any).markdown.getMarkdown()),
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
            aria-label="Bold" aria-pressed={editor.isActive('bold')}
            onClick={() => editor.chain().focus().toggleBold().run()}
            className={editor.isActive('bold') ? 'active' : ''}>B</button>
          <button type="button" onMouseDown={(e) => e.preventDefault()}
            aria-label="Italic" aria-pressed={editor.isActive('italic')}
            onClick={() => editor.chain().focus().toggleItalic().run()}
            className={editor.isActive('italic') ? 'active' : ''}>i</button>
          <button type="button" onMouseDown={(e) => e.preventDefault()}
            aria-label="Inline code" aria-pressed={editor.isActive('code')}
            onClick={() => editor.chain().focus().toggleCode().run()}
            className={editor.isActive('code') ? 'active' : ''}>{'</>'}</button>
          <button type="button" onMouseDown={(e) => e.preventDefault()}
            aria-label="Link" aria-pressed={editor.isActive('link')}
            onClick={() => {
              if (editor.isActive('link')) {
                editor.chain().focus().unsetLink().run()
              } else {
                const url = window.prompt('URL')
                if (url) editor.chain().focus().extendMarkRange('link').setLink({ href: url }).run()
              }
            }}
            className={editor.isActive('link') ? 'active' : ''}>link</button>
        </BubbleMenu>
      )}
      <EditorContent editor={editor} />
    </div>
  )
}
