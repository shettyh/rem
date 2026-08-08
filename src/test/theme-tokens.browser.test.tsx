import { test, expect, afterEach } from 'vitest'
import '../ui/styles.css'

afterEach(() => {
  delete document.documentElement.dataset.theme
})

function token(name: string): string {
  return getComputedStyle(document.documentElement).getPropertyValue(name).trim()
}

function bg(): string {
  return token('--bg')
}

function luminance(hex: string): number {
  const channels = hex.slice(1).match(/.{2}/g)?.map((value) => Number.parseInt(value, 16) / 255)
  if (!channels || channels.length !== 3) throw new Error(`Expected a hex color, received ${hex}`)
  const [r, g, b] = channels.map((value) =>
    value <= 0.04045 ? value / 12.92 : ((value + 0.055) / 1.055) ** 2.4,
  )
  return 0.2126 * r + 0.7152 * g + 0.0722 * b
}

function contrast(a: string, b: string): number {
  const [lighter, darker] = [luminance(a), luminance(b)].sort((x, y) => y - x)
  return (lighter + 0.05) / (darker + 0.05)
}

test('--bg resolves to the light content value by default', () => {
  expect(bg()).toBe('#faf9f6')
})

test('--bg resolves to the dark content value under [data-theme=dark]', () => {
  document.documentElement.dataset.theme = 'dark'
  expect(bg()).toBe('#0f0e13')
})

test.each(['light', 'dark'] as const)('%s secondary text remains readable on app surfaces', (theme) => {
  document.documentElement.dataset.theme = theme
  for (const foreground of ['--muted', '--faint']) {
    for (const background of ['--bg', '--surface', '--sidebar', '--surface-inset']) {
      expect(
        contrast(token(foreground), token(background)),
        `${foreground} on ${background}`,
      ).toBeGreaterThanOrEqual(4.5)
    }
  }
})
