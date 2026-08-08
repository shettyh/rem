import { expect, test } from 'vitest'
import { page } from 'vitest/browser'
import { render } from 'vitest-browser-react'

test('danger and compact button variants have distinct semantics', async () => {
  await render(
    <div>
      <button type="button" className="btn btn-danger">Confirm delete</button>
      <button type="button" className="btn btn-danger-outline">Delete</button>
      <button type="button" className="btn btn-sm">Compact</button>
      <button type="button" className="btn">Default</button>
    </div>,
  )

  const danger = page.getByRole('button', { name: 'Confirm delete' }).element()
  const outline = page.getByRole('button', { name: 'Delete', exact: true }).element()
  const compact = page.getByRole('button', { name: 'Compact' }).element()
  const standard = page.getByRole('button', { name: 'Default' }).element()
  const dangerStyle = getComputedStyle(danger)
  const outlineStyle = getComputedStyle(outline)

  expect(dangerStyle.backgroundColor).not.toBe('rgba(0, 0, 0, 0)')
  expect(outlineStyle.backgroundColor).toBe('rgba(0, 0, 0, 0)')
  expect(dangerStyle.borderColor).toBe(outlineStyle.borderColor)
  expect(compact.getBoundingClientRect().height).toBeLessThan(standard.getBoundingClientRect().height)
})
