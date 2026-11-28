import { useEffect, useRef } from 'react'
import { useEditor, EditorContent, type Editor } from '@tiptap/react'
import { createEditorExtensions } from './editorExtensions'
import { protectMathForEditorLoad, unescapeMathFromEditorSave } from './mathPreprocess'
import 'highlight.js/styles/github-dark.css'

const MAX_MEDIA_UPLOAD_BYTES = 25 * 1024 * 1024

const AUDIO_EXT_TO_MIME: Record<string, string> = {
  mp3: 'audio/mpeg',
  m4a: 'audio/mp4',
  aac: 'audio/aac',
  ogg: 'audio/ogg',
  oga: 'audio/ogg',
  opus: 'audio/ogg',
  wav: 'audio/wav',
  flac: 'audio/flac',
  webm: 'audio/webm',
}

function resolveAudioMime(file: File): string | null {
  if (file.type.startsWith('audio/')) return file.type
  const ext = file.name.split('.').pop()?.toLowerCase() ?? ''
  return AUDIO_EXT_TO_MIME[ext] ?? null
}

/** Imperative handle handed to the parent once the editor is live, so a shared
 *  toolbar can drive whichever field is focused. */
export interface EditorHandle {
  editor: Editor
  openImagePicker: () => void
  openAudioPicker: () => void
}

/** A single inline WYSIWYG-markdown editor. Markdown is the value in and out.
 *  Formatting controls live in a shared toolbar (see EditorToolbar); this owns
 *  only the editing surface plus paste/drop/file-picker media ingestion. */
export function RichMarkdownEditor({
  value,
  onChange,
  placeholder,
  ariaLabel,
  autoFocus = false,
  resolveAsset,
  ingestAsset,
  ingestImage,
  onMediaError,
  onReady,
  onFocus,
}: {
  value: string
  onChange: (markdown: string) => void
  placeholder?: string
  ariaLabel?: string
  autoFocus?: boolean
  resolveAsset?: (hash: string) => Promise<string | null>
  ingestAsset?: (file: File, mimeOverride?: string) => Promise<{ hash: string; mime: string }>
  /** @deprecated Prefer `ingestAsset`; kept for backwards compatibility with existing callers. */
  ingestImage?: (file: File) => Promise<{ hash: string; mime: string }>
  onMediaError?: (message: string) => void
  onReady?: (handle: EditorHandle) => void
  onFocus?: () => void
}) {
  const onChangeRef = useRef(onChange)
  const onReadyRef = useRef(onReady)
  const onFocusRef = useRef(onFocus)
  const onMediaErrorRef = useRef(onMediaError)
  useEffect(() => {
    onChangeRef.current = onChange
    onReadyRef.current = onReady
    onFocusRef.current = onFocus
    onMediaErrorRef.current = onMediaError
  })

  const fileInputRef = useRef<HTMLInputElement>(null)
  const audioInputRef = useRef<HTMLInputElement>(null)
  const insertImageRef = useRef<(file: File) => void>(() => {})
  const insertAudioRef = useRef<(file: File) => void>(() => {})

  const readEditorMarkdown = (ed: Editor): string => {
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const raw = (ed.storage as any).markdown.getMarkdown() as string
    return unescapeMathFromEditorSave(raw)
  }

  const editor = useEditor({
    extensions: createEditorExtensions(placeholder, resolveAsset),
    content: protectMathForEditorLoad(value),
    autofocus: autoFocus ? 'end' : false,
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
    onUpdate: ({ editor: ed }) => onChangeRef.current(readEditorMarkdown(ed)),
  })

  const ingest = ingestAsset ?? ingestImage

  insertImageRef.current = (file: File) => {
    if (!editor || !ingest || !file.type.startsWith('image/')) return
    if (file.size > MAX_MEDIA_UPLOAD_BYTES) {
      onMediaErrorRef.current?.('Image exceeds the 25 MB upload limit.')
      return
    }
    void ingest(file)
      .then(({ hash }) => {
        editor.chain().focus().setImage({ src: `asset:${hash}` }).run()
      })
      .catch((err) => {
        onMediaErrorRef.current?.(
          err instanceof Error ? err.message : 'Could not attach image.',
        )
      })
  }

  insertAudioRef.current = (file: File) => {
    if (!editor || !ingest) return
    const audioMime = resolveAudioMime(file)
    if (!audioMime) {
      onMediaErrorRef.current?.('Unsupported audio file format.')
      return
    }
    if (file.size > MAX_MEDIA_UPLOAD_BYTES) {
      onMediaErrorRef.current?.('Audio file exceeds the 25 MB upload limit.')
      return
    }
    const upload = ingestAsset ? ingestAsset(file, audioMime) : ingest(file)
    void upload
      .then(({ hash }) => {
        const label = file.name.replace(/[\[\]()]/g, '').trim() || 'Audio'
        editor
          .chain()
          .focus()
          .insertContent([
            {
              type: 'text',
              text: `Audio: ${label}`,
              marks: [{ type: 'link', attrs: { href: `asset:${hash}` } }],
            },
            { type: 'text', text: ' ' },
          ])
          .run()
      })
      .catch((err) => {
        onMediaErrorRef.current?.(
          err instanceof Error ? err.message : 'Could not attach audio.',
        )
      })
  }

  // Hand the live editor (plus its media pickers) up to the shared toolbar, and
  // report focus so the toolbar retargets to the field the user is editing.
  useEffect(() => {
    if (!editor) return
    const openImagePicker = () => fileInputRef.current?.click()
    const openAudioPicker = () => audioInputRef.current?.click()
    onReadyRef.current?.({ editor, openImagePicker, openAudioPicker })
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
    if (value !== readEditorMarkdown(editor)) {
      editor.commands.setContent(protectMathForEditorLoad(value), { emitUpdate: false })
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
      <input
        ref={audioInputRef}
        type="file"
        accept="audio/*,.mp3,.m4a,.aac,.ogg,.oga,.opus,.wav,.flac,.webm"
        style={{ display: 'none' }}
        onChange={(e) => {
          const file = e.target.files?.[0]
          if (file) insertAudioRef.current(file)
          e.target.value = ''
        }}
      />
      <EditorContent editor={editor} />
    </div>
  )
}
