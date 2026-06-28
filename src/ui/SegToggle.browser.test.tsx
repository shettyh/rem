import { test, expect, vi } from 'vitest'
import { page } from 'vitest/browser'
import { render } from 'vitest-browser-react'
import { SegToggle } from './SegToggle'

test('marks the active option and reports the other on click', async () => {
  const onChange = vi.fn()
  render(
    <SegToggle
      value="sequential"
      onChange={onChange}
      options={[
        { value: 'sequential', label: 'SEQ' },
        { value: 'random', label: 'RANDOM' },
      ]}
    />,
  )
  await expect.element(page.getByRole('button', { name: 'SEQ' })).toHaveAttribute('aria-pressed', 'true')
  await page.getByRole('button', { name: 'RANDOM' }).click()
  expect(onChange).toHaveBeenCalledWith('random')
})
