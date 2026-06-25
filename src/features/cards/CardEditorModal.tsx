import { useEffect, useState } from 'react'
import { useStorage } from '../../data/StorageContext'
import { RichMarkdownEditor } from './RichMarkdownEditor'

/**
 * In-page modal for creating or editing a card. Overlays the deck view.
 * `cardId` present = edit (loads the card); absent = new card in `deckId`.
 */
export function CardEditorModal({
  deckId,
  cardId,
  onClose,
}: {
  deckId: string
  cardId?: string
  onClose: () => void
}) {
  const storage = useStorage()
  const editing = Boolean(cardId)

  const [front, setFront] = useState('')
  const [back, setBack] = useState('')

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

  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      if (e.key === 'Escape') onClose()
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [onClose])

  async function save() {
    if (!front.trim()) return
    if (editing && cardId) await storage.updateCard(cardId, { front, back })
    else await storage.createCard(deckId, front, back)
    onClose()
  }

  async function remove() {
    if (!cardId) return
    await storage.deleteCard(cardId)
    onClose()
  }

  return (
    <div className="modal-backdrop" onClick={onClose} role="presentation">
      <div className="modal" role="dialog" aria-modal="true" onClick={(e) => e.stopPropagation()}>
        <div className="modal-header">
          <h2>{editing ? 'Edit card' : 'New card'}</h2>
        </div>
        <div className="modal-body">
          <div className="modal-field">
            <label className="field-label">Front</label>
            <RichMarkdownEditor value={front} onChange={setFront} placeholder="Front (markdown)…" ariaLabel="Front" />
          </div>
          <div className="modal-field">
            <label className="field-label">Back</label>
            <RichMarkdownEditor value={back} onChange={setBack} placeholder="Back (markdown)…" ariaLabel="Back" />
          </div>
        </div>
        <div className="modal-footer">
          <button className="btn btn-primary" onClick={save} disabled={!front.trim()}>
            Save
          </button>
          <button className="btn btn-ghost" onClick={onClose}>
            Cancel
          </button>
          {editing && (
            <button className="btn btn-ghost btn-danger btn-delete" onClick={remove}>
              Delete
            </button>
          )}
        </div>
      </div>
    </div>
  )
}
