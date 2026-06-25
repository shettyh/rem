import { test, expect, afterEach } from 'vitest'
import '../ui/styles.css'

afterEach(() => {
  delete document.documentElement.dataset.theme
})

function bg(): string {
  return getComputedStyle(document.documentElement).getPropertyValue('--bg').trim()
}

test('--bg resolves to the light content value by default', () => {
  expect(bg()).toBe('#faf9f6')
})

test('--bg resolves to the dark content value under [data-theme=dark]', () => {
  document.documentElement.dataset.theme = 'dark'
  expect(bg()).toBe('#0f0e13')
})
