// src/features/cards/CardEditorPage.browser.test.tsx
import { describe, it, expect, beforeEach, afterEach } from 'vitest'
import { render } from 'vitest-browser-react'
import { page, userEvent } from 'vitest/browser'
import { createMemoryRouter, RouterProvider } from 'react-router-dom'
import Dexie from 'dexie'
import { RemDB } from '../../data/dexie/db'
import { DexieStorage } from '../../data/dexie/DexieStorage'
import { StorageProvider } from '../../data/StorageContext'
import { CardEditorPage } from './CardEditorPage'
import { freshStorage } from '../../test/seed'
import { renderRoute } from '../../test/renderRoute'

beforeEach(async () => {
  await Dexie.delete('rem-editorpage')
})

afterEach(async () => {
  await page.viewport(1280, 800)
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

    const front = surface.querySelector<HTMLElement>('.editor-field--front .rich-editor-content')!
    const back = surface.querySelector<HTMLElement>('.editor-field--back .rich-editor-content')!
    expect(getComputedStyle(front).fontSize).toBe(getComputedStyle(back).fontSize)
    expect(getComputedStyle(front).lineHeight).toBe(getComputedStyle(back).lineHeight)
    expect(getComputedStyle(front).fontFamily).toBe(getComputedStyle(document.body).fontFamily)
  })

  it('keeps essential card field labels legible', async () => {
    const storage = new DexieStorage(new RemDB('rem-editorpage'))
    const deck = await storage.createDeck('D')
    const screen = await renderAt(storage, `/decks/${deck.id}/cards/new`)

    for (const text of ['Front', 'Back', 'Tags']) {
      const label = screen.getByText(text, { exact: true }).element()
      expect(Number.parseFloat(getComputedStyle(label).fontSize)).toBeGreaterThanOrEqual(12)
    }
  })

  it('keeps image formatting reachable at the minimum app width', async () => {
    await page.viewport(760, 540)
    const storage = freshStorage()
    const deck = await storage.createDeck('Images')
    const svg = '<svg xmlns="http://www.w3.org/2000/svg" width="320" height="120"><rect width="320" height="120" fill="#8174ff"/></svg>'
    const asset = await storage.putAsset(new TextEncoder().encode(svg), 'image/svg+xml')
    const card = await storage.createCard(deck.id, `![Diagram](asset:${asset.hash})`, 'Explanation')
    await renderRoute({
      storage,
      entry: `/decks/${deck.id}/cards/${card.id}/edit`,
      path: '/decks/:deckId/cards/:cardId/edit',
      element: <CardEditorPage />,
    })

    const image = page.getByRole('img', { name: 'Diagram' })
    await expect.element(image).toBeVisible()
    await image.click()
    await expect.poll(() => image.element().classList.contains('ProseMirror-selectednode')).toBe(true)
    const alignRight = page.getByRole('button', { name: 'Align right' })
    await expect.element(alignRight).toBeVisible()

    const content = document.querySelector<HTMLElement>('.content')!
    const toolbar = document.querySelector<HTMLElement>('.md-toolbar-bar')!
    expect(content.scrollWidth).toBe(content.clientWidth)
    expect(getComputedStyle(toolbar).overflowX).toBe('auto')

    alignRight.element().scrollIntoView({ block: 'nearest', inline: 'end' })
    await new Promise((resolve) => requestAnimationFrame(resolve))
    expect(alignRight.element().getBoundingClientRect().right)
      .toBeLessThanOrEqual(toolbar.getBoundingClientRect().right)

    await alignRight.click()
    await expect.poll(() => document.querySelector<HTMLImageElement>('.rich-editor-content img')?.dataset.align)
      .toBe('right')
  })

  it('keeps a large rich answer readable and saveable while editing', async () => {
    await page.viewport(760, 540)
    const storage = freshStorage()
    const deck = await storage.createDeck('Rich cards')
    const details = Array.from(
      { length: 10 },
      (_, index) => `Editing detail ${index + 1} remains part of the answer.`,
    ).join('\n\n')
    const back = `> A quoted principle should remain visually distinct.\n\n` +
      '```ts\nconst readable = true\n```\n\n' +
      `${details}\n\nFinal editable answer line.`
    const card = await storage.createCard(deck.id, 'How should this appear?', back)
    await renderRoute({
      storage,
      entry: `/decks/${deck.id}/cards/${card.id}/edit`,
      path: '/decks/:deckId/cards/:cardId/edit',
      element: <CardEditorPage />,
    })

    const backField = document.querySelector<HTMLElement>('.editor-field--back')!
    await expect.poll(() => backField.querySelector('blockquote')).not.toBeNull()
    const quote = backField.querySelector<HTMLElement>('blockquote')!
    const code = backField.querySelector<HTMLElement>('pre')!
    expect(Number.parseFloat(getComputedStyle(quote).borderLeftWidth)).toBeGreaterThanOrEqual(2)
    expect(Number.parseFloat(getComputedStyle(code).fontSize)).toBeGreaterThanOrEqual(16)

    const save = page.getByRole('button', { name: 'Save card' })
    const closing = page.getByText('Final editable answer line.', { exact: true })
    closing.element().scrollIntoView({ block: 'end' })
    await new Promise((resolve) => requestAnimationFrame(resolve))

    await expect.element(save).toBeVisible()
    expect(closing.element().getBoundingClientRect().bottom).toBeLessThanOrEqual(window.innerHeight)
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
