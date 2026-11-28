import { useEffect, useRef, useState, type ChangeEvent } from 'react'
import { parseBackup, planImport, type DeckBackup } from '../../data/backup'
import type { Storage } from '../../data/Storage'
import type { ApkgImportPreview } from '../../domain/models'

export const BACKUP_FILE_ACCEPT = 'application/json,.json'
export const APKG_FILE_ACCEPT = '.apkg,.colpkg,application/octet-stream,application/zip'
export const MAX_APKG_BYTES = 512 * 1024 * 1024

export interface PendingBackupImport {
  decks: DeckBackup[]
  replaced: string[]
}

export interface PendingApkgImport {
  bytes: Uint8Array
  preview: ApkgImportPreview
}

export function formatBackupReplaceDescription(pending: PendingBackupImport): string {
  const names = pending.replaced.join(', ')
  const addedCount = pending.decks.length - pending.replaced.length
  if (addedCount > 0) {
    return `Importing will replace existing deck${pending.replaced.length === 1 ? '' : 's'}: ${names}. ${addedCount} new deck${addedCount === 1 ? '' : 's'} will also be added.`
  }
  return `Importing will replace existing deck${pending.replaced.length === 1 ? '' : 's'}: ${names}.`
}

export function formatApkgReplaceDescription(preview: ApkgImportPreview): string {
  const parts = [`${preview.totalCards} card${preview.totalCards === 1 ? '' : 's'}`]
  if (preview.mediaCount > 0) parts.push(`${preview.mediaCount} media`)
  if (preview.reviewLogs > 0) parts.push(`${preview.reviewLogs} review logs`)
  const addedNote =
    preview.added.length > 0
      ? ` (adding ${preview.added.length} new deck${preview.added.length === 1 ? '' : 's'})`
      : ''
  const warningNote =
    preview.warnings.length > 0 ? ` Warnings: ${preview.warnings.join(' · ')}` : ''
  return `Importing ${parts.join(', ')}${addedNote} will replace existing deck${preview.replaced.length === 1 ? '' : 's'}: ${preview.replaced.join(', ')}.${warningNote}`
}

export function useDeckImport(storage: Storage) {
  const backupInputRef = useRef<HTMLInputElement>(null)
  const apkgInputRef = useRef<HTMLInputElement>(null)
  const busyRef = useRef(false)
  const mountedRef = useRef(true)

  const [pendingBackup, setPendingBackup] = useState<PendingBackupImport | null>(null)
  const [pendingApkg, setPendingApkg] = useState<PendingApkgImport | null>(null)
  const [importingBackup, setImportingBackup] = useState(false)
  const [importingApkg, setImportingApkg] = useState(false)
  const [message, setMessage] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    mountedRef.current = true
    return () => {
      mountedRef.current = false
    }
  }, [])

  const canImportApkg = Boolean(storage.previewApkg && storage.importApkg)
  const importing = importingBackup || importingApkg

  async function executeBackupImport(toImport: DeckBackup[]) {
    try {
      const result = await storage.importDecks(toImport)
      if (result.replaced.length > 0) {
        await storage.sweepOrphanAssets()
      }
      if (!mountedRef.current) return
      setPendingBackup(null)
      const replacedNote = result.replaced.length ? ` (replaced ${result.replaced.length})` : ''
      setMessage(
        `Imported ${toImport.length} deck${toImport.length === 1 ? '' : 's'}${replacedNote}.`,
      )
    } catch (err) {
      if (!mountedRef.current) return
      setPendingBackup(null)
      setError(err instanceof Error ? err.message : 'Could not import backup.')
    }
  }

  async function executeApkgImport(bytes: Uint8Array) {
    if (!storage.importApkg) return
    try {
      const result = await storage.importApkg(bytes)
      if (!mountedRef.current) return
      setPendingApkg(null)
      const deckTotal = result.added.length + result.replaced.length
      const parts = [
        `${result.importedCards} card${result.importedCards === 1 ? '' : 's'}`,
        `${deckTotal} deck${deckTotal === 1 ? '' : 's'}`,
      ]
      if (result.importedReviews > 0) {
        parts.push(`${result.importedReviews} review${result.importedReviews === 1 ? '' : 's'}`)
      }
      if (result.importedMedia > 0) {
        parts.push(`${result.importedMedia} media`)
      }
      const replacedNote = result.replaced.length ? ` (replaced ${result.replaced.length})` : ''
      const warningNote = result.warnings.length ? ` · Warnings: ${result.warnings.join(' ')}` : ''
      setMessage(`Imported Anki deck: ${parts.join(', ')}${replacedNote}.${warningNote}`)
    } catch (err) {
      if (!mountedRef.current) return
      setPendingApkg(null)
      setError(err instanceof Error ? err.message : 'Could not import the Anki archive.')
    }
  }

  async function onBackupFile(e: ChangeEvent<HTMLInputElement>) {
    const file = e.target.files?.[0]
    e.target.value = '' // allow re-selecting the same file later
    if (!file || busyRef.current) return
    busyRef.current = true
    setImportingBackup(true)
    setError(null)
    setMessage(null)
    setPendingBackup(null)
    setPendingApkg(null)
    try {
      const parsed = parseBackup(await file.text(), Date.now())
      if (!mountedRef.current) return
      if (parsed.length === 0) {
        setError('Backup file contains no decks.')
        return
      }
      const existing = await storage.listDecks()
      if (!mountedRef.current) return
      const { replaced } = planImport(
        parsed.map((d) => d.name),
        existing.map((d) => d.name),
      )
      if (replaced.length > 0) {
        setPendingBackup({ decks: parsed, replaced })
      } else {
        await executeBackupImport(parsed)
      }
    } catch (err) {
      if (!mountedRef.current) return
      setError(err instanceof Error ? err.message : 'Could not read the file.')
    } finally {
      busyRef.current = false
      if (mountedRef.current) setImportingBackup(false)
    }
  }

  async function confirmBackupImport() {
    if (!pendingBackup || busyRef.current) return
    busyRef.current = true
    setImportingBackup(true)
    setError(null)
    setMessage(null)
    try {
      await executeBackupImport(pendingBackup.decks)
    } finally {
      busyRef.current = false
      if (mountedRef.current) setImportingBackup(false)
    }
  }

  function cancelBackupImport() {
    if (busyRef.current) return
    setPendingBackup(null)
  }

  async function onApkgFile(e: ChangeEvent<HTMLInputElement>) {
    const file = e.target.files?.[0]
    e.target.value = ''
    if (!file || !storage.previewApkg || !storage.importApkg || busyRef.current) return
    setError(null)
    setMessage(null)
    setPendingBackup(null)
    setPendingApkg(null)
    if (file.size > MAX_APKG_BYTES) {
      setError('Anki archive exceeds the maximum supported size (512 MB).')
      return
    }
    busyRef.current = true
    setImportingApkg(true)
    try {
      const bytes = new Uint8Array(await file.arrayBuffer())
      const preview = await storage.previewApkg(bytes)
      if (!mountedRef.current) return
      if (preview.totalCards === 0 && preview.added.length === 0 && preview.replaced.length === 0) {
        const detail = preview.warnings.length > 0 ? ` (${preview.warnings[0]})` : ''
        setError(`No importable cards were found in this .apkg file.${detail}`)
      } else if (preview.replaced.length > 0) {
        setPendingApkg({ bytes, preview })
      } else {
        await executeApkgImport(bytes)
      }
    } catch (err) {
      if (!mountedRef.current) return
      setError(err instanceof Error ? err.message : 'Could not read the Anki archive.')
    } finally {
      busyRef.current = false
      if (mountedRef.current) setImportingApkg(false)
    }
  }

  async function confirmApkgImport() {
    if (!pendingApkg || !storage.importApkg || busyRef.current) return
    busyRef.current = true
    setImportingApkg(true)
    setError(null)
    setMessage(null)
    try {
      await executeApkgImport(pendingApkg.bytes)
    } finally {
      busyRef.current = false
      if (mountedRef.current) setImportingApkg(false)
    }
  }

  function cancelApkgImport() {
    if (busyRef.current) return
    setPendingApkg(null)
  }

  function openBackupPicker() {
    if (busyRef.current) return
    backupInputRef.current?.click()
  }

  function openApkgPicker() {
    if (busyRef.current || !canImportApkg) return
    apkgInputRef.current?.click()
  }

  function clearFeedback() {
    setError(null)
    setMessage(null)
  }

  return {
    backupInputRef,
    apkgInputRef,
    canImportApkg,
    importing,
    importingBackup,
    importingApkg,
    pendingBackup,
    pendingApkg,
    message,
    error,
    openBackupPicker,
    openApkgPicker,
    onBackupFile,
    onApkgFile,
    confirmBackupImport,
    cancelBackupImport,
    confirmApkgImport,
    cancelApkgImport,
    clearFeedback,
  }
}
