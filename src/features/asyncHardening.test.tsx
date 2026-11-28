import { afterEach, describe, expect, it, vi } from 'vitest'
import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { createMemoryRouter, RouterProvider } from 'react-router-dom'
import { StorageProvider } from '../data/StorageContext'
import { SyncProvider } from '../data/SyncContext'
import { DEFAULT_DECK_SETTINGS, MS_PER_DAY, type CardSearchHit, type StudyView } from '../domain/models'
import { DEFAULT_FSRS_WEIGHTS, getFsrsOptimizer } from '../domain/scheduler/optimizer'
import { serializeBackup, type DeckBackup } from '../data/backup'
import { deferred } from '../test/prop'
import { freshStorage, type MemoryStorage } from '../test/seed'
import { ReviewPage } from './review/ReviewPage'
import { CardEditorPage } from './cards/CardEditorPage'
import { DeckSettingsPage } from './decks/DeckSettingsPage'
import { DraftInboxPage } from './drafts/DraftInboxPage'
import { SettingsPage } from './settings/SettingsPage'
import { SearchModal } from '../ui/SearchModal'

async function makeStorage(): Promise<{ storage: MemoryStorage; cleanupDb: () => Promise<void> }> {
  return {
    storage: freshStorage(),
    cleanupDb: async () => {},
  }
}

afterEach(() => {
  cleanup()
  vi.restoreAllMocks()
})

describe('deterministic async-interleaving guardrails', () => {
  it('ReviewPage renders a recoverable error state when startStudy rejects', async () => {
    const { storage, cleanupDb } = await makeStorage()
    try {
      const deck = await storage.createDeck('Broken')
      vi.spyOn(storage, 'startStudy').mockRejectedValueOnce(new Error('SQLite busy'))
      vi.spyOn(console, 'error').mockImplementation(() => {})

      const router = createMemoryRouter(
        [
          { path: '/decks/:deckId/study', element: <ReviewPage /> },
          { path: '/decks/:deckId', element: <div>Deck Home</div> },
        ],
        { initialEntries: [`/decks/${deck.id}/study`] },
      )
      render(
        <StorageProvider storage={storage}>
          <RouterProvider router={router} />
        </StorageProvider>,
      )

      await waitFor(() => expect(screen.getByRole('alert')).toHaveTextContent('SQLite busy'))
      expect(screen.getByRole('link', { name: 'Back to deck' })).toBeInTheDocument()
    } finally {
      await cleanupDb()
    }
  })

  it('ReviewPage ignores a stale revealStudy resolution after unmount or session reset and preserves canUndo on transient undo error', async () => {
    const { storage, cleanupDb } = await makeStorage()
    try {
      const deck = await storage.createDeck('Races')
      const card = await storage.createCard(deck.id, 'First prompt', 'First answer')
      await storage.updateCard(card.id, {
        scheduling: {
          kind: 'fsrs',
          stability: 10,
          difficulty: 5,
          reps: 2,
          lapses: 0,
          state: 2,
          step: 0,
          lastReview: 1,
          due: 0,
        },
      })
      vi.spyOn(console, 'error').mockImplementation(() => {})

      const revealGate = deferred<StudyView>()
      const origReveal = storage.revealStudy.bind(storage)
      vi.spyOn(storage, 'revealStudy').mockImplementationOnce(() => revealGate.promise)

      const router = createMemoryRouter(
        [{ path: '/decks/:deckId/study', element: <ReviewPage /> }],
        { initialEntries: [`/decks/${deck.id}/study`] },
      )
      render(
        <StorageProvider storage={storage}>
          <RouterProvider router={router} />
        </StorageProvider>,
      )

      const showBtn = await screen.findByRole('button', { name: /Show answer/i })
      // Two synchronous clicks while revealStudy is pending must only invoke revealStudy once.
      fireEvent.click(showBtn)
      fireEvent.click(showBtn)
      expect(storage.revealStudy).toHaveBeenCalledTimes(1)

      // Suspend click while reveal is in-flight is blocked by shared inFlightRef.
      const suspendSpy = vi.spyOn(storage, 'suspendStudyCard')
      fireEvent.click(screen.getByRole('button', { name: 'Suspend' }))
      expect(suspendSpy).not.toHaveBeenCalled()

      // Finish reveal cleanly and grade the card.
      const realRevealed = await origReveal((storage.revealStudy as ReturnType<typeof vi.fn>).mock.calls[0][0])
      revealGate.resolve(realRevealed)
      const goodBtn = await screen.findByRole('button', { name: /Good/i })
      fireEvent.click(goodBtn)
      await screen.findByText('Review complete')

      // Transient undo error must keep Undo button available for retry.
      const origUndo = storage.undoStudyGrade.bind(storage)
      vi.spyOn(storage, 'undoStudyGrade')
        .mockRejectedValueOnce(new Error('temporary disk lock'))
        .mockImplementationOnce((sid) => origUndo(sid))

      const undoBtn = screen.getByRole('button', { name: 'Undo last grade' })
      fireEvent.click(undoBtn)
      await waitFor(() => expect(screen.getByRole('status')).toHaveTextContent('Could not undo last grade.'))
      expect(screen.getByRole('button', { name: 'Undo last grade' })).toBeEnabled()

      // Retry succeeds and restores the card.
      fireEvent.click(screen.getByRole('button', { name: 'Undo last grade' }))
      await screen.findByText('First prompt')
    } finally {
      await cleanupDb()
    }
  })

  it('CardEditorPage prevents duplicate saves on rapid double-submit and surfaces errors', async () => {
    const { storage, cleanupDb } = await makeStorage()
    try {
      const deck = await storage.createDeck('Editor')
      const card = await storage.createCard(deck.id, 'Initial Q', 'Initial A')
      vi.spyOn(console, 'error').mockImplementation(() => {})

      const updateGate = deferred<void>()
      const updateSpy = vi.spyOn(storage, 'updateCard').mockImplementationOnce(() => updateGate.promise)

      const editRouter = createMemoryRouter(
        [
          { path: '/decks/:deckId/cards/:cardId/edit', element: <CardEditorPage /> },
          { path: '/decks/:deckId', element: <div>Deck Page</div> },
        ],
        { initialEntries: [`/decks/${deck.id}/cards/${card.id}/edit`] },
      )
      render(
        <StorageProvider storage={storage}>
          <RouterProvider router={editRouter} />
        </StorageProvider>,
      )

      const saveBtn = await screen.findByRole('button', { name: 'Save card' })
      await waitFor(() => expect(saveBtn).toBeEnabled())

      // Fire two rapid saves (click + ⌘⏎) before the first promise settles.
      fireEvent.click(saveBtn)
      fireEvent.keyDown(window, { key: 'Enter', metaKey: true })
      await waitFor(() => expect(updateSpy).toHaveBeenCalledTimes(1))

      // Reject first save -> surfaces error alert and re-enables saving.
      updateGate.reject(new Error('disk full'))
      await waitFor(() => expect(screen.getByRole('alert')).toHaveTextContent('disk full'))
      expect(screen.getByRole('button', { name: 'Save card' })).toBeEnabled()
    } finally {
      await cleanupDb()
    }
  })

  it('DeckSettingsPage preserves concurrent setting edits made while FSRS optimization is in flight', async () => {
    const { storage, cleanupDb } = await makeStorage()
    try {
      const deck = await storage.createDeck('OptimizeRace')
      const card = await storage.createCard(deck.id, 'Q', 'A')
      storage.addReviewLog({ cardId: card.id, deckId: deck.id, reviewedAt: 0, grade: 'good' })
      storage.addReviewLog({ cardId: card.id, deckId: deck.id, reviewedAt: MS_PER_DAY, grade: 'good' })

      const optGate = deferred<number[]>()
      vi.spyOn(getFsrsOptimizer(), 'optimize').mockReturnValueOnce(optGate.promise)

      const router = createMemoryRouter(
        [{ path: '/decks/:deckId/options', element: <DeckSettingsPage /> }],
        { initialEntries: [`/decks/${deck.id}/options`] },
      )
      render(
        <StorageProvider storage={storage}>
          <RouterProvider router={router} />
        </StorageProvider>,
      )

      const optBtn = await screen.findByRole('button', { name: 'Optimize' })
      await waitFor(() => expect(optBtn).toBeEnabled())
      fireEvent.click(optBtn)
      await screen.findByRole('button', { name: 'Optimizing…' })

      // Edit another setting while optimize() is still in flight.
      fireEvent.click(screen.getByLabelText('Increase New cards/day'))
      await waitFor(async () => expect((await storage.getDeck(deck.id))?.settings.newPerDay).toBe(25))

      // Resolve optimize() — must preserve newPerDay: 25 AND persist personalized weights!
      optGate.resolve([...DEFAULT_FSRS_WEIGHTS])
      await waitFor(async () => {
        const latest = await storage.getDeck(deck.id)
        expect(latest?.settings.fsrsWeights).toEqual([...DEFAULT_FSRS_WEIGHTS])
        expect(latest?.settings.newPerDay).toBe(25)
      })
    } finally {
      await cleanupDb()
    }
  })

  it('SearchModal avoids pre-effect "No cards match" flash and drops out-of-order stale search responses', async () => {
    const { storage, cleanupDb } = await makeStorage()
    try {
      const deck = await storage.createDeck('Search')
      const cardA = await storage.createCard(deck.id, 'Alpha item', 'A')
      const cardB = await storage.createCard(deck.id, 'Beta item', 'B')

      const q1 = deferred<CardSearchHit[]>()
      const q2 = deferred<CardSearchHit[]>()
      const searchSpy = vi
        .spyOn(storage, 'searchCards')
        .mockImplementationOnce(() => q1.promise)
        .mockImplementationOnce(() => q2.promise)

      const onClose = vi.fn()
      const router = createMemoryRouter(
        [
          { path: '/', element: <SearchModal open={true} onClose={onClose} /> },
          { path: '/decks/:deckId/cards/:cardId/edit', element: <div data-testid="opened">opened</div> },
        ],
        { initialEntries: ['/'] },
      )
      render(
        <StorageProvider storage={storage}>
          <RouterProvider router={router} />
        </StorageProvider>,
      )

      const input = screen.getByRole('combobox', { name: 'Search all cards' })
      fireEvent.change(input, { target: { value: 'Alpha' } })

      // Immediately after typing (before debounce/IPC settles), must show Searching…, never "No results for".
      expect(screen.getByText('Searching…')).toBeInTheDocument()
      expect(screen.queryByText(/No results for/i)).not.toBeInTheDocument()

      // Wait for debounce to dispatch query 1.
      await waitFor(() => expect(searchSpy).toHaveBeenCalledTimes(1))

      // Type second query before query 1 resolves.
      fireEvent.change(input, { target: { value: 'Beta' } })
      await waitFor(() => expect(searchSpy).toHaveBeenCalledTimes(2))

      // Resolve query 2 FIRST, then resolve stale query 1 AFTER.
      q2.resolve([{ card: cardB, deckName: deck.name, deckColor: deck.color }])
      await screen.findByRole('option', { name: /Beta item/ })

      q1.resolve([{ card: cardA, deckName: deck.name, deckColor: deck.color }])
      await Promise.resolve()

      // Stale query 1 must NOT overwrite query 2's results.
      expect(screen.getByRole('option', { name: /Beta item/ })).toBeInTheDocument()
      expect(screen.queryByRole('option', { name: /Alpha item/ })).not.toBeInTheDocument()
    } finally {
      await cleanupDb()
    }
  })

  it('SettingsPage prompts replace on whitespace-variant deck names and surfaces importDecks errors', async () => {
    const { storage, cleanupDb } = await makeStorage()
    try {
      await storage.createDeck('Spanish')
      const incoming: DeckBackup[] = [{
        name: '  Spanish  ',
        createdAt: 1,
        schedulerKind: 'fsrs',
        settings: DEFAULT_DECK_SETTINGS,
        cards: [],
      }]
      const file = new File([serializeBackup(incoming, 1000)], 'backup.json', { type: 'application/json' })

      const router = createMemoryRouter(
        [{ path: '/settings', element: <SettingsPage /> }],
        { initialEntries: ['/settings'] },
      )
      render(
        <StorageProvider storage={storage}>
          <SyncProvider autoSync={false}>
            <RouterProvider router={router} />
          </SyncProvider>
        </StorageProvider>,
      )

      // Wait for decks query to load so existing deck names are known.
      await waitFor(() => expect(screen.getByRole('button', { name: 'Export all' })).toBeEnabled())

      const fileInput = screen.getByLabelText('Import backup file') as HTMLInputElement
      fireEvent.change(fileInput, { target: { files: [file] } })

      // Must prompt Replace instead of silently overwriting.
      await screen.findByText(/will be replaced/i)

      vi.spyOn(storage, 'importDecks').mockRejectedValueOnce(new Error('storage write failed'))
      fireEvent.click(screen.getByRole('button', { name: 'Replace' }))
      await waitFor(() => expect(screen.getByRole('alert')).toHaveTextContent('storage write failed'))
    } finally {
      await cleanupDb()
    }
  })

  it('DraftInboxPage prevents duplicate resolveDraft calls on same-tick double clicks', async () => {
    const { storage, cleanupDb } = await makeStorage()
    try {
      const deck = await storage.createDeck('Drafts')
      await storage.proposeDrafts(
        deck.id,
        [{ front: 'Draft Q', back: 'Draft A', tags: [], rationale: null, sources: [] }],
        { proposedBy: 'agent' },
      )

      const gate = deferred<{ status: 'rejected' }>()
      const resolveSpy = vi.spyOn(storage, 'resolveDraft').mockImplementationOnce(() => gate.promise)

      const router = createMemoryRouter(
        [{ path: '/drafts', element: <DraftInboxPage /> }],
        { initialEntries: ['/drafts'] },
      )
      render(
        <StorageProvider storage={storage}>
          <RouterProvider router={router} />
        </StorageProvider>,
      )

      fireEvent.click(await screen.findByRole('button', { name: /Reveal proposal/i }))
      const rejectBtn = await screen.findByRole('button', { name: 'Reject draft' })
      act(() => {
        fireEvent.click(rejectBtn)
        fireEvent.click(rejectBtn)
      })
      expect(resolveSpy).toHaveBeenCalledTimes(1)
      expect(rejectBtn).toBeDisabled()
      await act(async () => {
        gate.resolve({ status: 'rejected' })
        await gate.promise
      })
      expect(screen.getByRole('status')).toHaveTextContent('Draft rejected.')
      expect(screen.getByRole('button', { name: 'Reject draft' })).toBeEnabled()
    } finally {
      await cleanupDb()
    }
  })
})
