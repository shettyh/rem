import { useCallback, useEffect, useState } from 'react'
import { Outlet, useLocation } from 'react-router-dom'
import { SearchModal } from './SearchModal'
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
  const [searchOpen, setSearchOpen] = useState(false)
  const closeSearch = useCallback(() => setSearchOpen(false), [])

  useEffect(() => {
    if (focusedReview) setSearchOpen(false)
  }, [focusedReview])

  useEffect(() => {
    return subscribeSidebarPrefs(() => {
      setCollapsed(getSidebarCollapsed())
      setHoverExpand(getSidebarHoverExpand())
    })
  }, [])

  // ⌘\ / Ctrl+\ toggles the sidebar rail; ⌘K / Ctrl+K opens cross-deck search.
  useEffect(() => {
    if (focusedReview) return
    function onKey(e: KeyboardEvent) {
      if ((e.metaKey || e.ctrlKey) && !e.shiftKey && !e.altKey && e.key === '\\') {
        e.preventDefault()
        setSidebarCollapsed(!getSidebarCollapsed())
      } else if ((e.metaKey || e.ctrlKey) && !e.shiftKey && !e.altKey && e.key.toLowerCase() === 'k') {
        if (document.querySelector('dialog[open]')) return
        e.preventDefault()
        setSearchOpen((prev) => !prev)
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
          onOpenSearch={() => setSearchOpen(true)}
        />
      )}
      <main className="content">
        <Outlet />
      </main>
      <SearchModal open={searchOpen} onClose={closeSearch} />
    </div>
  )
}
