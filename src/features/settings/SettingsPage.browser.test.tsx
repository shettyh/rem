import { test, expect } from 'vitest'
import { page } from 'vitest/browser'
import { SettingsPage } from './SettingsPage'
import { freshStorage } from '../../test/seed'
import { renderRoute } from '../../test/renderRoute'
import { serializeBackup, type DeckBackup } from '../../data/backup'
import { DEFAULT_DECK_SETTINGS } from '../../domain/models'
import packageInfo from '../../../package.json'

function renderSettings(storage: ReturnType<typeof freshStorage>) {
  return renderRoute({ storage, entry: '/settings', path: '/settings', element: <SettingsPage /> })
}

test('presents local storage first and keeps per-deck export behind choose decks', async () => {
  const storage = freshStorage()
  await storage.createDeck('TypeScript')
  await storage.createDeck('Spanish')
  renderSettings(storage)

  await expect.element(page.getByRole('heading', { name: 'Data & storage' })).toBeVisible()
  await expect.element(page.getByText('Active')).toBeVisible()
  await expect.element(page.getByRole('button', { name: 'Export all' })).toBeEnabled()
  await expect.element(page.getByText(`rem version ${packageInfo.version}`)).toBeVisible()
  await expect.element(page.getByLabelText('Select all decks')).not.toBeInTheDocument()

  await page.getByRole('button', { name: 'Choose decks' }).click()
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
      schedulerKind: 'fsrs',
      settings: DEFAULT_DECK_SETTINGS,
      cards: [
        { front: 'hola', back: 'hello', createdAt: 1, updatedAt: 1, tags: [], suspended: false, lastAgainAt: null, scheduling: { kind: 'fsrs', stability: 0, difficulty: 0, reps: 0, lapses: 0, state: 0, step: 0, lastReview: null, due: 1 }, reviews: [] },
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
