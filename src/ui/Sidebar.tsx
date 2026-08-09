import { NavLink, useNavigate } from 'react-router-dom'
import { useStorage } from '../data/StorageContext'
import { useStorageQuery } from '../data/useStorageQuery'
import { ThemeToggle } from './ThemeToggle'

const navClass = ({ isActive }: { isActive: boolean }) =>
  isActive ? 'nav-item is-active' : 'nav-item'

/** Persistent left sidebar: brand, Today, the deck list, and a footer. */
export function Sidebar() {
  const storage = useStorage()
  const navigate = useNavigate()

  const navigation = useStorageQuery(async () => {
    const [all, drafts] = await Promise.all([storage.listDecks(), storage.listDrafts()])
    const now = Date.now()
    const decks = await Promise.all(
      all.map(async (deck) => ({ deck, due: await storage.countDue(deck.id, now) })),
    )
    return { decks, draftCount: drafts.length }
  }, [])

  return (
    <aside className="sidebar">
      <div className="titlebar-spacer" data-tauri-drag-region />

      <div className="side-brand">
        <img className="brand-mark" src="/favicon.png" alt="" />
        <span className="brand-word">rem</span>
        <span className="brand-dot" />
        <span className="brand-tag">recall</span>
      </div>

      <nav className="side-nav">
        <NavLink to="/" end className={navClass}>
          <svg className="nav-icon" viewBox="0 0 16 16" aria-hidden="true">
            <rect x="2.5" y="3.5" width="11" height="10" rx="2" />
            <path d="M5 2v3M11 2v3M2.5 6.5h11" />
          </svg>
          <span className="nav-grow">Today</span>
        </NavLink>
        <NavLink to="/drafts" className={navClass} aria-label={`Drafts, ${navigation?.draftCount ?? 0} pending`}>
          <svg className="nav-icon" viewBox="0 0 16 16" aria-hidden="true">
            <path d="M4 2.5h5l3 3V12a1.5 1.5 0 0 1-1.5 1.5h-5A1.5 1.5 0 0 1 4 12V2.5Z" />
            <path d="M9 2.5v3h3M6.5 8.5h3M6.5 11h3" />
          </svg>
          <span className="nav-grow">Drafts</span>
          {(navigation?.draftCount ?? 0) > 0 && (
            <span className="side-badge">{navigation?.draftCount}</span>
          )}
        </NavLink>
        <NavLink to="/stats" className={navClass}>
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
          <NavLink key={deck.id} to={`/decks/${deck.id}`} className={navClass}>
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
          <span className="ico-lines">
            <span />
            <span />
            <span />
          </span>
          Settings
        </NavLink>
        <ThemeToggle />
      </div>
    </aside>
  )
}
