import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import { RouterProvider } from 'react-router-dom'
import { router } from './routes'
import { StorageProvider } from '../data/StorageContext'
import { applyCardTextSize, getStoredCardTextSize } from '../ui/cardTextSize'
import '../ui/styles.css'

applyCardTextSize(getStoredCardTextSize())

/** rem is a native app; the Tauri IPC bridge is always present inside the desktop window. */
const isDesktop = '__TAURI_INTERNALS__' in window || '__TAURI__' in window

/** Shown when the web bundle is opened in a plain browser instead of the desktop app. */
function DesktopOnly() {
  return (
    <div className="desktop-only">
      <div className="empty-state">
        <div className="ico" aria-hidden="true">DESKTOP</div>
        <h3>rem is a desktop app</h3>
        <p>
          rem runs natively with a local SQLite collection, which a browser can't open directly.
          Launch it with <code>npm run app:dev</code>, or install the desktop build. This
          localhost URL is just the desktop window's internal source.
        </p>
      </div>
    </div>
  )
}

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    {isDesktop ? (
      <StorageProvider>
        <RouterProvider router={router} />
      </StorageProvider>
    ) : (
      <DesktopOnly />
    )}
  </StrictMode>,
)
