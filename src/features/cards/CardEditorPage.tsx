// src/features/cards/CardEditorPage.tsx
import { useCallback, useEffect, useRef, useState, type KeyboardEvent as ReactKeyboardEvent } from 'react'
import { useBlocker, useLocation, useNavigate, useParams } from 'react-router-dom'
import { useStorage } from '../../data/StorageContext'
import type { Deck } from '../../domain/models'
import { ConfirmDialog } from '../../ui/ConfirmDialog'
import { PageHeader } from '../../ui/PageHeader'
import { RichMarkdownEditor, type EditorHandle } from './RichMarkdownEditor'
import { EditorToolbar } from './EditorToolbar'
import { loadAssetUrl } from './assetUrl'
import { isSystemTag, mergeUserTags, parseUserTags, userTags } from './cardTags'
import { extractClozeIndices, nextClozeIndex } from './cloze'
import { cardStatus } from './DeckDetailPage'

interface CardDraft {
  front: string
  back: string
  tagsKey: string
}

const EMPTY_DRAFT: CardDraft = { front: '', back: '', tagsKey: '' }

/** Full-screen create/edit card screen. Route params: deckId, optional cardId. */
export function CardEditorPage() {
  const { deckId, cardId } = useParams()
  const storage = useStorage()
  const navigate = useNavigate()
  const location = useLocation()
  const editing = Boolean(cardId)
  const returnTo = (location.state as { returnTo?: string } | null)?.returnTo

  const [deck, setDeck] = useState<Deck>()
  const [deckTagPool, setDeckTagPool] = useState<string[]>([])
  const [front, setFront] = useState('')
  const [back, setBack] = useState('')
  const [tags, setTags] = useState<string[]>([])
  const [userTagChips, setUserTagChips] = useState<string[]>([])
  const [tagDraft, setTagDraft] = useState('')
  const [suspended, setSuspended] = useState(false)
  const [isClozeCard, setIsClozeCard] = useState(false)
  const [reviewMeta, setReviewMeta] = useState<{ reps: number; nextLabel: string } | null>(null)
  const [baseline, setBaseline] = useState<CardDraft | null>(editing ? null : EMPTY_DRAFT)
  const [confirmDelete, setConfirmDelete] = useState(false)
  const [savedCount, setSavedCount] = useState(0)
  const allowNavigationRef = useRef(false)
  const tagInputRef = useRef<HTMLInputElement>(null)
  // The field the shared toolbar should act on — defaults to Front, follows focus.
  const [active, setActive] = useState<EditorHandle | null>(null)
  const frontHandle = useRef<EditorHandle | null>(null)
  const backHandle = useRef<EditorHandle | null>(null)

  useEffect(() => {
    if (!deckId) return
    let live = true
    Promise.all([storage.getDeck(deckId), storage.listCards(deckId)]).then(([d, deckCards]) => {
      if (!live) return
      if (d) setDeck(d)
      const unique = [...new Set(deckCards.flatMap((c) => userTags(c.tags)))].sort((a, b) =>
        a.localeCompare(b),
      )
      setDeckTagPool(unique)
    })
    return () => {
      live = false
    }
  }, [deckId, storage, savedCount])

  const [cardMissing, setCardMissing] = useState(false)

  useEffect(() => {
    allowNavigationRef.current = false
    if (!cardId) {
      setCardMissing(false)
      return
    }
    let live = true
    setCardMissing(false)
    setBaseline(null)
    setFront('')
    setBack('')
    setTags([])
    setUserTagChips([])
    setTagDraft('')
    setSuspended(false)
    setReviewMeta(null)
    storage.getCard(cardId).then((card) => {
      if (!live) return
      if (!card) {
        setCardMissing(true)
        return
      }
      const currentUserTags = userTags(card.tags)
      setFront(card.front)
      setBack(card.back)
      setTags(card.tags)
      setUserTagChips(currentUserTags)
      setTagDraft('')
      setSuspended(card.suspended)
      setIsClozeCard(Boolean(card.clozeIndex))
      if (card.scheduling.reps > 0) {
        const st = cardStatus(card, Date.now())
        setReviewMeta({
          reps: card.scheduling.reps,
          nextLabel: st.kind === 'scheduled' ? `Next review in ${st.label}` : `Status: ${st.label}`,
        })
      } else {
        setReviewMeta(null)
      }
      setBaseline({
        front: card.front,
        back: card.back,
        tagsKey: currentUserTags.join(', '),
      })
    })
    return () => {
      live = false
    }
  }, [cardId, storage])

  const combinedTagInput = [...userTagChips, tagDraft].filter(Boolean).join(', ')
  const currentTagsKey = parseUserTags(combinedTagInput).join(', ')

  const ingestImage = async (file: File) => {
    const bytes = new Uint8Array(await file.arrayBuffer())
    const asset = await storage.putAsset(bytes, file.type)
    return { hash: asset.hash, mime: asset.mime }
  }
  const resolveAsset = (hash: string) => loadAssetUrl(storage, hash)
  const dirty = !cardMissing && baseline !== null && (
    front !== baseline.front || back !== baseline.back || currentTagsKey !== baseline.tagsKey
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

  function leaveEditor() {
    navigate(returnTo ?? `/decks/${deckId}`)
  }

  function commitTagDraft(raw = tagDraft) {
    if (!raw.trim()) {
      setTagDraft('')
      return
    }
    setUserTagChips((prev) => parseUserTags([...prev, raw].join(', ')))
    setTagDraft('')
  }

  function addSuggestedTag(tag: string) {
    setUserTagChips((prev) => parseUserTags([...prev, tag].join(', ')))
    tagInputRef.current?.focus()
  }

  function removeTagChip(tag: string) {
    setUserTagChips((prev) => prev.filter((item) => item !== tag))
  }

  function onTagKeyDown(event: ReactKeyboardEvent<HTMLInputElement>) {
    if (event.key === 'Enter' || event.key === ',') {
      event.preventDefault()
      commitTagDraft()
    } else if (event.key === 'Backspace' && tagDraft === '' && userTagChips.length > 0) {
      setUserTagChips((prev) => prev.slice(0, -1))
    }
  }

  const insertCloze = useCallback(() => {
    const target = frontHandle.current ?? active
    if (!target) return
    const ed = target.editor
    const { from, to } = ed.state.selection
    const selectedText = ed.state.doc.textBetween(from, to, ' ')
    const idx = nextClozeIndex(front)
    const snippet = selectedText ? `{{c${idx}::${selectedText}}}` : `{{c${idx}::}}`
    ed.chain().focus().insertContent(snippet).run()
  }, [active, front])

  const save = useCallback(async (addAnother = false) => {
    if (!front.trim() || !deckId) return
    const nextTags = mergeUserTags(tags, combinedTagInput)
    let createdCount = 1
    if (editing && cardId) {
      if (isClozeCard && storage.updateClozeNote) {
        await storage.updateClozeNote(cardId, front, back, nextTags)
      } else {
        await storage.updateCard(cardId, { front, back, tags: nextTags })
      }
    } else if (storage.createCardOrCloze) {
      const created = await storage.createCardOrCloze(deckId, front, back, nextTags)
      createdCount = Math.max(1, created.length)
    } else {
      await storage.createCard(deckId, front, back, nextTags)
    }
    await storage.sweepOrphanAssets()

    if (addAnother && !editing) {
      setFront('')
      setBack('')
      setTags([])
      setUserTagChips([])
      setTagDraft('')
      setBaseline(EMPTY_DRAFT)
      setSavedCount((n) => n + createdCount)
      frontHandle.current?.editor.commands.focus('end')
      return
    }

    allowNavigationRef.current = true
    leaveEditor()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [front, back, tags, combinedTagInput, deckId, cardId, editing, isClozeCard, storage, returnTo])

  async function remove() {
    if (!cardId) return
    await storage.deleteCard(cardId)
    await storage.sweepOrphanAssets()
    allowNavigationRef.current = true
    leaveEditor()
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

  // ⌘⏎ / Ctrl+⏎ saves, ⌘⇧⏎ saves & starts another new card, Esc cancels.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.defaultPrevented) return
      if ((e.metaKey || e.ctrlKey) && e.key === 'Enter') {
        if (cardMissing) return
        e.preventDefault()
        void save(e.shiftKey && !editing)
      } else if ((e.metaKey || e.ctrlKey) && e.shiftKey && e.key.toLowerCase() === 'c') {
        if (cardMissing) return
        e.preventDefault()
        insertCloze()
      } else if (e.key === 'Escape') {
        e.preventDefault()
        leaveEditor()
      }
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [save, insertCloze, editing, cardMissing])

  const clozeIndices = extractClozeIndices(front)

  const systemTags = tags.filter(isSystemTag)
  const existingKeys = new Set(userTagChips.map((t) => t.toLocaleLowerCase()))
  const draftNeedle = tagDraft.trim().toLocaleLowerCase()
  const suggestedTags = deckTagPool
    .filter((tag) => {
      const key = tag.toLocaleLowerCase()
      if (existingKeys.has(key)) return false
      return !draftNeedle || key.includes(draftNeedle)
    })
    .slice(0, 6)

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
      <button className="btn btn-ghost" onClick={leaveEditor}>
        Cancel
      </button>
      {!editing && (
        <button className="btn btn-ghost" onClick={() => void save(true)} disabled={!front.trim()}>
          Save &amp; add another
        </button>
      )}
      <button className="btn btn-primary" onClick={() => void save(false)} disabled={!front.trim()}>
        Save card
      </button>
    </>
  )

  if (cardMissing) {
    return (
      <>
        <PageHeader
          title={title}
          actions={
            <button className="btn btn-ghost" onClick={leaveEditor}>
              Back
            </button>
          }
        />
        <div className="empty-state">
          <h3>Card not found</h3>
          <p>This card was deleted or is no longer available.</p>
          <button className="btn btn-primary cta" onClick={leaveEditor}>
            Return
          </button>
        </div>
      </>
    )
  }

  return (
    <>
      <PageHeader title={title} actions={actions} />
      <div className="card-editor">
        {savedCount > 0 && (
          <p className="editor-saved-notice" role="status">
            {savedCount === 1 ? 'Card saved.' : `${savedCount} cards saved.`} Ready for the next one.
          </p>
        )}

        <div className="md-toolbar-bar">
          <EditorToolbar
            editor={active?.editor ?? null}
            onImage={() => active?.openImagePicker()}
            onAudio={() => active?.openAudioPicker()}
            onCloze={insertCloze}
          />
        </div>

        {((!editing && clozeIndices.length > 0) || isClozeCard) && (
          <p className="editor-cloze-hint" role="status">
            {editing && isClozeCard
              ? `Cloze note (${clozeIndices.map((n) => `c${n}`).join(', ') || 'cloze'}) · edits update sibling cards.`
              : `Cloze note · creates ${clozeIndices.length} ${clozeIndices.length === 1 ? 'card' : 'cards'} (${clozeIndices.map((n) => `c${n}`).join(', ')}).`}
          </p>
        )}

        <section className="card-editor-surface" aria-label="Card content">
          <div className="editor-field editor-field--front">
            <div className="editor-field-label">Front</div>
            <RichMarkdownEditor
              value={front}
              onChange={setFront}
              placeholder="Type the prompt…"
              ariaLabel="Front"
              autoFocus={!editing}
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
          <div
            className="tag-chip-box"
            onClick={() => tagInputRef.current?.focus()}
          >
            {userTagChips.map((tag) => (
              <span key={tag} className="tag-chip">
                {tag}
                <button
                  type="button"
                  className="tag-chip-remove"
                  aria-label={`Remove tag ${tag}`}
                  onClick={(event) => {
                    event.stopPropagation()
                    removeTagChip(tag)
                  }}
                >
                  ×
                </button>
              </span>
            ))}
            <input
              ref={tagInputRef}
              id="card-tags"
              className="tag-chip-input"
              value={tagDraft}
              onChange={(event) => setTagDraft(event.target.value)}
              onKeyDown={onTagKeyDown}
              onBlur={() => commitTagDraft()}
              placeholder={userTagChips.length === 0 ? 'Add tags (press Enter or comma)…' : 'Add another…'}
            />
          </div>
          {suggestedTags.length > 0 && (
            <div className="tag-suggestions" aria-label="Suggested deck tags">
              <span className="tag-suggestions-label">Deck tags:</span>
              {suggestedTags.map((tag) => (
                <button
                  key={tag}
                  type="button"
                  className="tag-suggest-btn"
                  onMouseDown={(event) => event.preventDefault()}
                  onClick={() => addSuggestedTag(tag)}
                >
                  + {tag}
                </button>
              ))}
            </div>
          )}
          <p className="editor-tag-hint">Press Enter or comma to add a tag. The leech tag is managed automatically.</p>
        </div>

        <div className="editor-foot">
          <span className="editor-hint">
            {editing ? '⌘⏎ Save · Esc Cancel' : '⌘⏎ Save · ⌘⇧⏎ Save & add another · Esc Cancel'}
          </span>
          {editing && reviewMeta && (
            <span className="editor-review-meta">
              {reviewMeta.nextLabel} · {reviewMeta.reps} {reviewMeta.reps === 1 ? 'review' : 'reviews'}
            </span>
          )}
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
