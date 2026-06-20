import { page } from 'vitest/browser'

/**
 * Screenshot the element tagged with `data-testid={testId}` into
 * test-artifacts/<name>.png. Paths are relative to the calling test file, so all
 * screenshot tests must live in src/test/ for `../../` to reach repo root.
 */
export async function shoot(testId: string, name: string): Promise<void> {
  await page.getByTestId(testId).screenshot({ path: `../../test-artifacts/${name}.png` })
}
