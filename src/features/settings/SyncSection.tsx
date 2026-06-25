import { useState } from 'react'
import { isTauri } from '@tauri-apps/api/core'
import { appDataDir, join } from '@tauri-apps/api/path'
import type { Storage } from '../../data/Storage'
import { useStorage } from '../../data/StorageContext'
import { GitSyncService, type SyncConfig } from '../../data/sync/GitSyncService'
import { TauriGitBridge } from '../../data/sync/TauriGitBridge'

const REMOTE_KEY = 'rem.sync.remoteUrl'
const LAST_SYNC_KEY = 'rem.sync.lastSyncAt'

type Status =
  | { kind: 'idle' }
  | { kind: 'syncing' }
  | { kind: 'ok'; at: number }
  | { kind: 'error'; message: string }

function defaultMakeService(storage: Storage, cfg: SyncConfig): GitSyncService {
  return new GitSyncService(storage, new TauriGitBridge(), cfg)
}

async function resolveRepoDir(): Promise<string> {
  if (!isTauri()) return 'repo'
  return join(await appDataDir(), 'repo')
}

export function SyncSection({
  makeService = defaultMakeService,
}: {
  makeService?: (storage: Storage, cfg: SyncConfig) => GitSyncService
}) {
  const storage = useStorage()
  const [remoteUrl, setRemoteUrl] = useState(() => localStorage.getItem(REMOTE_KEY) ?? '')
  const lastSyncAt = Number(localStorage.getItem(LAST_SYNC_KEY)) || 0
  const [status, setStatus] = useState<Status>(
    lastSyncAt ? { kind: 'ok', at: lastSyncAt } : { kind: 'idle' },
  )

  function onUrlChange(value: string) {
    setRemoteUrl(value)
    localStorage.setItem(REMOTE_KEY, value)
  }

  async function onSync() {
    const url = remoteUrl.trim()
    if (!url) {
      setStatus({ kind: 'error', message: 'Enter a Git remote URL first.' })
      return
    }
    setStatus({ kind: 'syncing' })
    try {
      const repoDir = await resolveRepoDir()
      await makeService(storage, { remoteUrl: url, repoDir }).sync()
      const at = Date.now()
      localStorage.setItem(LAST_SYNC_KEY, String(at))
      setStatus({ kind: 'ok', at })
    } catch (err) {
      const message = err instanceof Error ? err.message : 'Sync failed.'
      setStatus({
        kind: 'error',
        message: message === 'git-not-installed' ? 'Git is not installed on this machine.' : message,
      })
    }
  }

  return (
    <section className="settings-section">
      <h2>Sync</h2>
      <p className="settings-hint">
        Sync decks across machines via a Git remote, using your existing git credentials.
      </p>
      <label className="field-label">Git remote URL</label>
      <input
        type="text"
        className="text-input settings-field-input"
        aria-label="Git remote URL"
        placeholder="git@github.com:you/rem-data.git"
        value={remoteUrl}
        onChange={(e) => onUrlChange(e.target.value)}
      />
      <button
        className="btn btn-primary btn-block"
        type="button"
        disabled={status.kind === 'syncing'}
        onClick={onSync}
      >
        {status.kind === 'syncing' ? 'Syncing…' : 'Sync now'}
      </button>
      {status.kind === 'ok' && (
        <p className="settings-status">Synced at {new Date(status.at).toLocaleString()}.</p>
      )}
      {status.kind === 'error' && <p className="settings-error">{status.message}</p>}
    </section>
  )
}
