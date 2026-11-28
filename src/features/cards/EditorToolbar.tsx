import { useEffect, useReducer, useRef, useState, type ReactNode } from 'react'
import type { Editor } from '@tiptap/react'
import { parseAssetSrc, buildAssetSrc, type ImageAlign } from './imageSrc'

/** Block types offered by the heading dropdown. */
const BLOCKS = [
  { id: 'p', label: 'Normal text', short: 'Normal', hint: '' },
  { id: 'h1', label: 'Heading 1', short: 'H1', hint: '#' },
  { id: 'h2', label: 'Heading 2', short: 'H2', hint: '##' },
  { id: 'h3', label: 'Heading 3', short: 'H3', hint: '###' },
] as const
type BlockId = (typeof BLOCKS)[number]['id']

function svg(children: ReactNode) {
  return (
    <svg width="17" height="17" viewBox="0 0 24 24" fill="none" stroke="currentColor"
      strokeWidth="1.9" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
      {children}
    </svg>
  )
}

const Icon = {
  code: svg(<><path d="M9 8l-3.5 4 3.5 4" /><path d="M15 8l3.5 4-3.5 4" /></>),
  codeBlock: svg(<><rect x="3" y="5" width="18" height="14" rx="2" /><path d="M8 10l-2 2 2 2" /><path d="M16 10l2 2-2 2" /></>),
  bullet: svg(<><circle cx="5" cy="6.5" r="1.2" fill="currentColor" stroke="none" /><circle cx="5" cy="12" r="1.2" fill="currentColor" stroke="none" /><circle cx="5" cy="17.5" r="1.2" fill="currentColor" stroke="none" /><path d="M10 6.5h9" /><path d="M10 12h9" /><path d="M10 17.5h9" /></>),
  ordered: svg(<><path d="M10 6.5h9" /><path d="M10 12h9" /><path d="M10 17.5h9" /><text x="2.5" y="8.8" fontSize="6.5" fill="currentColor" stroke="none" fontFamily="monospace">1</text><text x="2.5" y="14.3" fontSize="6.5" fill="currentColor" stroke="none" fontFamily="monospace">2</text></>),
  quote: svg(<><path d="M5.5 7v10" /><path d="M10 9h9" /><path d="M10 14h6" /></>),
  link: svg(<><path d="M9.5 13.5a3.5 3.5 0 0 1 0-5l2-2a3.5 3.5 0 0 1 5 5l-1 1" /><path d="M14.5 10.5a3.5 3.5 0 0 1 0 5l-2 2a3.5 3.5 0 0 1-5-5l1-1" /></>),
  image: svg(<><rect x="3" y="5" width="18" height="14" rx="2" /><circle cx="8.5" cy="10" r="1.5" /><path d="M21 16l-5-5L6 19" /></>),
  audio: svg(<><path d="M9 18V5l12-2v13" /><circle cx="6" cy="18" r="3" /><circle cx="18" cy="16" r="3" /></>),
  cloze: svg(<><path d="M7 6H4v12h3" /><path d="M17 6h3v12h-3" /><circle cx="9.5" cy="12" r="1" fill="currentColor" stroke="none" /><circle cx="12" cy="12" r="1" fill="currentColor" stroke="none" /><circle cx="14.5" cy="12" r="1" fill="currentColor" stroke="none" /></>),
  occlusion: svg(<><rect x="3" y="4" width="18" height="16" rx="2" /><rect x="6.5" y="7.5" width="6" height="4.5" rx="1" fill="currentColor" stroke="none" /><rect x="13" y="13" width="5" height="3.5" rx="1" strokeDasharray="2 1.5" /></>),
  alignLeft: svg(<><path d="M4 7h16" /><path d="M4 12h10" /><path d="M4 17h13" /></>),
  alignCenter: svg(<><path d="M4 7h16" /><path d="M7 12h10" /><path d="M5 17h14" /></>),
  alignRight: svg(<><path d="M4 7h16" /><path d="M10 12h10" /><path d="M7 17h13" /></>),
}

/** A toolbar button that keeps the editor selection (mousedown preventDefault). */
function Tool({ label, active, disabled, onRun, children }: {
  label: string
  active?: boolean
  disabled?: boolean
  onRun: () => void
  children: ReactNode
}) {
  return (
    <button
      type="button"
      className={`md-tool${active ? ' is-active' : ''}`}
      aria-label={label}
      aria-pressed={active}
      title={label}
      disabled={disabled}
      onMouseDown={(e) => e.preventDefault()}
      onClick={onRun}
    >
      {children}
    </button>
  )
}

function HeadingMenu({ editor }: { editor: Editor | null }) {
  const [open, setOpen] = useState(false)
  const [pos, setPos] = useState<{ top: number; left: number } | null>(null)
  const ref = useRef<HTMLDivElement>(null)

  useEffect(() => {
    if (!open) return
    const onDown = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) setOpen(false)
    }
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        e.stopPropagation()
        setOpen(false)
        editor?.commands.focus()
      }
    }
    const onViewportChange = () => setOpen(false)
    document.addEventListener('mousedown', onDown)
    document.addEventListener('keydown', onKey)
    window.addEventListener('resize', onViewportChange)
    window.addEventListener('scroll', onViewportChange, true)
    return () => {
      document.removeEventListener('mousedown', onDown)
      document.removeEventListener('keydown', onKey)
      window.removeEventListener('resize', onViewportChange)
      window.removeEventListener('scroll', onViewportChange, true)
    }
  }, [open, editor])

  const current: BlockId = !editor
    ? 'p'
    : editor.isActive('heading', { level: 1 })
      ? 'h1'
      : editor.isActive('heading', { level: 2 })
        ? 'h2'
        : editor.isActive('heading', { level: 3 })
          ? 'h3'
          : 'p'

  function toggleMenu() {
    if (!open && ref.current) {
      const rect = ref.current.getBoundingClientRect()
      setPos({
        top: rect.bottom + 6,
        left: Math.max(12, Math.min(rect.left, window.innerWidth - 200)),
      })
    }
    setOpen((v) => !v)
  }

  function apply(id: BlockId) {
    setOpen(false)
    if (!editor) return
    const chain = editor.chain().focus()
    if (id === 'p') chain.setParagraph().run()
    else chain.setHeading({ level: Number(id[1]) as 1 | 2 | 3 }).run()
  }

  const label = BLOCKS.find((b) => b.id === current)?.short ?? 'Normal'

  return (
    <div className="md-head" ref={ref}>
      <button
        type="button"
        className="md-head-trigger"
        aria-haspopup="listbox"
        aria-expanded={open}
        aria-label="Text style"
        disabled={!editor}
        onMouseDown={(e) => e.preventDefault()}
        onClick={toggleMenu}
      >
        <span>{label}</span>
        <svg width="10" height="10" viewBox="0 0 12 12" aria-hidden="true">
          <path d="M3 4.5 6 7.5 9 4.5" fill="none" stroke="currentColor" strokeWidth="1.5"
            strokeLinecap="round" strokeLinejoin="round" />
        </svg>
      </button>
      {open && (
        <div
          className="md-head-menu"
          role="listbox"
          style={pos ? { top: pos.top, left: pos.left } : undefined}
        >
          {BLOCKS.map((b) => (
            <button
              key={b.id}
              type="button"
              role="option"
              aria-selected={current === b.id}
              className={`md-head-opt md-head-opt--${b.id}${current === b.id ? ' is-active' : ''}`}
              onMouseDown={(e) => e.preventDefault()}
              onClick={() => apply(b.id)}
            >
              <span>{b.label}</span>
              {b.hint && <span className="md-head-hint">{b.hint}</span>}
            </button>
          ))}
        </div>
      )}
    </div>
  )
}

function LinkMenu({ editor }: { editor: Editor | null }) {
  const [open, setOpen] = useState(false)
  const [url, setUrl] = useState('')
  const [pos, setPos] = useState<{ top: number; left: number } | null>(null)
  const ref = useRef<HTMLDivElement>(null)
  const inputRef = useRef<HTMLInputElement>(null)
  const active = !!editor?.isActive('link')

  useEffect(() => {
    if (!open) return
    inputRef.current?.focus()
    inputRef.current?.select()
    const onDown = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) setOpen(false)
    }
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        e.stopPropagation()
        setOpen(false)
        editor?.commands.focus()
      }
    }
    document.addEventListener('mousedown', onDown)
    document.addEventListener('keydown', onKey)
    return () => {
      document.removeEventListener('mousedown', onDown)
      document.removeEventListener('keydown', onKey)
    }
  }, [open, editor])

  function openPopover() {
    if (!editor) return
    const existing = (editor.getAttributes('link').href as string | undefined) ?? ''
    setUrl(existing)
    if (ref.current) {
      const rect = ref.current.getBoundingClientRect()
      setPos({
        top: rect.bottom + 6,
        left: Math.max(12, Math.min(rect.left, window.innerWidth - 330)),
      })
    }
    setOpen((v) => !v)
  }

  function apply() {
    if (!editor) return
    const trimmed = url.trim()
    setOpen(false)
    if (!trimmed) {
      editor.chain().focus().unsetLink().run()
      return
    }
    editor.chain().focus().extendMarkRange('link').setLink({ href: trimmed }).run()
  }

  function remove() {
    setOpen(false)
    editor?.chain().focus().unsetLink().run()
  }

  return (
    <div className="md-head" ref={ref}>
      <Tool label="Link" active={active} disabled={!editor} onRun={openPopover}>
        {Icon.link}
      </Tool>
      {open && (
        <div
          className="md-link-pop"
          role="dialog"
          aria-label="Edit link"
          style={pos ? { top: pos.top, left: pos.left } : undefined}
          onKeyDown={(e) => {
            if (e.key === 'Enter') {
              e.preventDefault()
              apply()
            }
          }}
        >
          <input
            ref={inputRef}
            type="url"
            className="text-input md-link-input"
            placeholder="https://…"
            aria-label="Link URL"
            value={url}
            onChange={(e) => setUrl(e.target.value)}
          />
          <button type="button" className="btn btn-primary btn-sm" onClick={apply}>
            Apply
          </button>
          {active && (
            <button type="button" className="btn btn-ghost btn-sm" onClick={remove}>
              Remove
            </button>
          )}
        </div>
      )}
    </div>
  )
}

/**
 * The single formatting toolbar shared by both card fields. It acts on whichever
 * editor is currently focused (passed in as `editor`) and re-renders on that
 * editor's selection/transaction changes so its active states stay accurate.
 */
export function EditorToolbar({
  editor,
  onImage,
  onAudio,
  onCloze,
  onImageOcclusion,
  occlusionActive,
}: {
  editor: Editor | null
  onImage?: () => void
  onAudio?: () => void
  onCloze?: () => void
  onImageOcclusion?: () => void
  occlusionActive?: boolean
}) {
  const [, force] = useReducer((n: number) => n + 1, 0)
  useEffect(() => {
    if (!editor) return
    const update = () => force()
    editor.on('transaction', update)
    editor.on('selectionUpdate', update)
    editor.on('focus', update)
    editor.on('blur', update)
    return () => {
      editor.off('transaction', update)
      editor.off('selectionUpdate', update)
      editor.off('focus', update)
      editor.off('blur', update)
    }
  }, [editor])

  const run = (fn: (chain: ReturnType<Editor['chain']>) => ReturnType<Editor['chain']>) => () => {
    if (!editor) return
    fn(editor.chain().focus()).run()
  }

  const imageActive = !!editor?.isActive('image')
  const imageAlign: ImageAlign =
    parseAssetSrc(editor?.getAttributes('image').src)?.align ?? 'center'

  function setAlign(align: ImageAlign) {
    if (!editor) return
    const parsed = parseAssetSrc(editor.getAttributes('image').src)
    if (!parsed) return
    editor.chain().focus().updateAttributes('image', { src: buildAssetSrc(parsed.hash, align) }).run()
  }

  const a = (k: string) => !!editor?.isActive(k)

  return (
    <div className="md-toolbar" role="toolbar" aria-label="Formatting">
      <HeadingMenu editor={editor} />
      <span className="md-tool-sep" />
      <Tool label="Bold" active={a('bold')} onRun={run((c) => c.toggleBold())}>
        <span className="md-glyph md-glyph-bold">B</span>
      </Tool>
      <Tool label="Italic" active={a('italic')} onRun={run((c) => c.toggleItalic())}>
        <span className="md-glyph md-glyph-italic">I</span>
      </Tool>
      <Tool label="Strikethrough" active={a('strike')} onRun={run((c) => c.toggleStrike())}>
        <span className="md-glyph md-glyph-strike">S</span>
      </Tool>
      <Tool label="Inline code" active={a('code')} onRun={run((c) => c.toggleCode())}>
        {Icon.code}
      </Tool>
      <Tool label="Code block" active={a('codeBlock')} onRun={run((c) => c.toggleCodeBlock())}>
        {Icon.codeBlock}
      </Tool>
      <span className="md-tool-sep" />
      <Tool label="Bullet list" active={a('bulletList')} onRun={run((c) => c.toggleBulletList())}>
        {Icon.bullet}
      </Tool>
      <Tool label="Numbered list" active={a('orderedList')} onRun={run((c) => c.toggleOrderedList())}>
        {Icon.ordered}
      </Tool>
      <Tool label="Quote" active={a('blockquote')} onRun={run((c) => c.toggleBlockquote())}>
        {Icon.quote}
      </Tool>
      <span className="md-tool-sep" />
      <LinkMenu editor={editor} />
      {onImage && (
        <Tool label="Image" onRun={onImage}>
          {Icon.image}
        </Tool>
      )}
      {onAudio && (
        <Tool label="Audio clip" onRun={onAudio}>
          {Icon.audio}
        </Tool>
      )}
      {onCloze && (
        <Tool label="Cloze deletion (⌘⇧C)" onRun={onCloze}>
          {Icon.cloze}
        </Tool>
      )}
      {onImageOcclusion && (
        <Tool label="Image occlusion" active={occlusionActive} onRun={onImageOcclusion}>
          {Icon.occlusion}
        </Tool>
      )}
      {imageActive && (
        <>
          <span className="md-tool-sep" />
          <Tool label="Align left" active={imageAlign === 'left'} onRun={() => setAlign('left')}>
            {Icon.alignLeft}
          </Tool>
          <Tool label="Align center" active={imageAlign === 'center'} onRun={() => setAlign('center')}>
            {Icon.alignCenter}
          </Tool>
          <Tool label="Align right" active={imageAlign === 'right'} onRun={() => setAlign('right')}>
            {Icon.alignRight}
          </Tool>
        </>
      )}
    </div>
  )
}
