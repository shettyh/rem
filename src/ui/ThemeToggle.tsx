import { useState } from 'react'
import { type Theme, resolveInitialTheme, applyTheme } from './theme'

/** Sidebar-footer button that flips between light and dark and persists the choice. */
export function ThemeToggle() {
  const [theme, setTheme] = useState<Theme>(() => {
    const applied = document.documentElement.dataset.theme
    return applied === 'light' || applied === 'dark' ? applied : resolveInitialTheme()
  })

  const nextTheme: Theme = theme === 'dark' ? 'light' : 'dark'
  const actionLabel = `Switch to ${nextTheme} theme`

  function toggle() {
    applyTheme(nextTheme)
    setTheme(nextTheme)
  }

  return (
    <button
      type="button"
      className="theme-toggle"
      aria-label={actionLabel}
      title={actionLabel}
      onClick={toggle}
    >
      <span className="theme-icon" aria-hidden="true">
        {theme === 'dark' ? (
          <svg viewBox="0 0 16 16">
            <path
              d="M11.8 10.7A5.4 5.4 0 0 1 5.3 4.2 5.5 5.5 0 1 0 11.8 10.7Z"
              fill="currentColor"
              stroke="none"
            />
          </svg>
        ) : (
          <svg viewBox="0 0 16 16">
            <circle cx="8" cy="8" r="2.7" />
            <path d="M8 1v1.5M8 13.5V15M1 8h1.5M13.5 8H15M3 3l1.1 1.1M11.9 11.9 13 13M13 3l-1.1 1.1M4.1 11.9 3 13" />
          </svg>
        )}
      </span>
      <span>{theme === 'dark' ? 'Dark' : 'Light'}</span>
    </button>
  )
}
