// src/features/cards/CardEditorPage.tsx
import { useCallback, useEffect, useRef, useState } from 'react'
import { useNavigate, useParams } from 'react-router-dom'
import { useStorage } from '../../data/StorageContext'
import type { Deck } from '../../domain/models'
import { PageHeader } from '../../ui/PageHeader'
import { deckColor } from '../../ui/deckColor'
import { RichMarkdownEditor, type EditorHandle } from './RichMarkdownEditor'
import { EditorToolbar } from './EditorToolbar'
import { loadAssetUrl } from './assetUrl'

/** Full-screen create/edit card screen. Route params: deckId, optional cardId. */
export function CardEditorPage() {
  const { deckId, cardId } = useParams()
  const storage = useStorage()
  const navigate = useNavigate()
  const editing = Boolean(cardId)

  const [deck, setDeck] = useState<Deck>()
  const [front, setFront] = useState('')
  const [back, setBack] = useState('')
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
        setFront(card.front)
        setBack(card.back)
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
    if (editing && cardId) await storage.updateCard(cardId, { front, back })
    else await storage.createCard(deckId, front, back)
    await storage.sweepOrphanAssets()
    back2deck()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [front, back, deckId, cardId, editing, storage])

  async function remove() {
    if (!cardId) return
    await storage.deleteCard(cardId)
    await storage.sweepOrphanAssets()
    back2deck()
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

  const title = (
    <>
      <span className="header-title-text">{editing ? 'Edit card' : 'New card'}</span>
      {deck && (
        <span className="header-deck-chip">
          <span className="header-deck-dot" style={{ background: deckColor(deck.id) }} />
          {deck.name}
        </span>
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

        <div className="editor-field editor-field--front">
          <div className="field-rule">
            <span className="field-rule-label">Front</span>
            <span className="field-rule-line" />
          </div>
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
          <div className="field-rule">
            <span className="field-rule-label">Back</span>
            <span className="field-rule-line" />
          </div>
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

        <div className="editor-foot">
          <span className="editor-hint">⌘⏎ to save · esc to cancel · what you type is the card</span>
          {editing && (
            <button className="btn btn-ghost btn-danger" onClick={remove}>
              Delete card
            </button>
          )}
        </div>
      </div>
    </>
  )
}
