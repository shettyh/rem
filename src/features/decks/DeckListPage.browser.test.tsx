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

test('creates an SM-2 deck when SM-2 is selected', async () => {
  const storage = freshStorage()
  await renderRoute({ storage, entry: '/', path: '/', element: <DeckListPage /> })

  await page.getByLabelText('New deck name').fill('Spanish')
  await page.getByLabelText('Scheduler').selectOptions('sm2')
  await page.getByRole('button', { name: 'Add deck' }).click()

  await expect.poll(async () => (await storage.listDecks())[0]?.schedulerKind).toBe('sm2')
})
