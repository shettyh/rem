import { test, expect } from 'vitest'
import { render } from 'vitest-browser-react'
import { page } from 'vitest/browser'

test('browser mode mounts a component and writes a screenshot', async () => {
  render(<button data-testid="smoke">Hello rem</button>)
  await expect.element(page.getByTestId('smoke')).toBeVisible()
  const path = await page.getByTestId('smoke').screenshot({ path: '../../test-artifacts/smoke.png' })
  expect(path).toBeTruthy()
})
