import { createBrowserRouter } from 'react-router-dom'
import { Layout } from '../ui/Layout'
import { DeckListPage } from '../features/decks/DeckListPage'
import { DeckDetailPage } from '../features/cards/DeckDetailPage'
import { ReviewPage } from '../features/review/ReviewPage'
import { SettingsPage } from '../features/settings/SettingsPage'

export const router = createBrowserRouter([
  {
    path: '/',
    element: <Layout />,
    children: [
      { index: true, element: <DeckListPage /> },
      { path: 'decks/:deckId', element: <DeckDetailPage /> },
      { path: 'decks/:deckId/study', element: <ReviewPage /> },
      { path: 'study', element: <ReviewPage /> },
      { path: 'settings', element: <SettingsPage /> },
    ],
  },
])
