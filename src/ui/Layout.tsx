import { Link, Outlet } from 'react-router-dom'
import { ThemeToggle } from './ThemeToggle'

export function Layout() {
  return (
    <div className="app">
      <header className="app-header">
        <Link to="/" className="app-title">
          rem
        </Link>
        <div className="header-actions">
          <Link to="/settings" className="settings-link" aria-label="Settings">
            ⚙
          </Link>
          <ThemeToggle />
        </div>
      </header>
      <main className="app-main">
        <Outlet />
      </main>
    </div>
  )
}
