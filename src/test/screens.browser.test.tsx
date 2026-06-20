import { test, expect } from 'vitest'
import { page } from 'vitest/browser'
import { DeckListPage } from '../features/decks/DeckListPage'
import { freshStorage } from './seed'
import { renderRoute } from './renderRoute'
import { shoot } from './screenshot'

test('deck list — with decks', async () => {
  const storage = freshStorage()
  await storage.createDeck('TypeScript')
  await storage.createDeck('Spanish vocabulary')

  renderRoute({ storage, entry: '/', path: '/', element: <DeckListPage /> })

  await expect.element(page.getByText('TypeScript')).toBeVisible()
  await shoot('screen', 'deck-list')
})
