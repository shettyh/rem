import { useEffect, useState } from 'react'
import { useNavigate, useParams } from 'react-router-dom'
import { useStorage } from '../../data/StorageContext'
import { Sheet } from '../../ui/Sheet'
import { RichMarkdownEditor } from './RichMarkdownEditor'

export function CardEditorPage() {
  const { deckId, cardId } = useParams()
  const storage = useStorage()
  const navigate = useNavigate()
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

  const backToDeck = () => navigate(`/decks/${deckId}`)

  async function save() {
    if (!deckId || !front.trim()) return
    if (editing && cardId) {
      await storage.updateCard(cardId, { front, back })
    } else {
      await storage.createCard(deckId, front, back)
    }
    backToDeck()
  }

  async function remove() {
    if (!cardId) return
    await storage.deleteCard(cardId)
    backToDeck()
  }

  const footer = (
    <>
      <div className="row">
        <button className="btn btn-primary" onClick={save} disabled={!front.trim()}>
          {editing ? 'Save' : 'Add card'}
        </button>
        <button className="btn btn-ghost" onClick={backToDeck}>
          Cancel
        </button>
      </div>
      {editing && (
        <button className="btn btn-ghost btn-danger" onClick={remove}>
          Delete
        </button>
      )}
    </>
  )

  return (
    <Sheet title={editing ? 'Edit card' : 'New card'} onClose={backToDeck} footer={footer}>
      <CardField label="Front" value={front} onChange={setFront} />
      <CardField label="Back" value={back} onChange={setBack} />
    </Sheet>
  )
}

function CardField({
  label,
  value,
  onChange,
}: {
  label: string
  value: string
  onChange: (value: string) => void
}) {
  return (
    <div className="field">
      <label className="field-label">{label}</label>
      <RichMarkdownEditor
        value={value}
        onChange={onChange}
        placeholder={`${label} (markdown)…`}
        ariaLabel={label}
      />
    </div>
  )
}
