import { expect, test } from 'vitest'
import { page, userEvent } from 'vitest/browser'
import { renderRoute } from '../../test/renderRoute'
import { freshStorage } from '../../test/seed'
import { DraftInboxPage } from './DraftInboxPage'

async function propose(storage: ReturnType<typeof freshStorage>, deckId: string, front: string) {
  await storage.proposeDrafts(
    deckId,
    [{
      front,
      back: 'Proposed answer',
      tags: ['agent'],
      rationale: 'A durable invariant.',
      sources: [{ locator: 'src/core.rs:10-20', label: 'Core implementation' }],
    }],
    { proposedBy: 'pi' },
  )
}

test('front-first triage reveals, edits, and accepts a draft into the selected deck', async () => {
  const storage = freshStorage()
  const suggested = await storage.createDeck('Suggested')
  const acceptedInto = await storage.createDeck('Accepted')
  await propose(storage, suggested.id, 'Original question')

  await renderRoute({ storage, entry: '/drafts', path: '/drafts', element: <DraftInboxPage /> })

  await expect.element(page.getByLabelText('Drafts, 1 pending')).toBeVisible()
  await expect.element(page.getByText('Original question')).toBeVisible()
  await expect.element(page.getByText('Proposed answer')).not.toBeInTheDocument()
  await expect.element(page.getByText('A durable invariant.')).not.toBeInTheDocument()

  await page.getByRole('button', { name: 'Reveal proposal' }).click()
  await expect.element(page.getByText('Proposed answer')).toBeVisible()
  await expect.element(page.getByText('A durable invariant.')).toBeVisible()
  await expect.element(page.getByText('src/core.rs:10-20')).toBeVisible()

  await page.getByLabelText('Target deck').selectOptions(acceptedInto.id)
  await page.getByRole('textbox', { name: 'Tags' }).fill('approved, concurrency')
  const front = page.getByLabelText('Draft front')
  await front.fill('Edited question')
  const back = page.getByLabelText('Draft back')
  await back.fill('Edited answer')
  await page.getByRole('button', { name: 'Accept draft' }).click()

  await expect.poll(async () => (await storage.listDrafts()).length).toBe(0)
  const cards = await storage.listCards(acceptedInto.id)
  expect(cards).toHaveLength(1)
  expect(cards[0]).toMatchObject({
    front: 'Edited question',
    back: 'Edited answer',
    tags: ['approved', 'concurrency'],
  })
  expect(await storage.listCards(suggested.id)).toEqual([])
  await expect.element(page.getByText('Inbox clear')).toBeVisible()
  await expect.element(page.getByText('Draft accepted.')).toBeVisible()
  await expect.element(page.getByLabelText('Drafts, 0 pending')).toBeVisible()
})

test('accepting a proposal that became a card removes the duplicate draft', async () => {
  const storage = freshStorage()
  const deck = await storage.createDeck('Rust')
  await propose(storage, deck.id, 'Existing question')
  await storage.createCard(deck.id, 'Existing question', 'Proposed answer', ['agent'])

  await renderRoute({ storage, entry: '/drafts', path: '/drafts', element: <DraftInboxPage /> })
  await page.getByRole('button', { name: 'Reveal proposal' }).click()
  await page.getByRole('button', { name: 'Accept draft' }).click()

  await expect.poll(async () => (await storage.listDrafts()).length).toBe(0)
  expect(await storage.listCards(deck.id)).toHaveLength(1)
  await expect.element(page.getByText('Card already existed; draft removed.')).toBeVisible()
})

test('rejecting advances to the next draft without creating a card', async () => {
  const storage = freshStorage()
  const deck = await storage.createDeck('Rust')
  await propose(storage, deck.id, 'First question')
  await propose(storage, deck.id, 'Second question')

  const ordered = await storage.listDrafts()
  await renderRoute({ storage, entry: '/drafts', path: '/drafts', element: <DraftInboxPage /> })
  await expect.element(page.getByText(ordered[0].front)).toBeVisible()
  await page.getByRole('button', { name: 'Reveal proposal' }).click()
  await userEvent.click(page.getByRole('button', { name: 'Reject draft' }))

  await expect.poll(async () => (await storage.listDrafts()).length).toBe(1)
  await expect.element(page.getByText(ordered[1].front)).toBeVisible()
  await expect.element(page.getByText('Proposed answer')).not.toBeInTheDocument()
  expect(await storage.listCards(deck.id)).toEqual([])
})
