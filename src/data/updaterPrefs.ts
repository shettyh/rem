export const AUTO_UPDATE_KEY = 'rem-auto-update'

/** Whether rem automatically checks for and downloads updates in the background (default: true). */
export function getAutoUpdateEnabled(): boolean {
  if (typeof localStorage === 'undefined') return true
  return localStorage.getItem(AUTO_UPDATE_KEY) !== 'false'
}

export function setAutoUpdateEnabled(enabled: boolean): void {
  if (typeof localStorage !== 'undefined') {
    localStorage.setItem(AUTO_UPDATE_KEY, enabled ? 'true' : 'false')
  }
}
