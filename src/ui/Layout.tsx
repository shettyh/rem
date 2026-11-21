import { useEffect, useState } from 'react'
import { Outlet, useLocation } from 'react-router-dom'
import { Sidebar } from './Sidebar'
import {
  getSidebarCollapsed,
  getSidebarHoverExpand,
  setSidebarCollapsed,
  subscribeSidebarPrefs,
} from './sidebarPrefs'

/** App shell: persistent navigation, except during a focused study session. */
export function Layout() {
  const { pathname } = useLocation()
  const focusedReview =
    /^\/study\/?$/.test(pathname) || /^\/decks\/[^/]+\/study\/?$/.test(pathname)

  const [collapsed, setCollapsed] = useState<boolean>(() => getSidebarCollapsed())
  const [hoverExpand, setHoverExpand] = useState<boolean>(() => getSidebarHoverExpand())

  useEffect(() => {
    return subscribeSidebarPrefs(() => {
      setCollapsed(getSidebarCollapsed())
      setHoverExpand(getSidebarHoverExpand())
    })
  }, [])

  // ⌘\ / Ctrl+\ toggles the sidebar rail when not in a focused study session.
  useEffect(() => {
    if (focusedReview) return
    function onKey(e: KeyboardEvent) {
      if ((e.metaKey || e.ctrlKey) && !e.shiftKey && !e.altKey && e.key === '\\') {
        e.preventDefault()
        setSidebarCollapsed(!getSidebarCollapsed())
      }
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [focusedReview])

  const appClass = focusedReview
    ? 'app is-reviewing'
    : collapsed
      ? 'app is-sidebar-collapsed'
      : 'app'

  return (
    <div className={appClass}>
      {!focusedReview && (
        <Sidebar
          collapsed={collapsed}
          hoverExpand={hoverExpand}
          onToggleCollapse={() => setSidebarCollapsed(!collapsed)}
        />
      )}
      <main className="content">
        <Outlet />
      </main>
    </div>
  )
}
