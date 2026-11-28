import { afterEach, test, expect, vi } from 'vitest'
import { page, userEvent } from 'vitest/browser'
import { DeckListPage } from './DeckListPage'
import { serializeBackup, type DeckBackup } from '../../data/backup'
import { DEFAULT_DECK_SETTINGS } from '../../domain/models'
import { freshStorage } from '../../test/seed'
import { renderRoute } from '../../test/renderRoute'

afterEach(async () => {
  await page.viewport(1280, 800)
})

test('creates a deck using the FSRS scheduler', async () => {
  const storage = freshStorage()
  await renderRoute({ storage, entry: '/', path: '/', element: <DeckListPage /> })

  await page.getByLabelText('New deck name').fill('Algorithms')
  await page.getByRole('button', { name: 'Add deck' }).click()

  await expect.poll(async () => (await storage.listDecks())[0]?.schedulerKind).toBe('fsrs')
})

test('presents decks with monochrome identity icons instead of color markers', async () => {
  const storage = freshStorage()
  await storage.createDeck('Spanish')
  await renderRoute({ storage, entry: '/', path: '/', element: <DeckListPage /> })

  const deckList = page.getByLabelText('Decks')
  await expect.element(deckList).toBeVisible()
  expect(deckList.element().querySelector('.deck-list-dot')).toBeNull()
  expect(document.querySelector('.deck-dot')).toBeNull()
  expect(document.querySelector('.deck-icon')).toBeTruthy()
  await expect.element(page.getByText('FSRS')).not.toBeInTheDocument()
})

test('presents due cards with one compact action instead of a gradient panel', async () => {
  const storage = freshStorage()
  const deck = await storage.createDeck('Spanish')
  await storage.createCard(deck.id, 'Hola', 'Hello')
  await renderRoute({ storage, entry: '/', path: '/', element: <DeckListPage /> })

  const summary = page.getByLabelText('Review summary')
  const action = page.getByRole('link', { name: /Start review/ })
  await expect.element(summary).toBeVisible()
  await expect.element(action).toBeVisible()

  expect(getComputedStyle(summary.element()).backgroundImage).toBe('none')
  expect(action.element().getBoundingClientRect().width).toBeLessThan(
    summary.element().getBoundingClientRect().width / 2,
  )
})

test('Today stays within the minimum supported window width', async () => {
  await page.viewport(760, 720)
  const storage = freshStorage()
  const deck = await storage.createDeck('A long deck name that still fits cleanly')
  await storage.createCard(deck.id, 'Question', 'Answer')
  await renderRoute({ storage, entry: '/', path: '/', element: <DeckListPage /> })
  await expect.element(page.getByLabelText('Review summary')).toBeVisible()

  const content = document.querySelector<HTMLElement>('.content')!
  expect(content.scrollWidth).toBe(content.clientWidth)
})

test('renders studied progress as an inline ring beside deck metadata instead of an underline bar', async () => {
  const storage = freshStorage()
  const deck = await storage.createDeck('Rust')
  const card = await storage.createCard(deck.id, 'Main', 'fn main() {}')
  await storage.updateCard(card.id, {
    scheduling: { ...card.scheduling, state: 2, reps: 1, due: Date.now() + 86_400_000 * 5 },
  })
  await renderRoute({ storage, entry: '/', path: '/', element: <DeckListPage /> })

  await expect.element(page.getByText('1 / 1 studied')).toBeVisible()
  expect(document.querySelector('.deck-list-progress')).toBeNull()
  expect(document.querySelector('.deck-progress-ring.is-complete')).toBeTruthy()
})

test('formats nested decks with aligned chevrons, icons, typography, and progressive indentation', async () => {
  localStorage.removeItem('rem:collapsed-deck-paths')
  const storage = freshStorage()
  const u1 = await storage.createDeck('Anatomy::Unit 01::Cells')
  const u3 = await storage.createDeck('Anatomy::Unit 03')
  const u3Child = await storage.createDeck('Anatomy::Unit 03::3.1 Intro')
  const rust = await storage.createDeck('Rust')
  await storage.createCard(u1.id, 'Q1', 'A1')
  await storage.createCard(u3.id, 'Q2', 'A2')
  await storage.createCard(u3Child.id, 'Q3', 'A3')
  await storage.createCard(rust.id, 'Q4', 'A4')

  await renderRoute({ storage, entry: '/', path: '/', element: <DeckListPage /> })
  await expect.element(page.getByText('3.1 Intro').first()).toBeVisible()
  await expect
    .poll(() => document.querySelectorAll('.side-decks .side-deck-row').length)
    .toBe(6)

  const sidebar = document.querySelector<HTMLElement>('.side-decks')!
  const rows = [...sidebar.querySelectorAll<HTMLElement>('.side-deck-row')]
  const byLabel = (label: string) =>
    rows.find((row) => row.querySelector('.nav-grow')?.textContent?.trim() === label)!

  const anatomyRow = byLabel('Anatomy')
  const rustRow = byLabel('Rust')
  const unit01Row = byLabel('Unit 01')
  const unit03Row = byLabel('Unit 03')
  const introRow = byLabel('3.1 Intro')

  // Depth 0 virtual folder and depth 0 leaf deck share identical typography and icon/label X alignment.
  const anatomyMain = anatomyRow.querySelector<HTMLElement>('.side-deck-main')!
  const rustMain = rustRow.querySelector<HTMLElement>('.side-deck-main')!
  expect(getComputedStyle(anatomyMain).fontSize).toBe(getComputedStyle(rustMain).fontSize)
  expect(getComputedStyle(anatomyMain).fontWeight).toBe(getComputedStyle(rustMain).fontWeight)
  expect(getComputedStyle(anatomyMain).cursor).toBe('pointer')
  expect(
    Math.abs(
      anatomyRow.querySelector('.nav-grow')!.getBoundingClientRect().left -
        rustRow.querySelector('.nav-grow')!.getBoundingClientRect().left,
    ),
  ).toBeLessThan(1)

  // Real parent deck (Unit 03, depth 1) aligns its chevron & label with virtual sibling (Unit 01, depth 1),
  // indented strictly to the right of depth 0 (Anatomy).
  const anatomyChevronLeft = anatomyRow.querySelector('.deck-tree-toggle')!.getBoundingClientRect().left
  const unit01ChevronLeft = unit01Row.querySelector('.deck-tree-toggle')!.getBoundingClientRect().left
  const unit03ChevronLeft = unit03Row.querySelector('.deck-tree-toggle')!.getBoundingClientRect().left
  expect(unit03ChevronLeft).toBeGreaterThan(anatomyChevronLeft)
  expect(Math.abs(unit03ChevronLeft - unit01ChevronLeft)).toBeLessThan(1)

  // Depth 2 child (3.1 Intro) icon is strictly indented to the right of its depth 1 real parent (Unit 03).
  const unit03IconLeft = unit03Row.querySelector('.deck-icon')!.getBoundingClientRect().left
  const introIconLeft = introRow.querySelector('.deck-icon')!.getBoundingClientRect().left
  expect(introIconLeft).toBeGreaterThan(unit03IconLeft)

  // Uses crisp SVG chevron rather than raw Unicode triangle glyph.
  expect(anatomyRow.querySelector('.deck-tree-toggle svg.deck-chevron')).toBeTruthy()
  expect(anatomyRow.querySelector('.deck-tree-toggle')?.textContent?.trim()).toBe('')
})

test('deletes a whole virtual parent folder from Today with subtree count confirmation', async () => {
  localStorage.removeItem('rem:collapsed-deck-paths')
  const storage = freshStorage()
  const d1 = await storage.createDeck('JLPT::N5::Vocab')
  const d2 = await storage.createDeck('JLPT::N4::Kanji')
  const keep = await storage.createDeck('Rust')
  await storage.createCard(d1.id, 'Q1', 'A1')
  await storage.createCard(d2.id, 'Q2', 'A2')
  await storage.createCard(keep.id, 'Q3', 'A3')

  await renderRoute({ storage, entry: '/', path: '/', element: <DeckListPage /> })
  const deckList = page.getByLabelText('Decks')
  await expect.element(deckList.getByText('JLPT')).toBeVisible()

  const jlptRow = deckList.element().querySelector<HTMLElement>('.deck-list-row--virtual')!
  const deleteBtn = jlptRow.querySelector<HTMLButtonElement>('.deck-list-delete')!
  deleteBtn.click()

  await expect.element(page.getByText('Delete "JLPT"?')).toBeVisible()
  await expect.element(page.getByText(/2 subdecks and 2 cards inside "JLPT"/)).toBeVisible()
  await page.getByRole('button', { name: 'Delete all' }).click()

  await expect.poll(async () => (await storage.listDecks()).map((d) => d.name)).toEqual(['Rust'])
})

function makeBackupFile(decks: DeckBackup[], filename = 'backup.json'): File {
  return new File([serializeBackup(decks, 1_700_000_000_000)], filename, {
    type: 'application/json',
  })
}

function makeSampleDeckBackup(name: string): DeckBackup {
  return {
    name,
    createdAt: 1,
    schedulerKind: 'fsrs',
    settings: DEFAULT_DECK_SETTINGS,
    cards: [
      {
        front: 'Question',
        back: 'Answer',
        createdAt: 1,
        updatedAt: 1,
        tags: [],
        suspended: false,
        lastAgainAt: null,
        scheduling: {
          kind: 'fsrs',
          stability: 0,
          difficulty: 0,
          reps: 0,
          lapses: 0,
          state: 0,
          step: 0,
          lastReview: null,
          due: 1,
        },
        reviews: [],
      },
    ],
  }
}

test('offers Anki and JSON import from the add-deck row and empty state', async () => {
  const storage = freshStorage()
  storage.previewApkg = vi.fn(async () => ({
    added: ['Japanese'],
    replaced: [],
    totalCards: 2,
    mediaCount: 0,
    reviewLogs: 0,
    warnings: [],
  }))
  storage.importApkg = vi.fn(async () => {
    await storage.createDeck('Japanese')
    return {
      added: ['Japanese'],
      replaced: [],
      importedCards: 2,
      importedReviews: 0,
      importedMedia: 0,
      warnings: [],
    }
  })

  await renderRoute({ storage, entry: '/', path: '/', element: <DeckListPage /> })

  await expect.element(page.getByRole('button', { name: 'Import Anki (.apkg)' })).toBeVisible()
  await expect.element(page.getByRole('button', { name: 'Import JSON backup' })).toBeVisible()

  const importBtn = page.getByRole('button', { name: 'Import', exact: true })
  await importBtn.click()
  await expect.element(page.getByRole('menuitem', { name: /Anki deck \(\.apkg\)/ })).toBeVisible()
  await expect.element(page.getByRole('menuitem', { name: /rem backup \(\.json\)/ })).toBeVisible()

  // Escape closes the menu and restores focus to the trigger.
  await userEvent.keyboard('{Escape}')
  await expect.element(page.getByRole('menu', { name: 'Import deck' })).not.toBeInTheDocument()

  // Importing an .apkg archive with no conflicts imports immediately and displays summary feedback.
  const apkgFile = new File([new Uint8Array([0x50, 0x4b, 0x03, 0x04])], 'japanese.apkg', {
    type: 'application/zip',
  })
  await page.getByLabelText('Import Anki deck file').upload(apkgFile)

  await expect.element(page.getByText('Imported Anki deck: 2 cards, 1 deck.')).toBeVisible()
  await expect.poll(async () => (await storage.listDecks()).map((d) => d.name)).toEqual(['Japanese'])
})

test('imports JSON backup from Today and confirms before replacing same-named decks', async () => {
  const storage = freshStorage()
  await storage.createDeck('Spanish')
  await renderRoute({ storage, entry: '/', path: '/', element: <DeckListPage /> })

  // Fresh deck imports directly.
  await page.getByLabelText('Import backup file').upload(makeBackupFile([makeSampleDeckBackup('Rust')]))
  await expect.element(page.getByText('Imported 1 deck.')).toBeVisible()
  await expect
    .poll(async () => (await storage.listDecks()).map((d) => d.name).sort())
    .toEqual(['Rust', 'Spanish'])

  // Same-named deck opens confirmation dialog before replacing.
  await page.getByLabelText('Import backup file').upload(makeBackupFile([makeSampleDeckBackup('Spanish')]))
  await expect.element(page.getByRole('heading', { name: 'Replace existing decks?' })).toBeVisible()
  await expect.element(page.getByText(/will replace existing deck: Spanish/)).toBeVisible()
  await page.getByRole('button', { name: 'Replace', exact: true }).click()

  await expect.element(page.getByText('Imported 1 deck (replaced 1).')).toBeVisible()
})

test('confirms Anki .apkg replacement and rejects empty JSON backups on Today', async () => {
  const storage = freshStorage()
  await storage.createDeck('Spanish')
  storage.previewApkg = vi.fn(async () => ({
    added: ['Spanish::Verbs'],
    replaced: ['Spanish'],
    totalCards: 5,
    mediaCount: 1,
    reviewLogs: 3,
    warnings: [],
  }))
  storage.importApkg = vi.fn(async () => ({
    added: ['Spanish::Verbs'],
    replaced: ['Spanish'],
    importedCards: 5,
    importedReviews: 3,
    importedMedia: 1,
    warnings: [],
  }))

  await renderRoute({ storage, entry: '/', path: '/', element: <DeckListPage /> })

  // Empty JSON backup surfaces an error.
  await page.getByLabelText('Import backup file').upload(makeBackupFile([]))
  await expect.element(page.getByText('Backup file contains no decks.')).toBeVisible()

  // Conflicting .apkg prompts before calling importApkg.
  const apkgFile = new File([new Uint8Array([0x50, 0x4b, 0x03, 0x04])], 'spanish.apkg', {
    type: 'application/zip',
  })
  await page.getByLabelText('Import Anki deck file').upload(apkgFile)

  await expect.element(page.getByRole('heading', { name: 'Replace existing decks?' })).toBeVisible()
  expect(storage.importApkg).not.toHaveBeenCalled()
  await page.getByRole('button', { name: 'Replace & import' }).click()

  expect(storage.importApkg).toHaveBeenCalledTimes(1)
  await expect
    .element(page.getByText('Imported Anki deck: 5 cards, 2 decks, 3 reviews, 1 media (replaced 1).'))
    .toBeVisible()
})

test('closes replace confirmation modal and surfaces error if replacement import fails', async () => {
  const storage = freshStorage()
  await storage.createDeck('Spanish')
  storage.previewApkg = vi.fn(async () => ({
    added: [],
    replaced: ['Spanish'],
    totalCards: 3,
    mediaCount: 0,
    reviewLogs: 0,
    warnings: [],
  }))
  storage.importApkg = vi.fn(async () => {
    throw new Error('Corrupt archive payload')
  })

  await renderRoute({ storage, entry: '/', path: '/', element: <DeckListPage /> })

  const apkgFile = new File([new Uint8Array([0x50, 0x4b, 0x03, 0x04])], 'bad.apkg', {
    type: 'application/zip',
  })
  await page.getByLabelText('Import Anki deck file').upload(apkgFile)

  await expect.element(page.getByRole('heading', { name: 'Replace existing decks?' })).toBeVisible()
  await page.getByRole('button', { name: 'Replace & import' }).click()

  await expect.element(page.getByRole('heading', { name: 'Replace existing decks?' })).not.toBeInTheDocument()
  await expect.element(page.getByRole('alert')).toHaveTextContent('Corrupt archive payload')
})
