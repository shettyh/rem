import { createBrowserRouter } from 'react-router-dom'
import { Layout } from '../ui/Layout'
import { DeckListPage } from '../features/decks/DeckListPage'
import { DeckDetailPage } from '../features/cards/DeckDetailPage'
import { CardEditorPage } from '../features/cards/CardEditorPage'
import { ReviewPage } from '../features/review/ReviewPage'
import { SettingsPage } from '../features/settings/SettingsPage'
import { DeckSettingsPage } from '../features/decks/DeckSettingsPage'
import { StatsPage } from '../features/stats/StatsPage'

export const router = createBrowserRouter([
  {
    path: '/',
    element: <Layout />,
    children: [
      { index: true, element: <DeckListPage /> },
      { path: 'decks/:deckId', element: <DeckDetailPage /> },
      { path: 'decks/:deckId/options', element: <DeckSettingsPage /> },
      { path: 'decks/:deckId/cards/new', element: <CardEditorPage /> },
      { path: 'decks/:deckId/cards/:cardId/edit', element: <CardEditorPage /> },
      { path: 'decks/:deckId/study', element: <ReviewPage /> },
      { path: 'study', element: <ReviewPage /> },
      { path: 'stats', element: <StatsPage /> },
      { path: 'settings', element: <SettingsPage /> },
    ],
  },
])
