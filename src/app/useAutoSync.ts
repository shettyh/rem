import { useEffect } from 'react'
import { isTauri } from '@tauri-apps/api/core'
import { appDataDir, join } from '@tauri-apps/api/path'
import type { Storage } from '../data/Storage'
import { GitSyncService } from '../data/sync/GitSyncService'
import { TauriGitBridge } from '../data/sync/TauriGitBridge'

const REMOTE_KEY = 'rem.sync.remoteUrl'
const LAST_SYNC_KEY = 'rem.sync.lastSyncAt'

async function runSync(storage: Storage): Promise<void> {
  const remoteUrl = (localStorage.getItem(REMOTE_KEY) ?? '').trim()
  if (!remoteUrl) return
  const repoDir = await join(await appDataDir(), 'repo')
  await new GitSyncService(storage, new TauriGitBridge(), { remoteUrl, repoDir }).sync()
  localStorage.setItem(LAST_SYNC_KEY, String(Date.now()))
}

/** Desktop-only: sync once on launch and whenever the window is hidden.
 *  No-op in the browser build or when no remote is configured. */
export function useAutoSync(storage: Storage): void {
  useEffect(() => {
    if (!isTauri()) return
    void runSync(storage).catch(() => {})
    const onHide = () => {
      if (document.visibilityState === 'hidden') void runSync(storage).catch(() => {})
    }
    document.addEventListener('visibilitychange', onHide)
    return () => document.removeEventListener('visibilitychange', onHide)
  }, [storage])
}
