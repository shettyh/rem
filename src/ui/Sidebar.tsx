import { NavLink } from 'react-router-dom'
import { useLiveQuery } from 'dexie-react-hooks'
import { useStorage } from '../data/StorageContext'
import { ThemeToggle } from './ThemeToggle'

const navClass = ({ isActive }: { isActive: boolean }) =>
  isActive ? 'nav-item is-active' : 'nav-item'

/** Persistent left sidebar: app nav, the deck list (navigation), and a footer. */
export function Sidebar() {
  const storage = useStorage()

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
      <div className="side-brand">rem</div>

      <nav>
        <NavLink to="/" end className={navClass}>
          <span className="nav-grow">Today</span>
        </NavLink>
      </nav>

      <div className="side-section">
        <span className="side-section-label">Decks</span>
        <NavLink to="/" end className="side-add" aria-label="New deck" title="New deck">
          +
        </NavLink>
      </div>

      <nav>
        {(decks ?? []).map(({ deck, due }) => (
          <NavLink key={deck.id} to={`/decks/${deck.id}`} className={navClass}>
            <span className="nav-grow">{deck.name}</span>
            {due > 0 && <span className="side-badge">{due}</span>}
          </NavLink>
        ))}
      </nav>

      <div className="side-spacer" />

      <div className="side-footer">
        <NavLink to="/settings" className="icon-btn" aria-label="Settings" title="Settings">
          ⚙
        </NavLink>
        <ThemeToggle />
      </div>
    </aside>
  )
}
