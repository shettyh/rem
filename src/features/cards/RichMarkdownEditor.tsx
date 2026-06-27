import { useEffect, useRef } from 'react'
import { useEditor, EditorContent, type Editor } from '@tiptap/react'
import { createEditorExtensions } from './editorExtensions'
import 'highlight.js/styles/github-dark.css'

/** Imperative handle handed to the parent once the editor is live, so a shared
 *  toolbar can drive whichever field is focused. */
export interface EditorHandle {
  editor: Editor
  openImagePicker: () => void
}

/** A single inline WYSIWYG-markdown editor. Markdown is the value in and out.
 *  Formatting controls live in a shared toolbar (see EditorToolbar); this owns
 *  only the editing surface plus paste/drop/file-picker image ingestion. */
export function RichMarkdownEditor({
  value,
  onChange,
  placeholder,
  ariaLabel,
  resolveAsset,
  ingestImage,
  onReady,
  onFocus,
}: {
  value: string
  onChange: (markdown: string) => void
  placeholder?: string
  ariaLabel?: string
  resolveAsset?: (hash: string) => Promise<string | null>
  ingestImage?: (file: File) => Promise<{ hash: string; mime: string }>
  onReady?: (handle: EditorHandle) => void
  onFocus?: () => void
}) {
  const onChangeRef = useRef(onChange)
  const onReadyRef = useRef(onReady)
  const onFocusRef = useRef(onFocus)
  useEffect(() => {
    onChangeRef.current = onChange
    onReadyRef.current = onReady
    onFocusRef.current = onFocus
  })

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

  // Hand the live editor (plus its image picker) up to the shared toolbar, and
  // report focus so the toolbar retargets to the field the user is editing.
  useEffect(() => {
    if (!editor) return
    const openImagePicker = () => fileInputRef.current?.click()
    onReadyRef.current?.({ editor, openImagePicker })
    const handleFocus = () => onFocusRef.current?.()
    editor.on('focus', handleFocus)
    return () => {
      editor.off('focus', handleFocus)
    }
  }, [editor])

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
