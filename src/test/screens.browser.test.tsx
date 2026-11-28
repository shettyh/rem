import { test, expect, afterEach } from 'vitest'
import { page, userEvent } from 'vitest/browser'
import { DeckListPage } from '../features/decks/DeckListPage'
import { DeckDetailPage } from '../features/cards/DeckDetailPage'
import { CardEditorPage } from '../features/cards/CardEditorPage'
import { composeOcclusionDocument } from '../features/cards/occlusion'
import { ReviewPage } from '../features/review/ReviewPage'
import { SettingsPage } from '../features/settings/SettingsPage'
import { StatsPage } from '../features/stats/StatsPage'
import { DraftInboxPage } from '../features/drafts/DraftInboxPage'
import { freshStorage, MS_PER_DAY } from './seed'
import { renderRoute } from './renderRoute'
import { shoot } from './screenshot'

const TINY_WAV = new Uint8Array([
  0x52, 0x49, 0x46, 0x46, 0x26, 0x00, 0x00, 0x00, 0x57, 0x41, 0x56, 0x45,
  0x66, 0x6d, 0x74, 0x20, 0x10, 0x00, 0x00, 0x00, 0x01, 0x00, 0x01, 0x00,
  0x44, 0xac, 0x00, 0x00, 0x88, 0x58, 0x01, 0x00, 0x02, 0x00, 0x10, 0x00,
  0x64, 0x61, 0x74, 0x61, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00,
])

const TCP_DIAGRAM_SVG = new TextEncoder().encode(
  `<svg xmlns="http://www.w3.org/2000/svg" width="640" height="320" viewBox="0 0 640 320">
    <rect width="640" height="320" rx="12" fill="#f4f6fb"/>
    <text x="320" y="38" text-anchor="middle" font-family="system-ui, sans-serif" font-size="18" font-weight="600" fill="#1e293b">TCP Segment Header (32-bit words)</text>
    <rect x="32" y="60" width="280" height="58" rx="6" fill="#dbeafe" stroke="#3b82f6" stroke-width="2"/>
    <text x="172" y="95" text-anchor="middle" font-family="system-ui, sans-serif" font-size="15" font-weight="600" fill="#1e3a8a">Source Port (16 bits)</text>
    <rect x="328" y="60" width="280" height="58" rx="6" fill="#dbeafe" stroke="#3b82f6" stroke-width="2"/>
    <text x="468" y="95" text-anchor="middle" font-family="system-ui, sans-serif" font-size="15" font-weight="600" fill="#1e3a8a">Destination Port (16 bits)</text>
    <rect x="32" y="132" width="576" height="58" rx="6" fill="#e0e7ff" stroke="#6366f1" stroke-width="2"/>
    <text x="320" y="167" text-anchor="middle" font-family="system-ui, sans-serif" font-size="15" font-weight="600" fill="#312e81">Sequence Number (32 bits)</text>
    <rect x="32" y="204" width="200" height="58" rx="6" fill="#fef3c7" stroke="#d97706" stroke-width="2"/>
    <text x="132" y="239" text-anchor="middle" font-family="system-ui, sans-serif" font-size="15" font-weight="600" fill="#78350f">Flags (SYN / ACK / FIN)</text>
    <rect x="248" y="204" width="360" height="58" rx="6" fill="#dcfce7" stroke="#16a34a" stroke-width="2"/>
    <text x="428" y="239" text-anchor="middle" font-family="system-ui, sans-serif" font-size="15" font-weight="600" fill="#14532d">Receive Window Size (16 bits)</text>
  </svg>`,
)

async function waitForOcclusionImageLoaded() {
  await expect
    .poll(() => {
      const img = document.querySelector<HTMLImageElement>('img.io-img')
      return Boolean(img && img.src.startsWith('blob:') && img.complete && img.naturalWidth > 0)
    })
    .toBe(true)
}

const CODE_BACK = 'Use a guard:\n\n```ts\nfunction f(x: unknown) {\n  if (typeof x === "string") return x\n}\n```'
const RICH_EDITOR_BACK = `## Readable answer

> Preserve hierarchy when an answer mixes several kinds of content.

- Keep prose at a comfortable measure.
- Keep grading controls available.
- Let wide content scroll locally.

\`\`\`ts
function normalize(input: string): string {
  return input.trim().toLowerCase()
}
\`\`\`

[Read the WCAG text-spacing guidance](https://www.w3.org/WAI/WCAG22/Understanding/text-spacing.html)`
const RICH_BACK = `${RICH_EDITOR_BACK}

| Content | Treatment |
| --- | --- |
| Prose | Comfortable reading size |
| Code | Locally scrollable block |`

type Storage = ReturnType<typeof freshStorage>

async function pushToFuture(storage: Storage, cardId: string) {
  const card = await storage.getCard(cardId)
  if (!card) throw new Error('seed card missing')
  await storage.updateCard(cardId, { scheduling: { ...card.scheduling, due: Date.now() + 10 * MS_PER_DAY } })
}

const scenarios: { name: string; run: () => Promise<void> }[] = [
  {
    name: 'deck-list',
    run: async () => {
      const storage = freshStorage()
      await storage.createDeck('TypeScript')
      await storage.createDeck('Spanish vocabulary')
      await renderRoute({ storage, entry: '/', path: '/', element: <DeckListPage /> })
      await expect.element(page.getByText('Your decks')).toBeVisible()
    },
  },
  {
    name: 'deck-list-due',
    run: async () => {
      const storage = freshStorage()
      const deck = await storage.createDeck('TypeScript')
      await storage.createCard(deck.id, 'What is the `never` type?', 'The empty type.')
      await renderRoute({ storage, entry: '/', path: '/', element: <DeckListPage /> })
      await expect.element(page.getByLabelText('Review summary')).toBeVisible()
    },
  },
  {
    name: 'deck-list-empty',
    run: async () => {
      const storage = freshStorage()
      await renderRoute({ storage, entry: '/', path: '/', element: <DeckListPage /> })
      await expect.element(page.getByText('No decks yet', { exact: false })).toBeVisible()
    },
  },
  {
    name: 'deck-detail',
    run: async () => {
      const storage = freshStorage()
      const deck = await storage.createDeck('TypeScript')
      await storage.createCard(deck.id, 'What is the `never` type?', 'The empty type.', ['types'])
      await storage.createCard(deck.id, 'What does `satisfies` do?', 'Checks without widening.', ['syntax'])
      await renderRoute({ storage, entry: `/decks/${deck.id}`, path: '/decks/:deckId', element: <DeckDetailPage /> })
      await expect.element(page.getByText('Study', { exact: false })).toBeVisible()
    },
  },
  {
    name: 'deck-detail-nothing-due',
    run: async () => {
      const storage = freshStorage()
      const deck = await storage.createDeck('TypeScript')
      const card = await storage.createCard(deck.id, 'Reviewed already', 'Yes')
      await pushToFuture(storage, card.id)
      await renderRoute({ storage, entry: `/decks/${deck.id}`, path: '/decks/:deckId', element: <DeckDetailPage /> })
      await expect.element(page.getByText('Caught up', { exact: false })).toBeVisible()
    },
  },
  {
    name: 'deck-detail-empty',
    run: async () => {
      const storage = freshStorage()
      const deck = await storage.createDeck('Empty deck')
      await renderRoute({ storage, entry: `/decks/${deck.id}`, path: '/decks/:deckId', element: <DeckDetailPage /> })
      await expect.element(page.getByText('No cards yet', { exact: false })).toBeVisible()
    },
  },
  {
    name: 'settings',
    run: async () => {
      const storage = freshStorage()
      await storage.createDeck('TypeScript')
      await storage.createDeck('Spanish vocabulary')
      await renderRoute({ storage, entry: '/settings', path: '/settings', element: <SettingsPage /> })
      await expect.element(page.getByRole('heading', { name: 'Settings' })).toBeVisible()
      await expect.element(page.getByLabelText('Settings')).toBeVisible() // header gear link
    },
  },
  {
    name: 'stats',
    run: async () => {
      const storage = freshStorage()
      const typescript = await storage.createDeck('TypeScript')
      const spanish = await storage.createDeck('Spanish vocabulary')
      const now = Date.now()
      storage.setStats({
        historyCount: 3,
        totalReviews: 3,
        recallRate: 2 / 3,
        currentStreakDays: 2,
        activeDays: 2,
        grades: { again: 1, hard: 0, good: 1, easy: 1 },
        daily: Array.from({ length: 30 }, (_, idx) => ({
          day: `day-${idx}`,
          timestampMs: now - (29 - idx) * MS_PER_DAY,
          count: idx === 29 ? 2 : idx === 28 ? 1 : 0,
        })),
        byDeck: [
          { deckId: typescript.id, deckName: typescript.name, deckColor: typescript.color, reviews: 2, recallRate: 0.5 },
          { deckId: spanish.id, deckName: spanish.name, deckColor: spanish.color, reviews: 1, recallRate: 1 },
        ],
        upcoming7d: [],
      })
      await renderRoute({ storage, entry: '/stats', path: '/stats', element: <StatsPage /> })
      await expect.element(page.getByLabelText('FSRS reviews')).toMatchTextContent('3')
    },
  },
  {
    name: 'draft-inbox-question',
    run: async () => {
      const storage = freshStorage()
      const deck = await storage.createDeck('TypeScript')
      await storage.proposeDrafts(deck.id, [{
        front: 'Why does optimistic concurrency protect review commits?',
        back: 'It prevents two open clients from grading the same stale card.',
        tags: ['concurrency', 'review'],
        rationale: 'This is a durable correctness invariant.',
        sources: [{ locator: 'src-tauri/crates/rem-core/src/review_store.rs', label: 'Review store' }],
      }], { proposedBy: 'pi' })
      await renderRoute({ storage, entry: '/drafts', path: '/drafts', element: <DraftInboxPage /> })
      await expect.element(page.getByRole('button', { name: 'Reveal proposal' })).toBeVisible()
    },
  },
  {
    name: 'draft-inbox-revealed',
    run: async () => {
      const storage = freshStorage()
      const deck = await storage.createDeck('TypeScript')
      await storage.proposeDrafts(deck.id, [{
        front: 'Why does optimistic concurrency protect review commits?',
        back: 'It prevents two open clients from grading the same stale card.',
        tags: ['concurrency', 'review'],
        rationale: 'This is a durable correctness invariant.',
        sources: [{ locator: 'src-tauri/crates/rem-core/src/review_store.rs', label: 'Review store' }],
      }], { proposedBy: 'pi' })
      await renderRoute({ storage, entry: '/drafts', path: '/drafts', element: <DraftInboxPage /> })
      await page.getByRole('button', { name: 'Reveal proposal' }).click()
      await expect.element(page.getByRole('button', { name: 'Accept draft' })).toBeVisible()
    },
  },
  {
    name: 'card-editor-new',
    run: async () => {
      const storage = freshStorage()
      const deck = await storage.createDeck('TypeScript')
      await renderRoute({
        storage,
        entry: `/decks/${deck.id}`,
        path: '/decks/:deckId',
        element: <DeckDetailPage />,
        extraRoutes: [{ path: '/decks/:deckId/cards/new', element: <CardEditorPage /> }],
      })
      await page.getByRole('button', { name: 'Add your first card', exact: false }).click()
      await expect.element(page.getByText('New card')).toBeVisible()
    },
  },
  {
    name: 'card-editor-edit',
    run: async () => {
      const storage = freshStorage()
      const deck = await storage.createDeck('TypeScript')
      await storage.createCard(deck.id, 'How to narrow `unknown`?', CODE_BACK, ['types', 'guards'])
      await renderRoute({
        storage,
        entry: `/decks/${deck.id}`,
        path: '/decks/:deckId',
        element: <DeckDetailPage />,
        extraRoutes: [{ path: '/decks/:deckId/cards/:cardId/edit', element: <CardEditorPage /> }],
      })
      await page.getByText('How to narrow', { exact: false }).click()
      await expect.element(page.getByText('Edit card')).toBeVisible()
    },
  },
  {
    name: 'card-editor-rich',
    run: async () => {
      const storage = freshStorage()
      const deck = await storage.createDeck('TypeScript')
      const card = await storage.createCard(deck.id, 'How should a rich answer read?', RICH_EDITOR_BACK)
      await renderRoute({
        storage,
        entry: `/decks/${deck.id}/cards/${card.id}/edit`,
        path: '/decks/:deckId/cards/:cardId/edit',
        element: <CardEditorPage />,
      })
      await expect.element(page.getByText('Preserve hierarchy', { exact: false })).toBeVisible()
    },
  },
  {
    name: 'review-question',
    run: async () => {
      const storage = freshStorage()
      const deck = await storage.createDeck('TypeScript')
      await storage.createCard(deck.id, 'What is the `never` type?', 'The empty type.')
      await renderRoute({ storage, entry: `/decks/${deck.id}/study`, path: '/decks/:deckId/study', element: <ReviewPage /> })
      await expect.element(page.getByRole('button', { name: 'Show answer', exact: false })).toBeVisible()
    },
  },
  {
    name: 'review-answer',
    run: async () => {
      const storage = freshStorage()
      const deck = await storage.createDeck('TypeScript')
      await storage.createCard(deck.id, 'What is the `never` type?', 'The empty type — no value is assignable to it.')
      await renderRoute({ storage, entry: `/decks/${deck.id}/study`, path: '/decks/:deckId/study', element: <ReviewPage /> })
      await page.getByRole('button', { name: 'Show answer', exact: false }).click()
      await expect.element(page.getByText('no value is assignable', { exact: false })).toBeVisible()
    },
  },
  {
    name: 'review-rich-answer',
    run: async () => {
      const storage = freshStorage()
      const deck = await storage.createDeck('TypeScript')
      await storage.createCard(deck.id, 'How should a rich answer read?', RICH_BACK)
      await renderRoute({ storage, entry: `/decks/${deck.id}/study`, path: '/decks/:deckId/study', element: <ReviewPage /> })
      await page.getByRole('button', { name: 'Show answer', exact: false }).click()
      await expect.element(page.getByRole('heading', { name: 'Readable answer' })).toBeVisible()
    },
  },
  {
    name: 'review-nothing-due',
    run: async () => {
      const storage = freshStorage()
      const deck = await storage.createDeck('TypeScript')
      const card = await storage.createCard(deck.id, 'Already reviewed', 'Yes')
      await pushToFuture(storage, card.id)
      await renderRoute({ storage, entry: `/decks/${deck.id}/study`, path: '/decks/:deckId/study', element: <ReviewPage /> })
      await expect.element(page.getByRole('heading', { name: 'Nothing due' })).toBeVisible()
    },
  },
  {
    name: 'review-complete',
    run: async () => {
      const storage = freshStorage()
      const deck = await storage.createDeck('TypeScript')
      await storage.createCard(deck.id, 'Only card', 'Done')
      await renderRoute({ storage, entry: `/decks/${deck.id}/study`, path: '/decks/:deckId/study', element: <ReviewPage /> })
      await page.getByRole('button', { name: 'Show answer', exact: false }).click()
      await page.getByRole('button', { name: 'Good', exact: false }).click()
      await expect.element(page.getByText('Review complete', { exact: false })).toBeVisible()
    },
  },
  {
    name: 'deck-list-nested',
    run: async () => {
      localStorage.removeItem('rem:collapsed-deck-paths')
      const storage = freshStorage()
      const cells = await storage.createDeck('Anatomy::Unit 01::Cells')
      const unit3 = await storage.createDeck('Anatomy::Unit 03')
      const intro = await storage.createDeck('Anatomy::Unit 03::3.1 Intro')
      const tissues = await storage.createDeck('Anatomy::Unit 03::3.2 Tissues')
      const vocab = await storage.createDeck('Japanese::JLPT N5::Vocabulary')
      const rust = await storage.createDeck('Rust')

      await storage.createCard(cells.id, 'What organelle synthesizes ATP?', 'Mitochondria.')
      await storage.createCard(cells.id, 'Where does transcription occur?', 'Nucleus.')
      await storage.createCard(unit3.id, 'Define homeostasis.', 'Dynamic internal equilibrium.')
      await storage.createCard(intro.id, 'Superior vs inferior?', 'Toward head vs toward feet.')
      const reviewed = await storage.createCard(tissues.id, 'Four primary tissue types?', 'Epithelial, connective, muscle, nervous.')
      await pushToFuture(storage, reviewed.id)
      await storage.createCard(vocab.id, '{勉強|べんきょう}', 'Study')
      await storage.createCard(rust.id, 'What does Pin<P> guarantee?', 'The pointee cannot move in memory.')

      await renderRoute({ storage, entry: '/', path: '/', element: <DeckListPage /> })
      await expect.element(page.getByLabelText('Review summary')).toBeVisible()
      await expect.element(page.getByText('3.1 Intro').first()).toBeVisible()
      await expect.element(page.getByText('Vocabulary').first()).toBeVisible()
    },
  },
  {
    name: 'review-cloze-question',
    run: async () => {
      const storage = freshStorage()
      const deck = await storage.createDeck('Physics::Quantum')
      const audio = await storage.putAsset(TINY_WAV, 'audio/wav')
      await storage.createCardOrCloze(
        deck.id,
        `In {量子力学|りょうしりきがく}, the time evolution of a state obeys {{c1::$i\\hbar \\frac{\\partial \\psi}{\\partial t} = \\hat{H}\\psi$::Schrödinger equation}} while photon energy satisfies $E = h\\nu$.\n\n[Audio: schrodinger.wav](asset:${audio.hash})`,
        'The {波動関数|はどうかんすう} $\\psi(x, t)$ encodes probability amplitudes under the Hamiltonian $\\hat{H}$.',
      )
      await renderRoute({ storage, entry: `/decks/${deck.id}/study`, path: '/decks/:deckId/study', element: <ReviewPage /> })
      await expect.element(page.getByText('Cloze · c1')).toBeVisible()
      await expect.poll(() => document.querySelector('.cloze-blank')?.textContent).toBe('[Schrödinger equation]')
      await expect.poll(() => document.querySelector('.katex')).not.toBeNull()
      await expect.poll(() => document.querySelector('ruby')).not.toBeNull()
      await expect.poll(() => document.querySelector<HTMLAudioElement>('audio.card-audio-el')?.getAttribute('src')).toMatch(/^blob:/)
    },
  },
  {
    name: 'review-cloze-answer',
    run: async () => {
      const storage = freshStorage()
      const deck = await storage.createDeck('Physics::Quantum')
      const audio = await storage.putAsset(TINY_WAV, 'audio/wav')
      await storage.createCardOrCloze(
        deck.id,
        `In {量子力学|りょうしりきがく}, the time evolution of a state obeys {{c1::$i\\hbar \\frac{\\partial \\psi}{\\partial t} = \\hat{H}\\psi$::Schrödinger equation}} while photon energy satisfies $E = h\\nu$.\n\n[Audio: schrodinger.wav](asset:${audio.hash})`,
        'The {波動関数|はどうかんすう} $\\psi(x, t)$ encodes probability amplitudes under the Hamiltonian $\\hat{H}$.',
      )
      await renderRoute({ storage, entry: `/decks/${deck.id}/study`, path: '/decks/:deckId/study', element: <ReviewPage /> })
      await page.getByRole('button', { name: 'Show answer', exact: false }).click()
      await expect.element(page.getByRole('region', { name: 'Extra' })).toBeVisible()
      await expect.poll(() => document.querySelector('.cloze-reveal .katex')).not.toBeNull()
      await expect.poll(() => document.querySelector('ruby')).not.toBeNull()
      await expect.poll(() => document.querySelector<HTMLAudioElement>('audio.card-audio-el')?.getAttribute('src')).toMatch(/^blob:/)
    },
  },
  {
    name: 'review-occlusion-question',
    run: async () => {
      const storage = freshStorage()
      const deck = await storage.createDeck('Networking::TCP')
      const asset = await storage.putAsset(TCP_DIAGRAM_SVG, 'image/svg+xml')
      const front = composeOcclusionDocument('Identify the highlighted TCP header field', {
        v: 1,
        image: `asset:${asset.hash}`,
        mode: 'hide-all',
        alt: 'TCP Segment Header',
        masks: [
          { id: 1, type: 'rect', x: 0.05, y: 0.19, w: 0.44, h: 0.18 },
          { id: 2, type: 'rect', x: 0.05, y: 0.41, w: 0.9, h: 0.18 },
          { id: 3, type: 'ellipse', x: 0.05, y: 0.64, w: 0.31, h: 0.18 },
        ],
      })
      await storage.createCardOrCloze(deck.id, front, 'Ports multiplex connections on the same host IP.')
      await renderRoute({ storage, entry: `/decks/${deck.id}/study`, path: '/decks/:deckId/study', element: <ReviewPage /> })
      await expect.element(page.getByText('Image Occlusion · #1')).toBeVisible()
      await waitForOcclusionImageLoaded()
      await expect.poll(() => document.querySelector('[data-mask-id="1"]')?.getAttribute('data-mask-state')).toBe('active')
    },
  },
  {
    name: 'review-occlusion-answer',
    run: async () => {
      const storage = freshStorage()
      const deck = await storage.createDeck('Networking::TCP')
      const asset = await storage.putAsset(TCP_DIAGRAM_SVG, 'image/svg+xml')
      const front = composeOcclusionDocument('Identify the highlighted TCP header field', {
        v: 1,
        image: `asset:${asset.hash}`,
        mode: 'hide-all',
        alt: 'TCP Segment Header',
        masks: [
          { id: 1, type: 'rect', x: 0.05, y: 0.19, w: 0.44, h: 0.18 },
          { id: 2, type: 'rect', x: 0.05, y: 0.41, w: 0.9, h: 0.18 },
          { id: 3, type: 'ellipse', x: 0.05, y: 0.64, w: 0.31, h: 0.18 },
        ],
      })
      await storage.createCardOrCloze(deck.id, front, 'Ports multiplex connections on the same host IP.')
      await renderRoute({ storage, entry: `/decks/${deck.id}/study`, path: '/decks/:deckId/study', element: <ReviewPage /> })
      await waitForOcclusionImageLoaded()
      await page.getByRole('button', { name: 'Show answer', exact: false }).click()
      await expect.poll(() => document.querySelector('[data-mask-id="1"]')?.getAttribute('data-mask-state')).toBe('revealed')
      await expect.element(page.getByRole('button', { name: 'Show all masks' })).toBeVisible()
    },
  },
  {
    name: 'card-editor-occlusion',
    run: async () => {
      const storage = freshStorage()
      const deck = await storage.createDeck('Networking::TCP')
      const asset = await storage.putAsset(TCP_DIAGRAM_SVG, 'image/svg+xml')
      const front = composeOcclusionDocument('TCP Segment Header', {
        v: 1,
        image: `asset:${asset.hash}`,
        mode: 'hide-all',
        alt: 'TCP Segment Header',
        masks: [
          { id: 1, type: 'rect', x: 0.05, y: 0.19, w: 0.44, h: 0.18 },
          { id: 2, type: 'rect', x: 0.05, y: 0.41, w: 0.9, h: 0.18 },
          { id: 3, type: 'ellipse', x: 0.05, y: 0.64, w: 0.31, h: 0.18 },
        ],
      })
      const [card] = await storage.createCardOrCloze(
        deck.id,
        front,
        'RFC 9293 defines the 20-byte minimum TCP header.',
        ['networking', 'tcp'],
      )
      await renderRoute({
        storage,
        entry: `/decks/${deck.id}/cards/${card.id}/edit`,
        path: '/decks/:deckId/cards/:cardId/edit',
        element: <CardEditorPage />,
      })
      await expect.element(page.getByLabelText('Image Occlusion editor')).toBeVisible()
      await waitForOcclusionImageLoaded()
      await expect.poll(() => document.querySelectorAll('[data-editor-mask-index]').length).toBe(3)
    },
  },
  {
    name: 'search-modal',
    run: async () => {
      const storage = freshStorage()
      const rust = await storage.createDeck('Rust')
      const sql = await storage.createDeck('SQL::Internals')
      await storage.createCard(rust.id, 'Ownership Rules', 'Each value in Rust has an owner.', ['ownership'])
      await storage.createCard(rust.id, 'Cargo basics', 'cargo build', ['cargo'])
      await storage.createCard(sql.id, 'What is WAL mode?', 'Write-Ahead Logging.', ['sqlite'])
      await renderRoute({ storage, entry: `/decks/${rust.id}`, path: '/decks/:deckId', element: <DeckDetailPage /> })
      await expect.element(page.getByText('Ownership Rules')).toBeVisible()
      await page.getByRole('button', { name: 'Search', exact: false }).click()
      await expect.element(page.getByText('Jump to deck')).toBeVisible()
    },
  },
  {
    name: 'search-modal-results',
    run: async () => {
      const storage = freshStorage()
      const rust = await storage.createDeck('Rust')
      const sql = await storage.createDeck('SQL::Internals')
      await storage.createCard(rust.id, 'Ownership Rules', 'Each value in Rust has an owner.', ['ownership'])
      await storage.createCard(rust.id, 'Rust Pin<P> guarantee', 'The pointee cannot be moved in memory.', ['async', 'rust'])
      await storage.createCard(sql.id, 'What is WAL mode?', 'Write-Ahead Logging.', ['sqlite'])
      await renderRoute({ storage, entry: `/decks/${rust.id}`, path: '/decks/:deckId', element: <DeckDetailPage /> })
      await expect.element(page.getByText('Ownership Rules')).toBeVisible()
      await page.getByRole('button', { name: 'Search', exact: false }).click()
      const input = page.getByRole('combobox', { name: 'Search all cards' })
      await expect.element(input).toBeVisible()
      await userEvent.type(input.element(), 'Rust')
      const results = page.getByRole('listbox', { name: 'Search results' })
      await expect.element(results).toHaveAttribute('aria-busy', 'false')
      await expect.element(results.getByText('Rust Pin<P> guarantee')).toBeVisible()
    },
  },
  {
    name: 'search-modal-shortcut',
    run: async () => {
      const storage = freshStorage()
      const rust = await storage.createDeck('Rust')
      const os = await storage.createDeck('Systems::Memory')
      const sql = await storage.createDeck('SQL')
      await storage.createCard(
        rust.id,
        'What does `Pin<P>` guarantee for self-referential memory?',
        'The pointee will not be moved in memory before being dropped.',
        ['async', 'memory'],
      )
      await storage.createCard(
        os.id,
        'How does a TLB accelerate virtual memory translation?',
        'It caches recent page table entries on-chip.',
        ['hardware', 'memory'],
      )
      await storage.createCard(sql.id, 'What is WAL mode?', 'Write-Ahead Logging.')

      await renderRoute({ storage, entry: '/', path: '/', element: <DeckListPage /> })
      await expect.element(page.getByLabelText('Review summary')).toBeVisible()

      window.dispatchEvent(new KeyboardEvent('keydown', { key: 'k', ctrlKey: true, bubbles: true }))
      const searchbox = page.getByRole('combobox', { name: 'Search all cards' })
      await expect.element(searchbox).toBeVisible()
      await userEvent.type(searchbox.element(), 'memory')

      const results = page.getByRole('listbox', { name: 'Search results' })
      await expect.element(results).toHaveAttribute('aria-busy', 'false')
      await expect.element(results.getByText('How does a TLB accelerate virtual memory translation?')).toBeVisible()
    },
  },
]

afterEach(() => {
  delete document.documentElement.dataset.theme
})

for (const theme of ['light', 'dark'] as const) {
  for (const sc of scenarios) {
    test(`${sc.name} — ${theme}`, async () => {
      if (theme === 'dark') document.documentElement.dataset.theme = 'dark'
      await sc.run()
      await shoot('screen', theme === 'dark' ? `${sc.name}-dark` : sc.name)
    })
  }
}
