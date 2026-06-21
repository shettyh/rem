import { test, expect, afterEach } from 'vitest'
import '../ui/styles.css'

afterEach(() => {
  delete document.documentElement.dataset.theme
})

function bg(): string {
  return getComputedStyle(document.documentElement).getPropertyValue('--bg').trim()
}

test('--bg resolves to the light paper value by default', () => {
  expect(bg()).toBe('#f7f5f1')
})

test('--bg resolves to the warm-dark value under [data-theme=dark]', () => {
  document.documentElement.dataset.theme = 'dark'
  expect(bg()).toBe('#1a1815')
})
