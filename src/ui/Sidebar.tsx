import { NavLink, useNavigate } from 'react-router-dom'
import { useLiveQuery } from 'dexie-react-hooks'
import { useStorage } from '../data/StorageContext'
import { ThemeToggle } from './ThemeToggle'
import { deckColor } from './deckColor'

const navClass = ({ isActive }: { isActive: boolean }) =>
  isActive ? 'nav-item is-active' : 'nav-item'

/** Persistent left sidebar: brand, Today, the deck list, and a footer. */
export function Sidebar() {
  const storage = useStorage()
  const navigate = useNavigate()

  const decks = useLiveQuery(async () => {
    const all = await storage.listDecks()
    const now = Date.now()
    return Promise.all(
      all.map(async (deck) => ({ deck, due: await storage.countDue(deck.id, now) })),
    )
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
          <span className="nav-dot" />
          <span className="nav-grow">Today</span>
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
        {(decks ?? []).map(({ deck, due }) => (
          <NavLink key={deck.id} to={`/decks/${deck.id}`} className={navClass}>
            <span className="deck-dot" style={{ background: deck.color ?? deckColor(deck.id) }} />
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
