import { useState, type ChangeEvent } from 'react'
import { useLiveQuery } from 'dexie-react-hooks'
import { useStorage } from '../../data/StorageContext'
import { PageHeader } from '../../ui/PageHeader'
import { SyncSection } from './SyncSection'
import {
  collectBackup,
  serializeBackup,
  parseBackup,
  planImport,
  type DeckBackup,
} from '../../data/backup'

function downloadJson(json: string, filename: string) {
  const url = URL.createObjectURL(new Blob([json], { type: 'application/json' }))
  const a = document.createElement('a')
  a.href = url
  a.download = filename
  a.click()
  URL.revokeObjectURL(url)
}

function todayStamp(): string {
  return new Date().toISOString().slice(0, 10)
}

export function SettingsPage() {
  const storage = useStorage()
  const decks = useLiveQuery(() => storage.listDecks(), [])

  const [selected, setSelected] = useState<Set<string>>(new Set())
  const [pending, setPending] = useState<{ decks: DeckBackup[]; replaced: string[] } | null>(null)
  const [message, setMessage] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)

  const list = decks ?? []
  const allSelected = list.length > 0 && list.every((d) => selected.has(d.id))

  function toggle(id: string) {
    setSelected((prev) => {
      const next = new Set(prev)
      if (next.has(id)) next.delete(id)
      else next.add(id)
      return next
    })
  }

  function toggleAll() {
    setSelected(allSelected ? new Set() : new Set(list.map((d) => d.id)))
  }

  async function onExport() {
    const ids = list.filter((d) => selected.has(d.id)).map((d) => d.id)
    const payload = await collectBackup(storage, ids)
    downloadJson(serializeBackup(payload, Date.now()), `rem-backup-${todayStamp()}.json`)
  }

  async function runImport(toImport: DeckBackup[]) {
    const result = await storage.importDecks(toImport)
    setPending(null)
    const replacedNote = result.replaced.length ? ` (replaced ${result.replaced.length})` : ''
    setMessage(`Imported ${toImport.length} deck${toImport.length === 1 ? '' : 's'}${replacedNote}.`)
  }

  async function onFile(e: ChangeEvent<HTMLInputElement>) {
    const file = e.target.files?.[0]
    e.target.value = '' // allow re-selecting the same file later
    if (!file) return
    setError(null)
    setMessage(null)
    setPending(null)
    let parsed: DeckBackup[]
    try {
      parsed = parseBackup(await file.text())
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Could not read the file.')
      return
    }
    const { replaced } = planImport(
      parsed.map((d) => d.name),
      list.map((d) => d.name),
    )
    if (replaced.length > 0) setPending({ decks: parsed, replaced })
    else await runImport(parsed)
  }

  return (
    <>
      <PageHeader title="Settings" />
      <div className="page-body measure stack">
        <SyncSection />

      <section className="settings-section">
        <h2>Export decks</h2>
        {list.length === 0 ? (
          <p className="settings-hint">No decks to export yet.</p>
        ) : (
          <>
            <label className="settings-check">
              <input
                type="checkbox"
                aria-label="Select all decks"
                checked={allSelected}
                onChange={toggleAll}
              />
              Select all
            </label>
            {list.map((d) => (
              <label key={d.id} className="settings-check">
                <input
                  type="checkbox"
                  checked={selected.has(d.id)}
                  onChange={() => toggle(d.id)}
                />
                {d.name}
              </label>
            ))}
            <button
              className="btn btn-primary"
              type="button"
              disabled={selected.size === 0}
              onClick={onExport}
            >
              Export selected
            </button>
          </>
        )}
      </section>

      <section className="settings-section">
        <h2>Import decks</h2>
        <p className="settings-hint">Same-named decks are replaced on import.</p>
        <input
          type="file"
          accept="application/json,.json"
          aria-label="Import backup file"
          onChange={onFile}
        />
        {error && <p className="settings-error">{error}</p>}
        {message && <p className="settings-ok">{message}</p>}
        {pending && (
          <div className="settings-warning" role="alertdialog" aria-label="Confirm replace">
            <p>These decks already exist and will be replaced:</p>
            <ul>
              {pending.replaced.map((name) => (
                <li key={name}>{name}</li>
              ))}
            </ul>
            <div className="add-row">
              <button className="btn btn-danger" type="button" onClick={() => runImport(pending.decks)}>
                Replace
              </button>
              <button className="btn btn-ghost" type="button" onClick={() => setPending(null)}>
                Cancel
              </button>
            </div>
          </div>
        )}
      </section>
      </div>
    </>
  )
}
