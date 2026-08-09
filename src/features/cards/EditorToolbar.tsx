import { useEffect, useReducer, useRef, useState, type ReactNode } from 'react'
import type { Editor } from '@tiptap/react'
import { parseAssetSrc, buildAssetSrc, type ImageAlign } from './imageSrc'

/** Block types offered by the heading dropdown. */
const BLOCKS = [
  { id: 'p', label: 'Normal text', short: 'Normal' },
  { id: 'h1', label: 'Heading 1', short: 'H1' },
  { id: 'h2', label: 'Heading 2', short: 'H2' },
  { id: 'h3', label: 'Heading 3', short: 'H3' },
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
  const ref = useRef<HTMLDivElement>(null)

  useEffect(() => {
    if (!open) return
    const onDown = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) setOpen(false)
    }
    const onKey = (e: KeyboardEvent) => e.key === 'Escape' && setOpen(false)
    document.addEventListener('mousedown', onDown)
    document.addEventListener('keydown', onKey)
    return () => {
      document.removeEventListener('mousedown', onDown)
      document.removeEventListener('keydown', onKey)
    }
  }, [open])

  const current: BlockId = !editor
    ? 'p'
    : editor.isActive('heading', { level: 1 })
      ? 'h1'
      : editor.isActive('heading', { level: 2 })
        ? 'h2'
        : editor.isActive('heading', { level: 3 })
          ? 'h3'
          : 'p'

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
        onClick={() => setOpen((v) => !v)}
      >
        <span>{label}</span>
        <svg width="10" height="10" viewBox="0 0 12 12" aria-hidden="true">
          <path d="M3 4.5 6 7.5 9 4.5" fill="none" stroke="currentColor" strokeWidth="1.5"
            strokeLinecap="round" strokeLinejoin="round" />
        </svg>
      </button>
      {open && (
        <div className="md-head-menu" role="listbox">
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
              {b.label}
            </button>
          ))}
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
export function EditorToolbar({ editor, onImage }: { editor: Editor | null; onImage?: () => void }) {
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

  function toggleLink() {
    if (!editor) return
    if (editor.isActive('link')) {
      editor.chain().focus().unsetLink().run()
      return
    }
    const url = window.prompt('Link URL')
    if (url) editor.chain().focus().extendMarkRange('link').setLink({ href: url }).run()
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
      <Tool label="Link" active={a('link')} onRun={toggleLink}>
        {Icon.link}
      </Tool>
      {onImage && (
        <Tool label="Image" onRun={onImage}>
          {Icon.image}
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
