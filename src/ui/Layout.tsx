import { Outlet, useLocation } from 'react-router-dom'
import { Sidebar } from './Sidebar'

/** App shell: persistent navigation, except during a focused study session. */
export function Layout() {
  const { pathname } = useLocation()
  const focusedReview =
    /^\/study\/?$/.test(pathname) || /^\/decks\/[^/]+\/study\/?$/.test(pathname)

  return (
    <div className={focusedReview ? 'app is-reviewing' : 'app'}>
      {!focusedReview && <Sidebar />}
      <main className="content">
        <Outlet />
      </main>
    </div>
  )
}
