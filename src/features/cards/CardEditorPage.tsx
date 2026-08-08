// src/features/cards/CardEditorPage.tsx
import { useCallback, useEffect, useRef, useState } from 'react'
import { useBlocker, useNavigate, useParams } from 'react-router-dom'
import { useStorage } from '../../data/StorageContext'
import type { Deck } from '../../domain/models'
import { ConfirmDialog } from '../../ui/ConfirmDialog'
import { PageHeader } from '../../ui/PageHeader'
import { RichMarkdownEditor, type EditorHandle } from './RichMarkdownEditor'
import { EditorToolbar } from './EditorToolbar'
import { loadAssetUrl } from './assetUrl'
import { isSystemTag, mergeUserTags, userTags } from './cardTags'

interface CardDraft {
  front: string
  back: string
  tagInput: string
}

const EMPTY_DRAFT: CardDraft = { front: '', back: '', tagInput: '' }

/** Full-screen create/edit card screen. Route params: deckId, optional cardId. */
export function CardEditorPage() {
  const { deckId, cardId } = useParams()
  const storage = useStorage()
  const navigate = useNavigate()
  const editing = Boolean(cardId)

  const [deck, setDeck] = useState<Deck>()
  const [front, setFront] = useState('')
  const [back, setBack] = useState('')
  const [tags, setTags] = useState<string[]>([])
  const [tagInput, setTagInput] = useState('')
  const [suspended, setSuspended] = useState(false)
  const [baseline, setBaseline] = useState<CardDraft | null>(editing ? null : EMPTY_DRAFT)
  const [confirmDelete, setConfirmDelete] = useState(false)
  const allowNavigationRef = useRef(false)
  // The field the shared toolbar should act on — defaults to Front, follows focus.
  const [active, setActive] = useState<EditorHandle | null>(null)
  const frontHandle = useRef<EditorHandle | null>(null)
  const backHandle = useRef<EditorHandle | null>(null)

  useEffect(() => {
    if (!deckId) return
    let live = true
    storage.getDeck(deckId).then((d) => live && d && setDeck(d))
    return () => {
      live = false
    }
  }, [deckId, storage])

  useEffect(() => {
    if (!cardId) return
    let active = true
    storage.getCard(cardId).then((card) => {
      if (active && card) {
        const nextTagInput = userTags(card.tags).join(', ')
        setFront(card.front)
        setBack(card.back)
        setTags(card.tags)
        setTagInput(nextTagInput)
        setSuspended(card.suspended)
        setBaseline({ front: card.front, back: card.back, tagInput: nextTagInput })
      }
    })
    return () => {
      active = false
    }
  }, [cardId, storage])

  const ingestImage = async (file: File) => {
    const bytes = new Uint8Array(await file.arrayBuffer())
    const asset = await storage.putAsset(bytes, file.type)
    return { hash: asset.hash, mime: asset.mime }
  }
  const resolveAsset = (hash: string) => loadAssetUrl(storage, hash)
  const dirty = baseline !== null && (
    front !== baseline.front || back !== baseline.back || tagInput !== baseline.tagInput
  )
  const blocker = useBlocker(({ currentLocation, nextLocation }) =>
    !allowNavigationRef.current &&
    dirty &&
    `${currentLocation.pathname}${currentLocation.search}` !==
      `${nextLocation.pathname}${nextLocation.search}`,
  )

  useEffect(() => {
    function onBeforeUnload(event: BeforeUnloadEvent) {
      if (!dirty || allowNavigationRef.current) return
      event.preventDefault()
      event.returnValue = ''
    }
    window.addEventListener('beforeunload', onBeforeUnload)
    return () => window.removeEventListener('beforeunload', onBeforeUnload)
  }, [dirty])

  const onFrontReady = useCallback((h: EditorHandle) => {
    frontHandle.current = h
    setActive((cur) => cur ?? h)
  }, [])
  const onBackReady = useCallback((h: EditorHandle) => {
    backHandle.current = h
  }, [])

  function back2deck() {
    navigate(`/decks/${deckId}`)
  }

  const save = useCallback(async () => {
    if (!front.trim() || !deckId) return
    const nextTags = mergeUserTags(tags, tagInput)
    if (editing && cardId) await storage.updateCard(cardId, { front, back, tags: nextTags })
    else await storage.createCard(deckId, front, back, nextTags)
    await storage.sweepOrphanAssets()
    allowNavigationRef.current = true
    back2deck()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [front, back, tags, tagInput, deckId, cardId, editing, storage])

  async function remove() {
    if (!cardId) return
    await storage.deleteCard(cardId)
    await storage.sweepOrphanAssets()
    allowNavigationRef.current = true
    back2deck()
  }

  async function discardAndLeave() {
    allowNavigationRef.current = true
    await storage.sweepOrphanAssets()
    if (blocker.state === 'blocked') blocker.proceed()
  }

  async function unsuspend() {
    if (!cardId) return
    await storage.updateCard(cardId, { suspended: false })
    setSuspended(false)
  }

  // ⌘⏎ / Ctrl+⏎ saves, Esc cancels — from anywhere on the screen.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key === 'Enter') {
        e.preventDefault()
        void save()
      } else if (e.key === 'Escape') {
        e.preventDefault()
        back2deck()
      }
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [save])

  const systemTags = tags.filter(isSystemTag)

  const title = (
    <>
      <span className="header-title-text">{editing ? 'Edit card' : 'New card'}</span>
      {deck && (
        <span className="header-deck-chip">{deck.name}</span>
      )}
    </>
  )

  const actions = (
    <>
      <button className="btn btn-ghost" onClick={back2deck}>
        Cancel
      </button>
      <button className="btn btn-primary" onClick={save} disabled={!front.trim()}>
        Save card
      </button>
    </>
  )

  return (
    <>
      <PageHeader title={title} actions={actions} />
      <div className="card-editor">
        <div className="md-toolbar-bar">
          <EditorToolbar editor={active?.editor ?? null} onImage={() => active?.openImagePicker()} />
        </div>

        <section className="card-editor-surface" aria-label="Card content">
          <div className="editor-field editor-field--front">
            <div className="editor-field-label">Front</div>
            <RichMarkdownEditor
              value={front}
              onChange={setFront}
              placeholder="Type the prompt…"
              ariaLabel="Front"
              resolveAsset={resolveAsset}
              ingestImage={ingestImage}
              onReady={onFrontReady}
              onFocus={() => frontHandle.current && setActive(frontHandle.current)}
            />
          </div>

          <div className="editor-field editor-field--back">
            <div className="editor-field-label">Back</div>
            <RichMarkdownEditor
              value={back}
              onChange={setBack}
              placeholder="Type the answer…"
              ariaLabel="Back"
              resolveAsset={resolveAsset}
              ingestImage={ingestImage}
              onReady={onBackReady}
              onFocus={() => backHandle.current && setActive(backHandle.current)}
            />
          </div>
        </section>

        <div className="editor-tag-field">
          <div className="field-rule">
            <label className="field-rule-label" htmlFor="card-tags">Tags</label>
            <span className="field-rule-line" />
          </div>
          <input
            id="card-tags"
            className="text-input"
            value={tagInput}
            onChange={(event) => setTagInput(event.target.value)}
            placeholder="grammar, chapter 1"
          />
          <p className="editor-tag-hint">Comma-separated. The leech tag is managed automatically.</p>
        </div>

        <div className="editor-foot">
          <span className="editor-hint">⌘⏎ Save · Esc Cancel</span>
          {editing && (systemTags.length > 0 || suspended) && (
            <div className="editor-card-state">
              {systemTags.length > 0 && (
                <div className="editor-tags" aria-label="System tags">
                  {systemTags.map((tag) => <span key={tag} className="status-tag status-leech">{tag}</span>)}
                </div>
              )}
              {suspended ? (
                <button className="btn btn-ghost" onClick={() => void unsuspend()}>
                  Unsuspend card
                </button>
              ) : (
                <span className="muted">Active</span>
              )}
            </div>
          )}
          {editing && (
            <button className="btn btn-danger-outline editor-delete" onClick={() => setConfirmDelete(true)}>
              Delete card
            </button>
          )}
        </div>
      </div>

      {blocker.state === 'blocked' && (
        <ConfirmDialog
          title="Discard unsaved changes?"
          description="Your edits to this card have not been saved."
          confirmLabel="Discard changes"
          cancelLabel="Keep editing"
          danger
          onCancel={() => blocker.reset()}
          onConfirm={() => void discardAndLeave()}
        />
      )}

      {confirmDelete && (
        <ConfirmDialog
          title="Delete this card?"
          description="This permanently removes the card and its review history. This cannot be undone."
          confirmLabel="Delete permanently"
          cancelLabel="Keep card"
          danger
          onCancel={() => setConfirmDelete(false)}
          onConfirm={() => void remove()}
        />
      )}
    </>
  )
}
