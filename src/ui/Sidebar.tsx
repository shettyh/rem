import { useEffect, useRef, useState, type CSSProperties } from 'react'
import { NavLink, useLocation, useNavigate } from 'react-router-dom'
import { useStorage } from '../data/StorageContext'
import { useSyncOptional } from '../data/SyncContext'
import { useUpdaterOptional } from '../data/UpdaterContext'
import { useStorageQuery } from '../data/useStorageQuery'
import {
  buildDeckTree,
  COLLAPSED_DECKS_EVENT,
  describeDeckDelete,
  flattenVisibleDeckTree,
  loadCollapsedDeckPaths,
  pruneCollapsedDeckPaths,
  saveCollapsedDeckPaths,
  summarizeDeckSubtree,
  type DeckTreeNode,
} from '../features/decks/deckTree'
import { loadDueOverview, type DeckOverview } from '../features/review/dueOverview'
import { ConfirmDialog } from './ConfirmDialog'
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
  const location = useLocation()
  const [peeking, setPeeking] = useState(false)
  const [collapsedDeckPaths, setCollapsedDeckPaths] = useState<Set<string>>(() =>
    loadCollapsedDeckPaths(),
  )
  const [pendingDelete, setPendingDelete] = useState<DeckTreeNode<DeckOverview> | null>(null)
  const hoverTimerRef = useRef<number | null>(null)

  useEffect(() => {
    const sync = () => setCollapsedDeckPaths(loadCollapsedDeckPaths())
    window.addEventListener(COLLAPSED_DECKS_EVENT, sync)
    return () => window.removeEventListener(COLLAPSED_DECKS_EVENT, sync)
  }, [])

  function toggleDeckPath(path: string) {
    const next = new Set(collapsedDeckPaths)
    if (next.has(path)) next.delete(path)
    else next.add(path)
    setCollapsedDeckPaths(next)
    saveCollapsedDeckPaths(next)
  }

  async function confirmDeleteDeckNode(node: DeckTreeNode<DeckOverview>) {
    const summary = summarizeDeckSubtree(node)
    setPendingDelete(null)
    try {
      const deck = node.item?.deck ?? null
      const deletedIds = deck
        ? (await storage.deleteDeck(deck.id), summary.deckIds)
        : await storage.deleteDeckTree(node.path)
      await storage.sweepOrphanAssets()
      const nextCollapsed = pruneCollapsedDeckPaths(loadCollapsedDeckPaths(), [
        node.path,
        ...summary.paths,
      ])
      setCollapsedDeckPaths(nextCollapsed)
      saveCollapsedDeckPaths(nextCollapsed)
      const deletedSet = new Set([...summary.deckIds, ...deletedIds])
      const match = location.pathname.match(/^\/decks\/([^/]+)/)
      if (match && deletedSet.has(decodeURIComponent(match[1]))) {
        navigate('/')
      }
    } catch (err) {
      console.error('sidebar deck delete failed', err)
    }
  }

  const navigation = useStorageQuery(async () => {
    const [overview, drafts] = await Promise.all([
      loadDueOverview(storage, Date.now()),
      storage.listDrafts(),
    ])
    return { decks: overview.decks, draftCount: drafts.length }
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
              className="nav-item side-search-btn"
              title="Search decks and cards (⌘K)"
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

        {(() => {
          const deckTree = buildDeckTree(navigation?.decks ?? [])
          const hasAnyNested = deckTree.some((root) => root.children.length > 0)
          const visibleNodes = flattenVisibleDeckTree(deckTree, collapsedDeckPaths)
          return (
            <div className={`side-decks${hasAnyNested ? ' has-nested' : ''}`}>
              {visibleNodes.map((node) => {
                const deck = node.item?.deck ?? null
                const hasChildren = node.children.length > 0
                const isCollapsed = collapsedDeckPaths.has(node.path)
                const visualDepth = Math.min(node.depth, 5)
                const isActiveDeck = Boolean(
                  deck &&
                    (location.pathname === `/decks/${deck.id}` ||
                      location.pathname.startsWith(`/decks/${deck.id}/`)),
                )
                return (
                  <div
                    key={deck ? deck.id : `group:${node.path}`}
                    className={`side-deck-row${isActiveDeck ? ' is-active' : ''}${!deck ? ' is-folder' : ''}${hasChildren ? ' has-children' : ''}`}
                    data-depth={node.depth}
                    style={{ '--deck-depth': visualDepth } as CSSProperties}
                  >
                    {hasAnyNested &&
                      (hasChildren ? (
                        <button
                          type="button"
                          className={`deck-tree-toggle${isCollapsed ? '' : ' is-expanded'}`}
                          aria-label={isCollapsed ? `Expand ${node.label}` : `Collapse ${node.label}`}
                          aria-expanded={!isCollapsed}
                          onClick={() => toggleDeckPath(node.path)}
                        >
                          <svg className="deck-chevron" viewBox="0 0 12 12" aria-hidden="true">
                            <path d="M4.25 2.75 7.75 6l-3.5 3.25" />
                          </svg>
                        </button>
                      ) : (
                        <span className="deck-tree-spacer" aria-hidden="true" />
                      ))}
                    {deck ? (
                      <NavLink
                        to={`/decks/${deck.id}`}
                        className="side-deck-main"
                        title={deck.name}
                      >
                        {hasChildren ? (
                          <svg className="deck-icon deck-folder-icon" viewBox="0 0 16 16" aria-hidden="true">
                            <path d="M2 4.5A1.5 1.5 0 0 1 3.5 3h2.6a1.5 1.5 0 0 1 1.15.54l.65.78a1 1 0 0 0 .77.36h3.83A1.5 1.5 0 0 1 14 6.18v5.32A1.5 1.5 0 0 1 12.5 13h-9A1.5 1.5 0 0 1 2 11.5v-7Z" />
                          </svg>
                        ) : (
                          <svg className="deck-icon" viewBox="0 0 16 16" aria-hidden="true">
                            <rect x="2.5" y="3.5" width="10" height="8" rx="1.5" />
                            <path d="M4.5 1.5h7a2 2 0 0 1 2 2v6" />
                          </svg>
                        )}
                        <span className="nav-grow">{node.label}</span>
                      </NavLink>
                    ) : (
                      <button
                        type="button"
                        className="side-deck-main side-deck-folder"
                        aria-expanded={!isCollapsed}
                        title={node.path}
                        onClick={() => toggleDeckPath(node.path)}
                      >
                        <svg className="deck-icon deck-folder-icon" viewBox="0 0 16 16" aria-hidden="true">
                          <path d="M2 4.5A1.5 1.5 0 0 1 3.5 3h2.6a1.5 1.5 0 0 1 1.15.54l.65.78a1 1 0 0 0 .77.36h3.83A1.5 1.5 0 0 1 14 6.18v5.32A1.5 1.5 0 0 1 12.5 13h-9A1.5 1.5 0 0 1 2 11.5v-7Z" />
                        </svg>
                        <span className="nav-grow">{node.label}</span>
                      </button>
                    )}
                    <span className="side-deck-trail">
                      {node.due > 0 && <span className="side-badge">{node.due}</span>}
                      <button
                        type="button"
                        className="side-deck-delete"
                        aria-label={`Delete ${node.label}`}
                        title={hasChildren ? `Delete ${node.label} and subdecks` : `Delete ${node.label}`}
                        onClick={(e) => {
                          e.stopPropagation()
                          setPendingDelete(node)
                        }}
                      >
                        <svg viewBox="0 0 16 16" aria-hidden="true">
                          <path d="M3 4.5h10M6 4.5V3.25A.75.75 0 0 1 6.75 2.5h2.5a.75.75 0 0 1 .75.75V4.5M5 4.5l.6 8.2a1 1 0 0 0 1 .93h2.8a1 1 0 0 0 1-.93l.6-8.2" />
                        </svg>
                      </button>
                    </span>
                  </div>
                )
              })}
            </div>
          )
        })()}

        {updater?.state.status === 'ready' && (
          <div className="side-update-wrap">
            <button
              type="button"
              className="side-update-btn"
              disabled={updater.relaunching}
              aria-label={`Update v${updater.state.info.version} ready — restart now`}
              title={`Update v${updater.state.info.version} ready — restart now`}
              onClick={() => void updater.relaunch()}
            >
              <svg className="side-update-icon" viewBox="0 0 16 16" aria-hidden="true">
                <circle cx="8" cy="8" r="6" />
                <path d="M8 10.75V5.25M5.75 7.5 8 5.25 10.25 7.5" />
              </svg>
              <span className="side-update-copy">
                <strong>{updater.relaunching ? 'Restarting…' : 'Update ready'}</strong>
                <small>Restart for v{updater.state.info.version}</small>
              </span>
            </button>
          </div>
        )}

        <div className="side-footer">
          {(() => {
            const updateAvailable = updater?.state.status === 'available'
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
                {updateAvailable && <span className="side-update-dot" aria-hidden="true" />}
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
      {pendingDelete && (() => {
        const copy = describeDeckDelete(pendingDelete)
        return (
          <ConfirmDialog
            danger
            title={copy.title}
            description={copy.description}
            confirmLabel={copy.confirmLabel}
            onConfirm={() => void confirmDeleteDeckNode(pendingDelete)}
            onCancel={() => setPendingDelete(null)}
          />
        )
      })()}
    </div>
  )
}
