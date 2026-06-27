// src/features/cards/CardEditorPage.tsx
import { useEffect, useState } from 'react'
import { useNavigate, useParams } from 'react-router-dom'
import { useStorage } from '../../data/StorageContext'
import { PageHeader } from '../../ui/PageHeader'
import { RichMarkdownEditor } from './RichMarkdownEditor'
import { loadAssetUrl } from './assetUrl'

/** Full-screen create/edit card screen. Route params: deckId, optional cardId. */
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

  const ingestImage = async (file: File) => {
    const bytes = new Uint8Array(await file.arrayBuffer())
    const asset = await storage.putAsset(bytes, file.type)
    return { hash: asset.hash, mime: asset.mime }
  }
  const resolveAsset = (hash: string) => loadAssetUrl(storage, hash)

  function back2deck() {
    navigate(`/decks/${deckId}`)
  }

  async function save() {
    if (!front.trim() || !deckId) return
    if (editing && cardId) await storage.updateCard(cardId, { front, back })
    else await storage.createCard(deckId, front, back)
    await storage.sweepOrphanAssets()
    back2deck()
  }

  async function remove() {
    if (!cardId) return
    await storage.deleteCard(cardId)
    await storage.sweepOrphanAssets()
    back2deck()
  }

  const actions = (
    <>
      <button className="btn btn-primary" onClick={save} disabled={!front.trim()}>
        Save
      </button>
      <button className="btn btn-ghost" onClick={back2deck}>
        Cancel
      </button>
      {editing && (
        <button className="btn btn-ghost btn-danger btn-delete" onClick={remove}>
          Delete
        </button>
      )}
    </>
  )

  return (
    <>
      <PageHeader title={editing ? 'Edit card' : 'New card'} actions={actions} />
      <div className="page-body card-editor">
        <div className="editor-field">
          <label className="field-label">Front</label>
          <RichMarkdownEditor
            value={front}
            onChange={setFront}
            placeholder="Front (markdown)…"
            ariaLabel="Front"
            resolveAsset={resolveAsset}
            ingestImage={ingestImage}
          />
        </div>
        <div className="editor-field">
          <label className="field-label">Back</label>
          <RichMarkdownEditor
            value={back}
            onChange={setBack}
            placeholder="Back (markdown)…"
            ariaLabel="Back"
            resolveAsset={resolveAsset}
            ingestImage={ingestImage}
          />
        </div>
      </div>
    </>
  )
}
