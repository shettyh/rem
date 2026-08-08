import { expect, test } from 'vitest'
import { render } from 'vitest-browser-react'
import { userEvent } from 'vitest/browser'
import './styles.css'

test('keyboard navigation gives controls a consistent visible focus ring', async () => {
  const { container } = await render(
    <div>
      <button type="button">Action</button>
      <a href="#target">Destination</a>
      <input aria-label="Name" />
      <select aria-label="Choice"><option>One</option></select>
    </div>,
  )

  const button = container.querySelector<HTMLButtonElement>('button')!
  expect(getComputedStyle(button).fontFamily).toBe(getComputedStyle(document.body).fontFamily)

  for (const selector of ['button', 'a', 'input', 'select']) {
    await userEvent.tab()
    const control = container.querySelector<HTMLElement>(selector)
    expect(document.activeElement).toBe(control)
    expect(control?.matches(':focus-visible')).toBe(true)
    expect(getComputedStyle(control!).outlineWidth).toBe('2px')
    expect(getComputedStyle(control!).outlineStyle).toBe('solid')
  }
})
