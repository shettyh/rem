import { Link, Outlet } from 'react-router-dom'

export function Layout() {
  return (
    <div className="app">
      <header className="app-header">
        <Link to="/" className="app-title">
          rem
        </Link>
      </header>
      <main className="app-main">
        <Outlet />
      </main>
    </div>
  )
}
