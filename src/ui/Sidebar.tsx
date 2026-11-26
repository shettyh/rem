import { useEffect, useRef, useState } from 'react'
import { NavLink, useNavigate } from 'react-router-dom'
import { useStorage } from '../data/StorageContext'
import { useSyncOptional } from '../data/SyncContext'
import { useUpdaterOptional } from '../data/UpdaterContext'
import { useStorageQuery } from '../data/useStorageQuery'
import { ThemeToggle } from './ThemeToggle'

const HOVER_INTENT_MS = 160

const navClass = ({ isActive }: { isActive: boolean }) =>
  isActive ? 'nav-item is-active' : 'nav-item'

interface SidebarProps {
  collapsed?: boolean
  hoverExpand?: boolean
  onToggleCollapse?: () => void
  onOpenSearch?: () => void
}

/** Persistent left sidebar: brand, Today, the deck list, and a footer. */
export function Sidebar({
  collapsed = false,
  hoverExpand = true,
  onToggleCollapse,
  onOpenSearch,
}: SidebarProps) {
  const storage = useStorage()
  const sync = useSyncOptional()
  const updater = useUpdaterOptional()
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
          {onOpenSearch && (
            <button
              type="button"
              className="nav-item nav-item-btn"
              title="Search cards (⌘K)"
              onClick={onOpenSearch}
            >
              <svg className="nav-icon" viewBox="0 0 16 16" aria-hidden="true">
                <circle cx="7" cy="7" r="4.5" />
                <path d="m10.5 10.5 3 3" />
              </svg>
              <span className="nav-grow">Search</span>
              <span className="side-kbd" aria-hidden="true">⌘K</span>
            </button>
          )}
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
          {(() => {
            const updateReady =
              updater?.state.status === 'available' || updater?.state.status === 'ready'
            const updateLabel =
              updater?.state.status === 'ready'
                ? `Settings — restart to install v${updater.state.info.version}`
                : updater?.state.status === 'available'
                  ? `Settings — update v${updater.state.info.version} available`
                  : 'Settings'
            return (
              <NavLink
                to="/settings"
                className={({ isActive }) => (isActive ? 'side-settings is-active' : 'side-settings')}
                aria-label={updateLabel}
                title={updateLabel}
              >
                <svg className="nav-icon" viewBox="0 0 16 16" aria-hidden="true">
                  <path d="M2.5 4.5h6M11.5 4.5h2M2.5 8h2M7.5 8h6M2.5 11.5h7M12.5 11.5h1" />
                  <circle cx="10" cy="4.5" r="1.5" />
                  <circle cx="6" cy="8" r="1.5" />
                  <circle cx="11" cy="11.5" r="1.5" />
                </svg>
                <span className="side-settings-label">Settings</span>
                {updateReady && <span className="side-update-dot" aria-hidden="true" />}
              </NavLink>
            )
          })()}
          {sync?.status?.mode === 'cloud' &&
            (() => {
              const st = sync.status
              const entState = st.entitlement?.state ?? null
              const isPaused = st.entitlement ? !st.entitlement.syncEnabled : st.subscriptionRequired
              const isPastDue = entState === 'past_due'
              const needsSettings = !st.signedIn || Boolean(st.lastError) || isPaused || isPastDue
              const dotState =
                st.activity === 'syncing'
                  ? 'syncing'
                  : isPaused || isPastDue
                    ? 'pending'
                    : st.activity === 'error' || st.lastError
                      ? 'error'
                      : !st.signedIn || st.pendingOutbox > 0
                        ? 'pending'
                        : 'synced'
              const pausedLabel = isPastDue
                ? 'rem Premium payment issue — open Settings'
                : 'rem Premium paused — open Settings'
              const ariaLabel =
                st.activity === 'syncing'
                  ? 'Cloud sync in progress'
                  : isPaused || isPastDue
                    ? pausedLabel
                    : st.activity === 'error' || st.lastError
                      ? 'Cloud sync error — open Settings'
                      : !st.signedIn
                        ? 'Cloud sync signed out — open Settings'
                        : st.pendingOutbox > 0
                          ? `Cloud sync: ${st.pendingOutbox} pending — click to sync`
                          : 'Cloud synced — click to sync now'
              const title =
                st.activity === 'syncing'
                  ? 'Syncing…'
                  : isPaused || isPastDue
                    ? pausedLabel
                    : st.lastError
                      ? `Sync error: ${st.lastError}`
                      : !st.signedIn
                        ? 'Signed out — open Settings'
                        : st.pendingOutbox > 0
                          ? `${st.pendingOutbox} pending change(s) — click to sync`
                          : 'Synced — click to sync now'

              return (
                <button
                  type="button"
                  className={`side-sync-btn is-${dotState}`}
                  aria-label={ariaLabel}
                  title={title}
                  disabled={st.activity === 'syncing'}
                  onClick={() => {
                    if (needsSettings) {
                      navigate('/settings')
                    } else {
                      void sync.syncNow().catch(() => {})
                    }
                  }}
                >
                  <svg className="side-sync-icon" viewBox="0 0 16 16" aria-hidden="true">
                    <path d="M12.5 6.5A4.5 4.5 0 0 0 4.2 5.2" />
                    <path d="M4.2 2.8v2.4h2.4" />
                    <path d="M3.5 9.5a4.5 4.5 0 0 0 8.3 1.3" />
                    <path d="M11.8 13.2v-2.4H9.4" />
                  </svg>
                  <span className="side-sync-dot" aria-hidden="true" />
                </button>
              )
            })()}
          <ThemeToggle />
        </div>
      </aside>
    </div>
  )
}
