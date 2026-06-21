import { Link, Outlet } from 'react-router-dom'
import { ThemeToggle } from './ThemeToggle'

export function Layout() {
  return (
    <div className="app">
      <header className="app-header">
        <Link to="/" className="app-title">
          rem
        </Link>
        <ThemeToggle />
      </header>
      <main className="app-main">
        <Outlet />
      </main>
    </div>
  )
}
