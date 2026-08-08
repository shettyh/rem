export const REMOTE_KEY = 'rem.sync.remoteUrl'
export const LAST_SYNC_KEY = 'rem.sync.lastSyncAt'
export const AUTO_SYNC_KEY = 'rem.sync.auto'

export function isAutoSyncEnabled(): boolean {
  return localStorage.getItem(AUTO_SYNC_KEY) !== 'false'
}
