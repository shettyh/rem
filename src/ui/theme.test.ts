import { describe, it, expect, beforeEach } from 'vitest'
import { THEME_KEY, getStoredTheme, resolveInitialTheme, applyTheme } from './theme'

describe('theme', () => {
  beforeEach(() => {
    localStorage.clear()
    delete document.documentElement.dataset.theme
  })

  it('getStoredTheme returns null when nothing valid is stored', () => {
    expect(getStoredTheme()).toBeNull()
    localStorage.setItem(THEME_KEY, 'banana')
    expect(getStoredTheme()).toBeNull()
  })

  it('getStoredTheme returns a stored valid theme', () => {
    localStorage.setItem(THEME_KEY, 'dark')
    expect(getStoredTheme()).toBe('dark')
  })

  it('resolveInitialTheme falls back to system (light in jsdom) when unset', () => {
    expect(resolveInitialTheme()).toBe('light')
  })

  it('applyTheme sets the data attribute and persists', () => {
    applyTheme('dark')
    expect(document.documentElement.dataset.theme).toBe('dark')
    expect(localStorage.getItem(THEME_KEY)).toBe('dark')
  })
})
