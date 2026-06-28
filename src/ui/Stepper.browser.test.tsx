import { test, expect, vi } from 'vitest'
import { page } from 'vitest/browser'
import { render } from 'vitest-browser-react'
import { Stepper } from './Stepper'

test('formats the value and increases by step, clamped to max', async () => {
  const onChange = vi.fn()
  render(<Stepper value={4} onChange={onChange} label="Easy interval" step={1} min={1} max={4} format={(v) => `${v}d`} />)

  await expect.element(page.getByText('4d')).toBeVisible()
  await page.getByLabelText('Increase Easy interval').click()
  expect(onChange).toHaveBeenCalledWith(4) // clamped at max

  await page.getByLabelText('Decrease Easy interval').click()
  expect(onChange).toHaveBeenCalledWith(3)
})
