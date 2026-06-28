import { test, expect, vi } from 'vitest'
import { page } from 'vitest/browser'
import { render } from 'vitest-browser-react'
import { Toggle } from './Toggle'

test('reflects checked state and flips on click', async () => {
  const onChange = vi.fn()
  render(<Toggle checked={false} onChange={onChange} label="Bury related new cards" />)
  const sw = page.getByRole('switch', { name: 'Bury related new cards' })
  await expect.element(sw).toHaveAttribute('aria-checked', 'false')
  await sw.click()
  expect(onChange).toHaveBeenCalledWith(true)
})
