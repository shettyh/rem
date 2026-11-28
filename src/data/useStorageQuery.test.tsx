import { render, screen, waitFor } from '@testing-library/react'
import { beforeEach, describe, expect, it } from 'vitest'
import { DEFAULT_DECK_SETTINGS } from '../domain/models'
import { freshStorage, type MemoryStorage } from '../test/seed'
import { StorageProvider, useStorage } from './StorageContext'
import { useStorageQuery } from './useStorageQuery'

let storage: MemoryStorage

beforeEach(() => {
  storage = freshStorage()
})

function DeckCount() {
  const current = useStorage()
  const decks = useStorageQuery(() => current.listDecks(), [current])
  return <span>{decks === undefined ? 'loading' : String(decks.length)}</span>
}

function DeckNames() {
  const current = useStorage()
  const decks = useStorageQuery(() => current.listDecks(), [current])
  return <span data-testid="names">{decks?.map((deck) => deck.name).join(',') ?? 'loading'}</span>
}

describe('useStorageQuery', () => {
  it('re-runs after successful mutations through the storage adapter', async () => {
    render(
      <StorageProvider storage={storage}>
        <DeckCount />
      </StorageProvider>,
    )
    await waitFor(() => expect(screen.getByText('0')).toBeInTheDocument())

    await storage.createDeck('Rust')

    await waitFor(() => expect(screen.getByText('1')).toBeInTheDocument())
  })

  it('invalidates after update, delete, and import mutations', async () => {
    render(
      <StorageProvider storage={storage}>
        <DeckNames />
      </StorageProvider>,
    )
    await waitFor(() => expect(screen.getByTestId('names').textContent).toBe(''))

    const deck = await storage.createDeck('Rust')
    await waitFor(() => expect(screen.getByTestId('names')).toHaveTextContent('Rust'))

    await storage.updateDeck(deck.id, { name: 'Rust language' })
    await waitFor(() => expect(screen.getByTestId('names')).toHaveTextContent('Rust language'))

    await storage.deleteDeck(deck.id)
    await waitFor(() => expect(screen.getByTestId('names').textContent).toBe(''))

    await storage.importDecks([{
      name: 'Imported',
      createdAt: 1,
      schedulerKind: 'fsrs',
      settings: DEFAULT_DECK_SETTINGS,
      cards: [],
    }])
    await waitFor(() => expect(screen.getByTestId('names')).toHaveTextContent('Imported'))
  })
})
