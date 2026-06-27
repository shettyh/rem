import { useEffect, useRef } from 'react'
import { useEditor, EditorContent } from '@tiptap/react'
import { createEditorExtensions } from './editorExtensions'
import 'highlight.js/styles/github-dark.css'

/** A single inline WYSIWYG-markdown editor. Markdown is the value in and out. */
export function RichMarkdownEditor({
  value,
  onChange,
  placeholder,
  ariaLabel,
  resolveAsset,
  ingestImage,
}: {
  value: string
  onChange: (markdown: string) => void
  placeholder?: string
  ariaLabel?: string
  resolveAsset?: (hash: string) => Promise<string | null>
  ingestImage?: (file: File) => Promise<{ hash: string; mime: string }>
}) {
  const onChangeRef = useRef(onChange)
  useEffect(() => {
    onChangeRef.current = onChange
  }, [onChange])

  const fileInputRef = useRef<HTMLInputElement>(null)
  const insertImageRef = useRef<(file: File) => void>(() => {})

  const editor = useEditor({
    extensions: createEditorExtensions(placeholder, resolveAsset),
    content: value,
    immediatelyRender: false,
    editorProps: {
      attributes: {
        class: 'rich-editor-content',
        ...(ariaLabel ? { 'aria-label': ariaLabel } : {}),
      },
      handlePaste: (_view: unknown, event: ClipboardEvent) => {
        const file = [...(event.clipboardData?.items ?? [])]
          .find((i) => i.type.startsWith('image/'))?.getAsFile()
        if (file) {
          insertImageRef.current(file)
          return true
        }
        return false
      },
      handleDrop: (_view: unknown, event: DragEvent) => {
        const file = [...(event.dataTransfer?.files ?? [])].find((f) => f.type.startsWith('image/'))
        if (file) {
          event.preventDefault()
          insertImageRef.current(file)
          return true
        }
        return false
      },
    },
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    onUpdate: ({ editor }) => onChangeRef.current((editor.storage as any).markdown.getMarkdown()),
  })

  insertImageRef.current = (file: File) => {
    if (!editor || !ingestImage || !file.type.startsWith('image/')) return
    void ingestImage(file).then(({ hash }) => {
      editor.chain().focus().setImage({ src: `asset:${hash}` }).run()
    })
  }

  // Re-sync when the external value changes (e.g. async card load on Edit).
  // Guard prevents cursor-jump / feedback loops while typing.
  useEffect(() => {
    if (!editor) return
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    if (value !== (editor.storage as any).markdown.getMarkdown()) {
      editor.commands.setContent(value, { emitUpdate: false })
    }
  }, [value, editor])

  function setLink() {
    if (!editor) return
    if (editor.isActive('link')) {
      editor.chain().focus().unsetLink().run()
      return
    }
    const url = window.prompt('URL')
    if (url) editor.chain().focus().extendMarkRange('link').setLink({ href: url }).run()
  }

  return (
    <div className="rich-editor">
      {editor && (
        <div className="editor-toolbar" role="toolbar" aria-label="Formatting">
          <button type="button" aria-label="Heading 1" onMouseDown={(e) => e.preventDefault()}
            className={editor.isActive('heading', { level: 1 }) ? 'active' : ''}
            onClick={() => editor.chain().focus().toggleHeading({ level: 1 }).run()}>H1</button>
          <button type="button" aria-label="Heading 2" onMouseDown={(e) => e.preventDefault()}
            className={editor.isActive('heading', { level: 2 }) ? 'active' : ''}
            onClick={() => editor.chain().focus().toggleHeading({ level: 2 }).run()}>H2</button>
          <button type="button" aria-label="Heading 3" onMouseDown={(e) => e.preventDefault()}
            className={editor.isActive('heading', { level: 3 }) ? 'active' : ''}
            onClick={() => editor.chain().focus().toggleHeading({ level: 3 }).run()}>H3</button>
          <span className="toolbar-sep" />
          <button type="button" aria-label="Bold" onMouseDown={(e) => e.preventDefault()}
            className={editor.isActive('bold') ? 'active' : ''}
            onClick={() => editor.chain().focus().toggleBold().run()}>B</button>
          <button type="button" aria-label="Italic" onMouseDown={(e) => e.preventDefault()}
            className={editor.isActive('italic') ? 'active' : ''}
            onClick={() => editor.chain().focus().toggleItalic().run()}>i</button>
          <button type="button" aria-label="Inline code" onMouseDown={(e) => e.preventDefault()}
            className={editor.isActive('code') ? 'active' : ''}
            onClick={() => editor.chain().focus().toggleCode().run()}>{'</>'}</button>
          <button type="button" aria-label="Code block" onMouseDown={(e) => e.preventDefault()}
            className={editor.isActive('codeBlock') ? 'active' : ''}
            onClick={() => editor.chain().focus().toggleCodeBlock().run()}>{'{ }'}</button>
          <span className="toolbar-sep" />
          <button type="button" aria-label="Bullet list" onMouseDown={(e) => e.preventDefault()}
            className={editor.isActive('bulletList') ? 'active' : ''}
            onClick={() => editor.chain().focus().toggleBulletList().run()}>•</button>
          <button type="button" aria-label="Numbered list" onMouseDown={(e) => e.preventDefault()}
            className={editor.isActive('orderedList') ? 'active' : ''}
            onClick={() => editor.chain().focus().toggleOrderedList().run()}>1.</button>
          <button type="button" aria-label="Quote" onMouseDown={(e) => e.preventDefault()}
            className={editor.isActive('blockquote') ? 'active' : ''}
            onClick={() => editor.chain().focus().toggleBlockquote().run()}>&ldquo;</button>
          <span className="toolbar-sep" />
          <button type="button" aria-label="Link" onMouseDown={(e) => e.preventDefault()}
            className={editor.isActive('link') ? 'active' : ''} onClick={setLink}>link</button>
          <button type="button" aria-label="Image" onMouseDown={(e) => e.preventDefault()}
            onClick={() => fileInputRef.current?.click()}>img</button>
        </div>
      )}
      <input
        ref={fileInputRef}
        type="file"
        accept="image/*"
        style={{ display: 'none' }}
        onChange={(e) => {
          const file = e.target.files?.[0]
          if (file) insertImageRef.current(file)
          e.target.value = ''
        }}
      />
      <EditorContent editor={editor} />
    </div>
  )
}
