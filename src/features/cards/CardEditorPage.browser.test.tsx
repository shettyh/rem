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
      { path: '/decks/:deckId/cards/:cardId/edit', element: <CardEditorPage /> },
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

    await expect.element(screen.getByText('D', { exact: true })).toBeVisible()
    const front = screen.container.querySelector('[aria-label="Front"]') as HTMLElement
    front.focus()
    await userEvent.type(front, 'Capital of France')
    await userEvent.type(screen.getByRole('textbox', { name: 'Tags', exact: true }), 'geography, capitals')
    await userEvent.click(screen.getByText('Save card'))

    await expect.poll(async () => (await storage.listCards(deck.id)).length).toBe(1)
    expect((await storage.listCards(deck.id))[0].tags).toEqual(['geography', 'capitals'])
    await expect.element(screen.getByText('deck page')).toBeInTheDocument()
  })

  it('groups front and back into one surface with a flat toolbar', async () => {
    const storage = new DexieStorage(new RemDB('rem-editorpage'))
    const deck = await storage.createDeck('D')
    const screen = await renderAt(storage, `/decks/${deck.id}/cards/new`)

    await expect.element(screen.getByRole('toolbar', { name: 'Formatting' })).toBeVisible()
    const surface = screen.container.querySelector<HTMLElement>('.card-editor-surface')!
    expect(surface).toBeTruthy()
    expect(surface.querySelectorAll('.rich-editor')).toHaveLength(2)

    const toolbar = screen.container.querySelector<HTMLElement>('.md-toolbar')!
    expect(getComputedStyle(toolbar).borderTopWidth).toBe('0px')
    for (const editor of surface.querySelectorAll<HTMLElement>('.rich-editor')) {
      expect(getComputedStyle(editor).borderTopWidth).toBe('0px')
      expect(getComputedStyle(editor).borderRadius).toBe('0px')
    }
  })

  it('warns before discarding unsaved changes', async () => {
    const storage = new DexieStorage(new RemDB('rem-editorpage'))
    const deck = await storage.createDeck('D')
    const screen = await renderAt(storage, `/decks/${deck.id}/cards/new`)
    const front = screen.container.querySelector('[aria-label="Front"]') as HTMLElement
    front.focus()
    await userEvent.type(front, 'Unsaved question')

    await userEvent.click(screen.getByRole('button', { name: 'Cancel' }))
    await expect.element(screen.getByRole('heading', { name: 'Discard unsaved changes?' })).toBeVisible()
    await userEvent.click(screen.getByRole('button', { name: 'Keep editing' }))
    await expect.element(screen.getByText('New card')).toBeVisible()

    await userEvent.click(screen.getByRole('button', { name: 'Cancel' }))
    await userEvent.click(screen.getByRole('button', { name: 'Discard changes' }))
    await expect.element(screen.getByText('deck page')).toBeInTheDocument()
    expect(await storage.listCards(deck.id)).toHaveLength(0)
  })

  it('confirms before permanently deleting a card', async () => {
    const storage = new DexieStorage(new RemDB('rem-editorpage'))
    const deck = await storage.createDeck('D')
    const card = await storage.createCard(deck.id, 'Question', 'Answer')
    const screen = await renderAt(storage, `/decks/${deck.id}/cards/${card.id}/edit`)
    await expect.element(screen.getByText('Question')).toBeVisible()

    await userEvent.click(screen.getByRole('button', { name: 'Delete card' }))
    await expect.element(screen.getByRole('heading', { name: 'Delete this card?' })).toBeVisible()
    expect(await storage.getCard(card.id)).toBeDefined()
    await userEvent.click(screen.getByRole('button', { name: 'Keep card' }))

    await userEvent.click(screen.getByRole('button', { name: 'Delete card' }))
    await userEvent.click(screen.getByRole('button', { name: 'Delete permanently' }))
    await expect.poll(async () => storage.getCard(card.id)).toBeUndefined()
    await expect.element(screen.getByText('deck page')).toBeInTheDocument()
  })

  it('shows a suspended leech and restores it without removing the tag', async () => {
    const storage = new DexieStorage(new RemDB('rem-editorpage'))
    const deck = await storage.createDeck('D')
    const card = await storage.createCard(deck.id, 'Hard question', 'Answer')
    await storage.updateCard(card.id, { tags: ['leech'], suspended: true })
    const screen = await renderAt(storage, `/decks/${deck.id}/cards/${card.id}/edit`)

    await expect.element(screen.getByText('leech', { exact: true })).toBeVisible()
    await expect.element(screen.getByRole('button', { name: 'Unsuspend card' })).toBeVisible()
    expect(await storage.countDue(deck.id, Date.now() + 1000)).toBe(0)

    await userEvent.click(screen.getByRole('button', { name: 'Unsuspend card' }))

    await expect.poll(async () => (await storage.getCard(card.id))?.suspended).toBe(false)
    expect((await storage.getCard(card.id))?.tags).toEqual(['leech'])
    expect(await storage.countDue(deck.id, Date.now() + 1000)).toBe(1)
    await expect.element(screen.getByText('Active')).toBeVisible()

    await userEvent.fill(screen.getByRole('textbox', { name: 'Tags', exact: true }), 'hard, grammar')
    await userEvent.click(screen.getByText('Save card'))

    await expect.poll(async () => (await storage.getCard(card.id))?.tags).toEqual([
      'leech',
      'hard',
      'grammar',
    ])
  })
})
