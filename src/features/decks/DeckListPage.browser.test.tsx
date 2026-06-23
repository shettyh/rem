import { test, expect } from 'vitest'
import { page } from 'vitest/browser'
import { DeckListPage } from './DeckListPage'
import { freshStorage } from '../../test/seed'
import { renderRoute } from '../../test/renderRoute'

test('creates a deck with the chosen scheduler', async () => {
  const storage = freshStorage()
  await renderRoute({ storage, entry: '/', path: '/', element: <DeckListPage /> })

  // Default selection is FSRS.
  await page.getByLabelText('New deck name').fill('Algorithms')
  await page.getByRole('button', { name: 'Add deck' }).click()

  await expect.poll(async () => (await storage.listDecks())[0]?.schedulerKind).toBe('fsrs')
})

test('name input keeps its width and is not collapsed by the scheduler picker', async () => {
  const storage = freshStorage()
  await renderRoute({ storage, entry: '/', path: '/', element: <DeckListPage /> })

  const nameInput = page.getByLabelText('New deck name').element()
  const picker = page.getByLabelText('Scheduler').element()

  const nameWidth = nameInput.getBoundingClientRect().width
  const pickerWidth = picker.getBoundingClientRect().width

  // The name field should fill the row; the picker is content-sized.
  expect(nameWidth).toBeGreaterThan(pickerWidth)
})

test('create-row controls (input, picker, button) are the same height', async () => {
  const storage = freshStorage()
  await renderRoute({ storage, entry: '/', path: '/', element: <DeckListPage /> })

  const h = (el: Element) => el.getBoundingClientRect().height
  const input = h(page.getByLabelText('New deck name').element())
  const picker = h(page.getByLabelText('Scheduler').element())
  const button = h(page.getByRole('button', { name: 'Add deck' }).element())

  // All three sit on one row; their heights should match within a few px
  // (native <select> renders a hair taller). Guards against the input towering
  // over the picker/button when form controls don't share font metrics.
  const spread = Math.max(input, picker, button) - Math.min(input, picker, button)
  expect(spread).toBeLessThanOrEqual(4)
})

test('creates an SM-2 deck when SM-2 is selected', async () => {
  const storage = freshStorage()
  await renderRoute({ storage, entry: '/', path: '/', element: <DeckListPage /> })

  await page.getByLabelText('New deck name').fill('Spanish')
  await page.getByLabelText('Scheduler').selectOptions('sm2')
  await page.getByRole('button', { name: 'Add deck' }).click()

  await expect.poll(async () => (await storage.listDecks())[0]?.schedulerKind).toBe('sm2')
})
