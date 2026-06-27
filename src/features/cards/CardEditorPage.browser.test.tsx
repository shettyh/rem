// src/features/cards/CardEditorPage.browser.test.tsx
import { describe, it, expect, beforeEach } from 'vitest'
import { render } from 'vitest-browser-react'
import { userEvent } from '@vitest/browser/context'
import { createMemoryRouter, RouterProvider } from 'react-router-dom'
import Dexie from 'dexie'
import { RemDB } from '../../data/dexie/db'
import { DexieStorage } from '../../data/dexie/DexieStorage'
import { StorageProvider } from '../../data/StorageContext'
import { CardEditorPage } from './CardEditorPage'

beforeEach(async () => {
  await Dexie.delete('rem-editorpage')
})

function renderAt(storage: DexieStorage, path: string) {
  const router = createMemoryRouter(
    [
      { path: '/decks/:deckId/cards/new', element: <CardEditorPage /> },
      { path: '/decks/:deckId', element: <div>deck page</div> },
    ],
    { initialEntries: [path] },
  )
  return render(
    <StorageProvider storage={storage}>
      <RouterProvider router={router} />
    </StorageProvider>,
  )
}

describe('CardEditorPage', () => {
  it('creates a card and navigates back to the deck', async () => {
    const storage = new DexieStorage(new RemDB('rem-editorpage'))
    const deck = await storage.createDeck('D', 'fsrs')
    const screen = await renderAt(storage, `/decks/${deck.id}/cards/new`)

    const front = screen.container.querySelector('[aria-label="Front"]') as HTMLElement
    front.focus()
    await userEvent.type(front, 'Capital of France')
    await userEvent.click(screen.getByText('Save card'))

    await expect.poll(async () => (await storage.listCards(deck.id)).length).toBe(1)
    await expect.element(screen.getByText('deck page')).toBeInTheDocument()
  })
})
