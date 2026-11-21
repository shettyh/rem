export const SIDEBAR_COLLAPSED_KEY = 'rem-sidebar-collapsed'
export const SIDEBAR_HOVER_EXPAND_KEY = 'rem-sidebar-hover-expand'

const SIDEBAR_PREFS_EVENT = 'rem:sidebar-prefs'

function notifySidebarPrefs(): void {
  if (typeof window !== 'undefined') {
    window.dispatchEvent(new Event(SIDEBAR_PREFS_EVENT))
  }
}

export function getSidebarCollapsed(): boolean {
  if (typeof localStorage === 'undefined') return false
  return localStorage.getItem(SIDEBAR_COLLAPSED_KEY) === 'true'
}

export function setSidebarCollapsed(collapsed: boolean): void {
  if (typeof localStorage !== 'undefined') {
    if (collapsed) localStorage.setItem(SIDEBAR_COLLAPSED_KEY, 'true')
    else localStorage.removeItem(SIDEBAR_COLLAPSED_KEY)
  }
  notifySidebarPrefs()
}

/** Whether hovering the collapsed icon rail temporarily floats the full sidebar open (default: true). */
export function getSidebarHoverExpand(): boolean {
  if (typeof localStorage === 'undefined') return true
  return localStorage.getItem(SIDEBAR_HOVER_EXPAND_KEY) !== 'false'
}

export function setSidebarHoverExpand(enabled: boolean): void {
  if (typeof localStorage !== 'undefined') {
    localStorage.setItem(SIDEBAR_HOVER_EXPAND_KEY, enabled ? 'true' : 'false')
  }
  notifySidebarPrefs()
}

export function subscribeSidebarPrefs(listener: () => void): () => void {
  if (typeof window === 'undefined') return () => {}
  window.addEventListener(SIDEBAR_PREFS_EVENT, listener)
  return () => window.removeEventListener(SIDEBAR_PREFS_EVENT, listener)
}
