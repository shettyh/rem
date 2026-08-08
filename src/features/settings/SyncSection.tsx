import { useState, type FormEvent } from 'react'
import { isTauri } from '@tauri-apps/api/core'
import { appDataDir, join } from '@tauri-apps/api/path'
import type { Storage } from '../../data/Storage'
import { useStorage } from '../../data/StorageContext'
import { GitSyncService, type SyncConfig } from '../../data/sync/GitSyncService'
import { TauriGitBridge } from '../../data/sync/TauriGitBridge'
import {
  AUTO_SYNC_KEY,
  LAST_SYNC_KEY,
  REMOTE_KEY,
  isAutoSyncEnabled,
} from '../../data/sync/preferences'

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
  const initialUrl = (localStorage.getItem(REMOTE_KEY) ?? '').trim()
  const lastSyncAt = Number(localStorage.getItem(LAST_SYNC_KEY)) || 0
  const [configuredUrl, setConfiguredUrl] = useState(initialUrl)
  const [draftUrl, setDraftUrl] = useState(initialUrl)
  const [setupOpen, setSetupOpen] = useState(false)
  const [autoSync, setAutoSync] = useState(isAutoSyncEnabled)
  const [lastSuccessfulAt, setLastSuccessfulAt] = useState(lastSyncAt)
  const [status, setStatus] = useState<Status>(
    lastSyncAt ? { kind: 'ok', at: lastSyncAt } : { kind: 'idle' },
  )

  const configured = configuredUrl.length > 0

  function toggleAutoSync(enabled: boolean) {
    setAutoSync(enabled)
    localStorage.setItem(AUTO_SYNC_KEY, String(enabled))
  }

  async function sync(remoteUrl: string, configure: boolean) {
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
      setLastSuccessfulAt(at)
      if (configure) {
        localStorage.setItem(REMOTE_KEY, url)
        setConfiguredUrl(url)
        setDraftUrl(url)
        setSetupOpen(false)
      }
      setStatus({ kind: 'ok', at })
    } catch (err) {
      const message = err instanceof Error ? err.message : 'Sync failed.'
      setStatus({
        kind: 'error',
        message: message === 'git-not-installed' ? 'Git is not installed on this machine.' : message,
      })
    }
  }

  function submitSetup(e: FormEvent) {
    e.preventDefault()
    void sync(draftUrl, true)
  }

  return (
    <section className="settings-panel" aria-labelledby="git-sync-title">
      <div className="settings-panel-summary">
        <div className="settings-panel-copy">
          <div className="settings-title-line">
            <h3 id="git-sync-title">Git sync</h3>
            <span className={`settings-badge ${configured ? 'is-connected' : ''}`}>
              {configured ? 'Connected' : 'Optional'}
            </span>
          </div>
          <p>
            {configured
              ? 'Your local data can sync automatically through your Git remote.'
              : 'Sync across devices using your system Git and existing credentials.'}
          </p>
        </div>
        {!configured && !setupOpen && (
          <button className="btn btn-primary btn-sm" type="button" onClick={() => setSetupOpen(true)}>
            Set up Git sync
          </button>
        )}
      </div>

      {setupOpen && (
        <form className="settings-setup" onSubmit={submitSetup}>
          <label className="field-label" htmlFor="git-remote-url">Git remote URL</label>
          <input
            id="git-remote-url"
            type="text"
            className="text-input settings-field-input"
            aria-label="Git remote URL"
            placeholder="git@github.com:you/rem-data.git"
            value={draftUrl}
            onChange={(e) => setDraftUrl(e.target.value)}
            autoFocus
          />
          <div className="settings-inline-actions">
            <button className="btn btn-primary btn-sm" type="submit" disabled={status.kind === 'syncing'}>
              {status.kind === 'syncing'
                ? configured ? 'Updating…' : 'Connecting…'
                : configured ? 'Update and sync' : 'Connect and sync'}
            </button>
            <button
              className="btn btn-ghost btn-sm"
              type="button"
              disabled={status.kind === 'syncing'}
              onClick={() => {
                setDraftUrl(configuredUrl)
                setSetupOpen(false)
                setStatus(lastSuccessfulAt ? { kind: 'ok', at: lastSuccessfulAt } : { kind: 'idle' })
              }}
            >
              Cancel
            </button>
          </div>
          {status.kind === 'error' && <p className="settings-error" role="alert">{status.message}</p>}
        </form>
      )}

      {configured && !setupOpen && (
        <div className="settings-connected">
          <div className="settings-remote">
            <span>Repository</span>
            <div className="settings-remote-value">
              <code title={configuredUrl}>{configuredUrl}</code>
              <button
                className="btn btn-ghost btn-sm"
                type="button"
                disabled={status.kind === 'syncing'}
                onClick={() => {
                  setDraftUrl(configuredUrl)
                  setSetupOpen(true)
                  setStatus(lastSuccessfulAt ? { kind: 'ok', at: lastSuccessfulAt } : { kind: 'idle' })
                }}
              >
                Change remote
              </button>
            </div>
          </div>
          <label className="settings-toggle-row">
            <span>
              <strong>Automatic sync</strong>
              <small>Sync when rem opens and when it moves to the background.</small>
            </span>
            <input
              type="checkbox"
              aria-label="Automatic sync"
              checked={autoSync}
              onChange={(e) => toggleAutoSync(e.target.checked)}
            />
          </label>
          <div className="settings-sync-actions">
            <button
              className="btn btn-ghost btn-sm"
              type="button"
              disabled={status.kind === 'syncing'}
              onClick={() => void sync(configuredUrl, false)}
            >
              {status.kind === 'syncing' ? 'Syncing…' : 'Sync now'}
            </button>
            {status.kind === 'ok' && (
              <p className="settings-status" role="status">
                Last synced {new Date(status.at).toLocaleString()}
              </p>
            )}
            {status.kind === 'error' && <p className="settings-error" role="alert">{status.message}</p>}
          </div>
        </div>
      )}
    </section>
  )
}
