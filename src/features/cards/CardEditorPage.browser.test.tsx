// src/features/cards/CardEditorPage.browser.test.tsx
import { describe, it, expect, afterEach, vi } from 'vitest'
import { render } from 'vitest-browser-react'
import { page, userEvent } from 'vitest/browser'
import { createMemoryRouter, RouterProvider } from 'react-router-dom'
import type { Storage } from '../../data/Storage'
import { StorageProvider } from '../../data/StorageContext'
import { CardEditorPage } from './CardEditorPage'
import { extractOcclusionDocument } from './occlusion'
import { freshStorage } from '../../test/seed'
import { renderRoute } from '../../test/renderRoute'

afterEach(async () => {
  vi.restoreAllMocks()
  await page.viewport(1280, 800)
})

function renderAt(storage: Storage, path: string) {
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
    const storage = freshStorage()
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

  it('saves and resets for the next card with Save & add another', async () => {
    const storage = freshStorage()
    const deck = await storage.createDeck('D', 'fsrs')
    const screen = await renderAt(storage, `/decks/${deck.id}/cards/new`)

    const front = screen.container.querySelector('[aria-label="Front"]') as HTMLElement
    front.focus()
    await userEvent.type(front, 'First prompt')
    await userEvent.click(screen.getByRole('button', { name: 'Save & add another' }))

    await expect.poll(async () => (await storage.listCards(deck.id)).length).toBe(1)
    await expect.element(screen.getByText('Card saved.', { exact: false })).toBeVisible()
    await expect.element(screen.getByText('New card')).toBeVisible()
    await expect.poll(() => front.textContent?.trim()).toBe('')
  })

  it('groups front and back into one surface with a flat toolbar', async () => {
    const storage = freshStorage()
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
    const storage = freshStorage()
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
    const storage = freshStorage()
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
    const storage = freshStorage()
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
    const storage = freshStorage()
    const deck = await storage.createDeck('D')
    const card = await storage.createCard(deck.id, 'Hard question', 'Answer')
    await storage.updateCard(card.id, { tags: ['leech'], suspended: true })
    const screen = await renderAt(storage, `/decks/${deck.id}/cards/${card.id}/edit`)

    await expect.element(screen.getByText('leech', { exact: true })).toBeVisible()
    await expect.element(screen.getByRole('button', { name: 'Unsuspend card' })).toBeVisible()
    expect((await storage.getTodaySummary(Date.now() + 1000)).totalDue).toBe(0)

    await userEvent.click(screen.getByRole('button', { name: 'Unsuspend card' }))

    await expect.poll(async () => (await storage.getCard(card.id))?.suspended).toBe(false)
    expect((await storage.getCard(card.id))?.tags).toEqual(['leech'])
    expect((await storage.getTodaySummary(Date.now() + 1000)).totalDue).toBe(1)
    await expect.element(screen.getByText('Active')).toBeVisible()

    await userEvent.fill(screen.getByRole('textbox', { name: 'Tags', exact: true }), 'hard, grammar')
    await userEvent.click(screen.getByText('Save card'))

    await expect.poll(async () => (await storage.getCard(card.id))?.tags).toEqual([
      'leech',
      'hard',
      'grammar',
    ])
  })

  it('opens the heading dropdown cleanly outside the horizontally scrollable toolbar', async () => {
    const storage = freshStorage()
    const deck = await storage.createDeck('Rust')
    const screen = await renderAt(storage, `/decks/${deck.id}/cards/new`)

    const front = screen.container.querySelector('[aria-label="Front"]') as HTMLElement
    front.focus()
    await userEvent.type(front, 'Heading test')

    const trigger = screen.getByRole('button', { name: 'Text style' })
    await userEvent.click(trigger)
    const menu = screen.getByRole('listbox')
    await expect.element(menu).toBeVisible()
    expect(getComputedStyle(menu.element()).position).toBe('fixed')
    expect(menu.element().getBoundingClientRect().height).toBeGreaterThan(80)

    await userEvent.click(screen.getByRole('option', { name: /Heading 1/ }))
    await expect.element(trigger).toHaveTextContent('H1')
  })

  it('supports interactive tag chips and deck tag suggestions', async () => {
    const storage = freshStorage()
    const deck = await storage.createDeck('Rust')
    await storage.createCard(deck.id, 'Existing', 'Answer', ['ownership', 'cargo'])
    const screen = await renderAt(storage, `/decks/${deck.id}/cards/new`)

    const front = screen.container.querySelector('[aria-label="Front"]') as HTMLElement
    front.focus()
    await userEvent.type(front, 'Move semantics')

    await expect.element(screen.getByRole('button', { name: '+ ownership' })).toBeVisible()
    await userEvent.click(screen.getByRole('button', { name: '+ ownership' }))
    await userEvent.type(screen.getByRole('textbox', { name: 'Tags', exact: true }), 'basics{Enter}')
    await expect.element(screen.getByRole('button', { name: 'Remove tag basics' })).toBeVisible()

    await userEvent.click(screen.getByText('Save card'))
    await expect.poll(async () => (await storage.listCards(deck.id)).length).toBe(2)
    const saved = (await storage.listCards(deck.id)).find((c) => c.front.includes('Move semantics'))
    expect(saved?.tags).toEqual(['ownership', 'basics'])
  })

  it('creates cloze sibling cards with status banner via createCardOrCloze', async () => {
    const storage = freshStorage()
    const deck = await storage.createDeck('Biology', 'fsrs')
    const createClozeSpy = vi.spyOn(storage, 'createCardOrCloze')
    const screen = await renderAt(storage, `/decks/${deck.id}/cards/new`)

    const front = screen.container.querySelector('[aria-label="Front"]') as HTMLElement
    front.focus()
    // Double braces escape `{` / `}` in userEvent.type key-descriptor syntax.
    await userEvent.type(front, 'The {{{{c1::mitochondria}}}} is the {{{{c2::powerhouse}}}} of the cell.')

    await expect
      .element(screen.getByRole('status'))
      .toHaveTextContent('Cloze note · creates 2 cards (c1, c2).')

    const back = screen.container.querySelector('[aria-label="Back"]') as HTMLElement
    back.focus()
    await userEvent.type(back, 'Produces ATP via oxidative phosphorylation.')

    await userEvent.click(screen.getByRole('button', { name: 'Save card' }))

    await expect.poll(async () => (await storage.listCards(deck.id)).length).toBe(2)
    expect(createClozeSpy).toHaveBeenCalledTimes(1)
    const cards = await storage.listCards(deck.id)
    expect(cards.map((c) => c.clozeIndex).sort()).toEqual([1, 2])
    expect(cards[0].siblingGroupId).toBeTruthy()
    expect(cards[0].siblingGroupId).toBe(cards[1].siblingGroupId)
    expect(cards[0].back).toContain('Produces ATP')
    await expect.element(screen.getByText('deck page')).toBeInTheDocument()
  })

  it('editing an existing cloze card shows sibling-update banner and calls updateClozeNote', async () => {
    const storage = freshStorage()
    const deck = await storage.createDeck('Biology', 'fsrs')
    const [c1, c2] = await storage.createCardOrCloze(
      deck.id,
      'The {{c1::mitochondria}} is the {{c2::powerhouse}}.',
      'Original extra',
      ['cells'],
    )
    const updateClozeSpy = vi.spyOn(storage, 'updateClozeNote')
    const screen = await renderAt(storage, `/decks/${deck.id}/cards/${c1.id}/edit`)

    await expect
      .element(screen.getByRole('status'))
      .toHaveTextContent('Cloze note (c1, c2) · edits update sibling cards.')

    const back = screen.container.querySelector('[aria-label="Back"]') as HTMLElement
    back.focus()
    await userEvent.type(back, ' — revised')
    await userEvent.click(screen.getByRole('button', { name: 'Save card' }))

    await expect.poll(() => updateClozeSpy.mock.calls.length).toBe(1)
    expect(updateClozeSpy).toHaveBeenCalledWith(
      c1.id,
      expect.stringContaining('{{c1::mitochondria}}'),
      expect.stringContaining('revised'),
      ['cells'],
    )
    const refreshedC1 = await storage.getCard(c1.id)
    const refreshedC2 = await storage.getCard(c2.id)
    expect(refreshedC1?.back).toContain('revised')
    expect(refreshedC2?.back).toContain('revised')
    await expect.element(screen.getByText('deck page')).toBeInTheDocument()
  })

  it('switches into Image Occlusion mode on upload, blocks save at 0 masks, and saves once a mask is drawn', async () => {
    const storage = freshStorage()
    const deck = await storage.createDeck('Anatomy', 'fsrs')
    const screen = await renderAt(storage, `/decks/${deck.id}/cards/new`)

    const svgText =
      '<svg xmlns="http://www.w3.org/2000/svg" width="400" height="240"><rect width="400" height="240" fill="#eef2ff"/><rect x="40" y="40" width="120" height="60" fill="#818cf8"/></svg>'
    const imageFile = new File([svgText], 'diagram.svg', { type: 'image/svg+xml' })

    const occlusionInput = screen.getByLabelText('Choose image for occlusion')
    await occlusionInput.upload(imageFile)

    await expect.element(screen.getByLabelText('Image Occlusion editor')).toBeVisible()
    await expect
      .element(screen.getByRole('status'))
      .toHaveTextContent('Image Occlusion · draw at least one mask on the diagram to save.')

    // 0 masks blocks saving.
    const saveBtn = screen.getByRole('button', { name: 'Save card' })
    const saveAndAddBtn = screen.getByRole('button', { name: 'Save & add another' })
    await expect.element(saveBtn).toBeDisabled()
    await expect.element(saveAndAddBtn).toBeDisabled()

    // Wait for the diagram image to load with non-zero layout bounds, then draw a rectangle mask.
    await expect
      .poll(() => {
        const img = screen.container.querySelector<HTMLImageElement>('img.io-img')
        const rect = img?.getBoundingClientRect()
        return Boolean(img?.src.startsWith('blob:') && img.complete && rect && rect.width > 40 && rect.height > 40)
      })
      .toBe(true)

    const svg = screen.container.querySelector<SVGSVGElement>('svg.io-svg')!
    const bounds = svg.getBoundingClientRect()
    svg.dispatchEvent(
      new PointerEvent('pointerdown', {
        bubbles: true,
        button: 0,
        pointerId: 1,
        clientX: bounds.left + bounds.width * 0.15,
        clientY: bounds.top + bounds.height * 0.2,
      }),
    )
    await expect.poll(() => screen.container.querySelector('.io-mask--draft')).not.toBeNull()
    svg.dispatchEvent(
      new PointerEvent('pointermove', {
        bubbles: true,
        button: 0,
        pointerId: 1,
        clientX: bounds.left + bounds.width * 0.55,
        clientY: bounds.top + bounds.height * 0.6,
      }),
    )
    await expect
      .poll(() => Number(screen.container.querySelector('.io-mask--draft')?.getAttribute('width') ?? '0'))
      .toBeGreaterThan(20)
    svg.dispatchEvent(
      new PointerEvent('pointerup', {
        bubbles: true,
        button: 0,
        pointerId: 1,
        clientX: bounds.left + bounds.width * 0.55,
        clientY: bounds.top + bounds.height * 0.6,
      }),
    )

    await expect.poll(() => screen.container.querySelector('[data-editor-mask-index="0"]')).not.toBeNull()
    await expect
      .element(screen.getByRole('status'))
      .toHaveTextContent('Image Occlusion · creates 1 card (#1).')
    await expect.element(saveBtn).toBeEnabled()

    await userEvent.click(saveBtn)
    await expect.poll(async () => (await storage.listCards(deck.id)).length).toBe(1)
    const [saved] = await storage.listCards(deck.id)
    expect(saved.clozeIndex).toBe(1)
    const doc = extractOcclusionDocument(saved.front)
    expect(doc?.block.masks).toHaveLength(1)
    await expect.element(screen.getByText('deck page')).toBeInTheDocument()
  })

  it('inserts an audio asset markdown link via the audio attachment picker', async () => {
    const storage = freshStorage()
    const deck = await storage.createDeck('Japanese', 'fsrs')
    const screen = await renderAt(storage, `/decks/${deck.id}/cards/new`)

    await expect.element(screen.getByRole('button', { name: 'Audio clip' })).toBeVisible()
    const frontField = screen.container.querySelector<HTMLElement>('.editor-field--front')!
    const audioInput = frontField.querySelector<HTMLInputElement>('input[type="file"][accept*="audio"]')!
    expect(audioInput).toBeTruthy()

    const audioFile = new File([new Uint8Array([0x49, 0x44, 0x33, 0x03, 0x00])], 'ohayo.mp3', {
      type: 'audio/mpeg',
    })
    await userEvent.upload(audioInput, audioFile)

    await expect
      .poll(() => frontField.querySelector<HTMLAnchorElement>('.rich-editor-content a')?.getAttribute('href'))
      .toMatch(/^asset:[0-9a-f]{64}$/)
    const link = frontField.querySelector<HTMLAnchorElement>('.rich-editor-content a')!
    expect(link.textContent).toBe('Audio: ohayo.mp3')
    const hash = link.getAttribute('href')!.slice('asset:'.length)

    await userEvent.click(screen.getByRole('button', { name: 'Save card' }))
    await expect.poll(async () => (await storage.listCards(deck.id)).length).toBe(1)
    const [saved] = await storage.listCards(deck.id)
    expect(saved.front).toContain(`[Audio: ohayo.mp3](asset:${hash})`)
    expect((await storage.getAsset(hash))?.mime).toBe('audio/mpeg')
  })
})
