import { describe, it, expect, beforeEach } from 'vitest'
import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { ThemeToggle } from './ThemeToggle'
import { THEME_KEY } from './theme'

describe('ThemeToggle', () => {
  beforeEach(() => {
    localStorage.clear()
    document.documentElement.dataset.theme = 'light'
  })

  it('reflects an already-applied dark theme', () => {
    document.documentElement.dataset.theme = 'dark'
    render(<ThemeToggle />)

    const btn = screen.getByRole('button', { name: 'Switch to light theme' })
    expect(btn).toHaveTextContent('Dark')
  })

  it('shows the current theme and names the theme it will switch to', async () => {
    render(<ThemeToggle />)
    const btn = screen.getByRole('button', { name: 'Switch to dark theme' })
    expect(btn).toHaveTextContent('Light')

    await userEvent.click(btn)
    expect(document.documentElement.dataset.theme).toBe('dark')
    expect(localStorage.getItem(THEME_KEY)).toBe('dark')
    expect(btn).toHaveAccessibleName('Switch to light theme')
    expect(btn).toHaveTextContent('Dark')

    await userEvent.click(btn)
    expect(document.documentElement.dataset.theme).toBe('light')
    expect(localStorage.getItem(THEME_KEY)).toBe('light')
  })
})
