import { useState } from 'react'
import { type Theme, resolveInitialTheme, applyTheme } from './theme'

/** Sidebar-footer button that flips between light and dark and persists the choice. */
export function ThemeToggle() {
  const [theme, setTheme] = useState<Theme>(resolveInitialTheme)

  function toggle() {
    const next: Theme = theme === 'dark' ? 'light' : 'dark'
    applyTheme(next)
    setTheme(next)
  }

  return (
    <button type="button" className="theme-toggle" aria-label="Toggle theme" onClick={toggle}>
      <span className="dot" style={{ background: theme === 'dark' ? 'transparent' : 'var(--accent)' }} />
      {theme === 'dark' ? 'Dark' : 'Light'}
    </button>
  )
}
