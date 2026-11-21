import { useEffect, useRef, useState } from 'react'
import { NavLink, useNavigate } from 'react-router-dom'
import { useStorage } from '../data/StorageContext'
import { useStorageQuery } from '../data/useStorageQuery'
import { ThemeToggle } from './ThemeToggle'

const HOVER_INTENT_MS = 160

const navClass = ({ isActive }: { isActive: boolean }) =>
  isActive ? 'nav-item is-active' : 'nav-item'

interface SidebarProps {
  collapsed?: boolean
  hoverExpand?: boolean
  onToggleCollapse?: () => void
}

/** Persistent left sidebar: brand, Today, the deck list, and a footer. */
export function Sidebar({
  collapsed = false,
  hoverExpand = true,
  onToggleCollapse,
}: SidebarProps) {
  const storage = useStorage()
  const navigate = useNavigate()
  const [peeking, setPeeking] = useState(false)
  const hoverTimerRef = useRef<number | null>(null)

  const navigation = useStorageQuery(async () => {
    const [all, drafts] = await Promise.all([storage.listDecks(), storage.listDrafts()])
    const now = Date.now()
    const decks = await Promise.all(
      all.map(async (deck) => ({ deck, due: await storage.countDue(deck.id, now) })),
    )
    return { decks, draftCount: drafts.length }
  }, [])

  function clearHoverTimer() {
    if (hoverTimerRef.current !== null) {
      window.clearTimeout(hoverTimerRef.current)
      hoverTimerRef.current = null
    }
  }

  useEffect(() => {
    if (!collapsed || !hoverExpand) {
      clearHoverTimer()
      setPeeking(false)
    }
    return clearHoverTimer
  }, [collapsed, hoverExpand])

  function handleMouseEnter() {
    if (!collapsed || !hoverExpand) return
    clearHoverTimer()
    hoverTimerRef.current = window.setTimeout(() => {
      hoverTimerRef.current = null
      setPeeking(true)
    }, HOVER_INTENT_MS)
  }

  function handleMouseLeave() {
    clearHoverTimer()
    if (peeking) setPeeking(false)
  }

  function handleToggle() {
    clearHoverTimer()
    setPeeking(false)
    onToggleCollapse?.()
  }

  const isRail = collapsed && !peeking
  const sidebarClass = [
    'sidebar',
    collapsed ? 'is-collapsed' : '',
    peeking ? 'is-peeking' : '',
  ]
    .filter(Boolean)
    .join(' ')

  return (
    <div
      className={`sidebar-slot${collapsed ? ' is-collapsed' : ''}`}
      onMouseEnter={handleMouseEnter}
      onMouseLeave={handleMouseLeave}
    >
      <aside className={sidebarClass}>
        <div className="titlebar-spacer" data-tauri-drag-region />

        <div className="side-brand">
          <img className="brand-mark" src="/favicon.png" alt="" />
          <span className="brand-word">rem</span>
          {onToggleCollapse && (
            <button
              type="button"
              className="side-collapse-btn"
              aria-label={collapsed ? 'Expand sidebar' : 'Collapse sidebar'}
              aria-expanded={!isRail}
              title={collapsed ? 'Pin sidebar open (⌘\\)' : 'Collapse sidebar (⌘\\)'}
              onClick={handleToggle}
            >
              <svg viewBox="0 0 16 16" aria-hidden="true">
                <rect x="2" y="2.5" width="12" height="11" rx="2" />
                <path d="M6 2.5v11" />
              </svg>
            </button>
          )}
        </div>

        <nav className="side-nav">
          <NavLink to="/" end className={navClass} title="Today">
            <svg className="nav-icon" viewBox="0 0 16 16" aria-hidden="true">
              <rect x="2.5" y="3.5" width="11" height="10" rx="2" />
              <path d="M5 2v3M11 2v3M2.5 6.5h11" />
            </svg>
            <span className="nav-grow">Today</span>
          </NavLink>
          <NavLink
            to="/drafts"
            className={navClass}
            aria-label={`Drafts, ${navigation?.draftCount ?? 0} pending`}
            title="Drafts"
          >
            <svg className="nav-icon" viewBox="0 0 16 16" aria-hidden="true">
              <path d="M4 2.5h5l3 3V12a1.5 1.5 0 0 1-1.5 1.5h-5A1.5 1.5 0 0 1 4 12V2.5Z" />
              <path d="M9 2.5v3h3M6.5 8.5h3M6.5 11h3" />
            </svg>
            <span className="nav-grow">Drafts</span>
            {(navigation?.draftCount ?? 0) > 0 && (
              <span className="side-badge">{navigation?.draftCount}</span>
            )}
          </NavLink>
          <NavLink to="/stats" className={navClass} title="Stats">
            <svg className="nav-icon" viewBox="0 0 16 16" aria-hidden="true">
              <path d="M2.5 13.5h11M4 13.5v-4h2v4M8 13.5v-7h2v7M12 13.5v-10h2v10" />
            </svg>
            <span className="nav-grow">Stats</span>
          </NavLink>
        </nav>

        <div className="side-section">
          <span className="side-section-label">Decks</span>
          <button
            type="button"
            className="side-add"
            aria-label="New deck"
            title="Add deck"
            onClick={() => navigate('/', { state: { focusNewDeck: Date.now() } })}
          >
            +
          </button>
        </div>

        <div className="side-decks">
          {(navigation?.decks ?? []).map(({ deck, due }) => (
            <NavLink key={deck.id} to={`/decks/${deck.id}`} className={navClass} title={deck.name}>
              <svg className="deck-icon" viewBox="0 0 16 16" aria-hidden="true">
                <rect x="2.5" y="3.5" width="10" height="8" rx="1.5" />
                <path d="M4.5 1.5h7a2 2 0 0 1 2 2v6" />
              </svg>
              <span className="nav-grow">{deck.name}</span>
              {due > 0 && <span className="side-badge">{due}</span>}
            </NavLink>
          ))}
        </div>

        <div className="side-footer">
          <NavLink
            to="/settings"
            className={({ isActive }) => (isActive ? 'side-settings is-active' : 'side-settings')}
            aria-label="Settings"
            title="Settings"
          >
            <svg className="nav-icon" viewBox="0 0 16 16" aria-hidden="true">
              <path d="M2.5 4.5h6M11.5 4.5h2M2.5 8h2M7.5 8h6M2.5 11.5h7M12.5 11.5h1" />
              <circle cx="10" cy="4.5" r="1.5" />
              <circle cx="6" cy="8" r="1.5" />
              <circle cx="11" cy="11.5" r="1.5" />
            </svg>
            <span className="side-settings-label">Settings</span>
          </NavLink>
          <ThemeToggle />
        </div>
      </aside>
    </div>
  )
}
