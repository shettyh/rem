import { Outlet } from 'react-router-dom'
import { Sidebar } from './Sidebar'

/** App shell: persistent sidebar + a window-filling content area. */
export function Layout() {
  return (
    <div className="app">
      <Sidebar />
      <main className="content">
        <Outlet />
      </main>
    </div>
  )
}
