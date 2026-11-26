import { test, expect } from 'vitest'
import { page, userEvent } from 'vitest/browser'
import { useParams } from 'react-router-dom'
import { freshStorage } from '../test/seed'
import { renderRoute } from '../test/renderRoute'

function EditorRouteStub() {
  const { deckId, cardId } = useParams()
  return <div data-testid="card-editor-route">{`${deckId}/${cardId}`}</div>
}

test('global search opens via sidebar button, finds cards across decks, and navigates to edit', async () => {
  const storage = freshStorage()
  const rust = await storage.createDeck('Rust')
  const sql = await storage.createDeck('SQL')
  const target = await storage.createCard(
    rust.id,
    'What does Pin<P> guarantee?',
    'The pointee cannot be moved in memory.',
    ['async'],
  )
  await storage.createCard(sql.id, 'What is WAL mode?', 'Write-Ahead Logging.')

  await renderRoute({
    storage,
    entry: '/',
    path: '/',
    element: <div>Today home</div>,
    extraRoutes: [
      {
        path: '/decks/:deckId/cards/:cardId/edit',
        element: <EditorRouteStub />,
      },
    ],
  })

  await expect.element(page.getByText('Today home')).toBeVisible()
  await page.getByRole('button', { name: 'Search', exact: false }).click()

  const input = page.getByRole('searchbox', { name: 'Search all cards' })
  await expect.element(input).toBeVisible()
  await userEvent.type(input.element(), 'Pin')

  await expect.element(page.getByText('What does Pin<P> guarantee?')).toBeVisible()
  await expect.element(page.getByText('What is WAL mode?')).not.toBeInTheDocument()

  await page.getByText('What does Pin<P> guarantee?').click()
  await expect.element(page.getByTestId('card-editor-route')).toHaveTextContent(`${rust.id}/${target.id}`)
})
