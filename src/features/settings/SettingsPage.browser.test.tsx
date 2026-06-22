import { test, expect } from 'vitest'
import { page } from 'vitest/browser'
import { SettingsPage } from './SettingsPage'
import { freshStorage } from '../../test/seed'
import { renderRoute } from '../../test/renderRoute'
import { serializeBackup, type DeckBackup } from '../../data/backup'

function renderSettings(storage: ReturnType<typeof freshStorage>) {
  return renderRoute({ storage, entry: '/settings', path: '/settings', element: <SettingsPage /> })
}

test('export button enables only when a deck is selected', async () => {
  const storage = freshStorage()
  await storage.createDeck('TypeScript')
  await storage.createDeck('Spanish')
  renderSettings(storage)

  const exportBtn = page.getByRole('button', { name: 'Export selected' })
  await expect.element(exportBtn).toBeDisabled()
  await page.getByLabelText('Select all decks').click()
  await expect.element(exportBtn).toBeEnabled()
})

test('importing a same-named deck warns, then replaces on confirm', async () => {
  const storage = freshStorage()
  await storage.createDeck('Spanish')
  renderSettings(storage)

  const incoming: DeckBackup[] = [
    {
      name: 'Spanish',
      createdAt: 1,
      cards: [
        { front: 'hola', back: 'hello', createdAt: 1, updatedAt: 1, scheduling: { repetitions: 0, intervalDays: 0, easeFactor: 2.5, due: 1 } },
      ],
    },
  ]
  const file = new File([serializeBackup(incoming, 1000)], 'backup.json', { type: 'application/json' })

  // Locator.upload sets the file on the (real) <input type=file>.
  // Fallback if unavailable: import { userEvent } from 'vitest/browser'; userEvent.upload(locator, file)
  await page.getByLabelText('Import backup file').upload(file)

  await expect.element(page.getByText('will be replaced', { exact: false })).toBeVisible()
  await page.getByRole('button', { name: 'Replace' }).click()
  await expect.element(page.getByText('Imported', { exact: false })).toBeVisible()
})
