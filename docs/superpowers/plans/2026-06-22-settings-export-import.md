# Settings Surface + Deck Export/Import Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a reusable `/settings` page with deck-scoped JSON export and replace-by-name import (backup insurance for browser-only data).

**Architecture:** A pure, DB-agnostic `backup.ts` module handles serialization, validation, and import planning. One new `Storage.importDecks` method (implemented transactionally in `DexieStorage`) performs the atomic, fidelity-preserving insert. A new `SettingsPage` drives the UI; export composes existing reads, import flows file → parse → warn → confirm → `importDecks`.

**Tech Stack:** React 19, react-router-dom v7, Dexie/IndexedDB, TipTap (unrelated), Vitest (unit `jsdom` + browser `playwright/chromium` projects).

## Global Constraints

- **Preserve the seam discipline.** New persistence behavior goes behind the `Storage` interface; UI talks only to `Storage` + the pure `backup.ts` helpers.
- **Match decks by `name`** for import collisions (names are not unique in the model — replacing a name removes *all* existing decks with that name).
- **Omit IDs from the backup file.** Generate fresh `crypto.randomUUID()` IDs on import.
- **Full fidelity:** preserve `front`/`back`/`createdAt`/`updatedAt`/`scheduling` on round-trip.
- **Backup file shape:** `{ format: 'rem-backup', version: 1, exportedAt: number, decks: DeckBackup[] }`.
- **Follow existing style:** CSS classes use the token system in `src/ui/tokens.css` (e.g. `var(--border)`, `var(--muted)`, `var(--danger)`, `var(--radius-sm)`, `var(--text-lg)`, `var(--text-sm)`). Buttons use `btn` / `btn-primary` / `btn-danger` / `btn-ghost`.
- **Tests:** unit tests `*.test.ts(x)` run in the `unit` (jsdom, `fake-indexeddb/auto`) project; browser tests `*.browser.test.tsx` run in the `browser` (chromium) project. `npm test` runs both.

---

### Task 1: Backup serialization module

Pure functions — no DB, no React. Fully unit-testable with stub storage.

**Files:**
- Create: `src/data/backup.ts`
- Test: `src/data/backup.test.ts`

**Interfaces:**
- Consumes: `Storage` (type only, for `collectBackup`), `ID` / `SchedulingState` from `../domain/models`.
- Produces:
  - `interface CardBackup { front: string; back: string; createdAt: number; updatedAt: number; scheduling: SchedulingState }`
  - `interface DeckBackup { name: string; createdAt: number; cards: CardBackup[] }`
  - `interface BackupFile { format: 'rem-backup'; version: 1; exportedAt: number; decks: DeckBackup[] }`
  - `collectBackup(storage: Storage, deckIds: ID[]): Promise<DeckBackup[]>`
  - `serializeBackup(decks: DeckBackup[], exportedAt: number): string`
  - `parseBackup(text: string): DeckBackup[]`
  - `planImport(incomingNames: string[], existingNames: string[]): { added: string[]; replaced: string[] }`

- [ ] **Step 1: Write the failing tests**

Create `src/data/backup.test.ts`:

```ts
import { describe, it, expect } from 'vitest'
import {
  collectBackup,
  serializeBackup,
  parseBackup,
  planImport,
  type DeckBackup,
} from './backup'
import type { Storage } from './Storage'
import type { Card, Deck } from '../domain/models'

const sched = { repetitions: 1, intervalDays: 3, easeFactor: 2.6, due: 999 }

function fakeStorage(decks: Deck[], cardsByDeck: Record<string, Card[]>): Storage {
  return {
    listDecks: async () => decks,
    listCards: async (id: string) => cardsByDeck[id] ?? [],
  } as unknown as Storage
}

const deckA: Deck = { id: 'a', name: 'Spanish', createdAt: 10 }
const cardA: Card = {
  id: 'c1', deckId: 'a', front: 'hola', back: 'hello',
  createdAt: 11, updatedAt: 12, scheduling: sched,
}

describe('collectBackup', () => {
  it('collects selected decks with their cards, dropping ids', async () => {
    const storage = fakeStorage([deckA], { a: [cardA] })
    const out = await collectBackup(storage, ['a'])
    expect(out).toEqual([
      {
        name: 'Spanish',
        createdAt: 10,
        cards: [{ front: 'hola', back: 'hello', createdAt: 11, updatedAt: 12, scheduling: sched }],
      },
    ])
  })

  it('skips deck ids that do not exist', async () => {
    const storage = fakeStorage([deckA], { a: [cardA] })
    expect(await collectBackup(storage, ['missing'])).toEqual([])
  })
})

describe('serializeBackup', () => {
  it('emits the format/version/exportedAt envelope', () => {
    const decks: DeckBackup[] = [{ name: 'Spanish', createdAt: 10, cards: [] }]
    const parsed = JSON.parse(serializeBackup(decks, 1234))
    expect(parsed.format).toBe('rem-backup')
    expect(parsed.version).toBe(1)
    expect(parsed.exportedAt).toBe(1234)
    expect(parsed.decks).toEqual(decks)
  })
})

describe('parseBackup', () => {
  const valid = serializeBackup(
    [{ name: 'Spanish', createdAt: 10, cards: [{ front: 'hola', back: 'hello', createdAt: 11, updatedAt: 12, scheduling: sched }] }],
    1234,
  )

  it('round-trips valid input', () => {
    expect(parseBackup(valid)).toEqual([
      { name: 'Spanish', createdAt: 10, cards: [{ front: 'hola', back: 'hello', createdAt: 11, updatedAt: 12, scheduling: sched }] },
    ])
  })

  it('rejects non-JSON', () => {
    expect(() => parseBackup('not json')).toThrow(/valid JSON/i)
  })

  it('rejects a wrong format tag', () => {
    expect(() => parseBackup(JSON.stringify({ format: 'other', version: 1, decks: [] }))).toThrow(/rem backup/i)
  })

  it('rejects an unsupported version', () => {
    expect(() => parseBackup(JSON.stringify({ format: 'rem-backup', version: 2, decks: [] }))).toThrow(/version/i)
  })

  it('rejects a malformed deck', () => {
    expect(() => parseBackup(JSON.stringify({ format: 'rem-backup', version: 1, decks: [{ name: 5, createdAt: 1, cards: [] }] }))).toThrow(/malformed/i)
  })

  it('rejects a malformed card', () => {
    const bad = { format: 'rem-backup', version: 1, decks: [{ name: 'x', createdAt: 1, cards: [{ front: 'a' }] }] }
    expect(() => parseBackup(JSON.stringify(bad))).toThrow(/malformed/i)
  })
})

describe('planImport', () => {
  it('splits incoming names into added vs replaced and de-dupes', () => {
    expect(planImport(['Spanish', 'French', 'Spanish'], ['Spanish', 'German'])).toEqual({
      added: ['French'],
      replaced: ['Spanish'],
    })
  })
})
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `npm test -- --project unit src/data/backup.test.ts`
Expected: FAIL — `Cannot find module './backup'` (file does not exist yet).

- [ ] **Step 3: Write the implementation**

Create `src/data/backup.ts`:

```ts
import type { ID, SchedulingState } from '../domain/models'
import type { Storage } from './Storage'

export interface CardBackup {
  front: string
  back: string
  createdAt: number
  updatedAt: number
  scheduling: SchedulingState
}

export interface DeckBackup {
  name: string
  createdAt: number
  cards: CardBackup[]
}

export interface BackupFile {
  format: 'rem-backup'
  version: 1
  exportedAt: number
  decks: DeckBackup[]
}

/** Read the named decks (with their cards) into the DB-agnostic backup shape. */
export async function collectBackup(storage: Storage, deckIds: ID[]): Promise<DeckBackup[]> {
  const decks = await storage.listDecks()
  const byId = new Map(decks.map((d) => [d.id, d]))
  const out: DeckBackup[] = []
  for (const id of deckIds) {
    const deck = byId.get(id)
    if (!deck) continue
    const cards = await storage.listCards(id)
    out.push({
      name: deck.name,
      createdAt: deck.createdAt,
      cards: cards.map((c) => ({
        front: c.front,
        back: c.back,
        createdAt: c.createdAt,
        updatedAt: c.updatedAt,
        scheduling: c.scheduling,
      })),
    })
  }
  return out
}

export function serializeBackup(decks: DeckBackup[], exportedAt: number): string {
  const file: BackupFile = { format: 'rem-backup', version: 1, exportedAt, decks }
  return JSON.stringify(file, null, 2)
}

export function parseBackup(text: string): DeckBackup[] {
  let data: unknown
  try {
    data = JSON.parse(text)
  } catch {
    throw new Error('Not a valid JSON file.')
  }
  if (!isObject(data) || data.format !== 'rem-backup') {
    throw new Error('Not a rem backup file.')
  }
  if (data.version !== 1) {
    throw new Error(`Unsupported backup version: ${String(data.version)}.`)
  }
  if (!Array.isArray(data.decks)) {
    throw new Error('Backup file is malformed.')
  }
  return data.decks.map(parseDeck)
}

/** Classify incoming deck names against existing names (de-duplicated). */
export function planImport(
  incomingNames: string[],
  existingNames: string[],
): { added: string[]; replaced: string[] } {
  const existing = new Set(existingNames)
  const seen = new Set<string>()
  const added: string[] = []
  const replaced: string[] = []
  for (const name of incomingNames) {
    if (seen.has(name)) continue
    seen.add(name)
    if (existing.has(name)) replaced.push(name)
    else added.push(name)
  }
  return { added, replaced }
}

function isObject(v: unknown): v is Record<string, unknown> {
  return typeof v === 'object' && v !== null
}

function parseDeck(raw: unknown): DeckBackup {
  if (
    !isObject(raw) ||
    typeof raw.name !== 'string' ||
    typeof raw.createdAt !== 'number' ||
    !Array.isArray(raw.cards)
  ) {
    throw new Error('Backup file is malformed.')
  }
  return { name: raw.name, createdAt: raw.createdAt, cards: raw.cards.map(parseCard) }
}

function parseCard(raw: unknown): CardBackup {
  if (
    !isObject(raw) ||
    typeof raw.front !== 'string' ||
    typeof raw.back !== 'string' ||
    typeof raw.createdAt !== 'number' ||
    typeof raw.updatedAt !== 'number' ||
    !isScheduling(raw.scheduling)
  ) {
    throw new Error('Backup file is malformed.')
  }
  return {
    front: raw.front,
    back: raw.back,
    createdAt: raw.createdAt,
    updatedAt: raw.updatedAt,
    scheduling: raw.scheduling,
  }
}

function isScheduling(v: unknown): v is SchedulingState {
  return (
    isObject(v) &&
    typeof v.repetitions === 'number' &&
    typeof v.intervalDays === 'number' &&
    typeof v.easeFactor === 'number' &&
    typeof v.due === 'number'
  )
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `npm test -- --project unit src/data/backup.test.ts`
Expected: PASS (all cases green).

- [ ] **Step 5: Commit**

```bash
git add src/data/backup.ts src/data/backup.test.ts
git commit -m "feat: add deck backup serialization module"
```

---

### Task 2: Storage.importDecks (interface + Dexie implementation)

**Files:**
- Modify: `src/data/Storage.ts` (add `ImportResult` type + `importDecks` to the interface)
- Modify: `src/data/dexie/DexieStorage.ts` (implement `importDecks`)
- Test: `src/data/dexie/DexieStorage.test.ts` (append an `import` describe block)

**Interfaces:**
- Consumes: `DeckBackup` from `../backup` (Task 1); `planImport` from `../backup`.
- Produces:
  - `interface ImportResult { added: string[]; replaced: string[] }` (in `Storage.ts`)
  - `Storage.importDecks(decks: DeckBackup[]): Promise<ImportResult>`

- [ ] **Step 1: Write the failing tests**

Append to `src/data/dexie/DexieStorage.test.ts`:

```ts
describe('importDecks', () => {
  it('adds brand-new decks with their cards', async () => {
    const result = await storage.importDecks([
      {
        name: 'Spanish',
        createdAt: 5,
        cards: [
          { front: 'hola', back: 'hello', createdAt: 6, updatedAt: 7, scheduling: { repetitions: 2, intervalDays: 4, easeFactor: 2.7, due: 8 } },
        ],
      },
    ])

    expect(result).toEqual({ added: ['Spanish'], replaced: [] })
    const decks = await storage.listDecks()
    expect(decks).toHaveLength(1)
    const cards = await storage.listCards(decks[0].id)
    expect(cards).toHaveLength(1)
    expect(cards[0].front).toBe('hola')
    expect(cards[0].scheduling).toEqual({ repetitions: 2, intervalDays: 4, easeFactor: 2.7, due: 8 })
    expect(cards[0].createdAt).toBe(6)
    expect(cards[0].updatedAt).toBe(7)
  })

  it('replaces a same-named deck, dropping its old cards', async () => {
    const old = await storage.createDeck('Spanish')
    await storage.createCard(old.id, 'old-front', 'old-back')

    const result = await storage.importDecks([
      { name: 'Spanish', createdAt: 5, cards: [
        { front: 'new', back: 'new', createdAt: 6, updatedAt: 7, scheduling: { repetitions: 0, intervalDays: 0, easeFactor: 2.5, due: 8 } },
      ] },
    ])

    expect(result).toEqual({ added: [], replaced: ['Spanish'] })
    const decks = await storage.listDecks()
    expect(decks).toHaveLength(1)
    expect(decks[0].id).not.toBe(old.id) // fresh id
    const cards = await storage.listCards(decks[0].id)
    expect(cards.map((c) => c.front)).toEqual(['new'])
    expect(await storage.getCard('old-front')).toBeUndefined()
  })

  it('removes every existing deck sharing an incoming name', async () => {
    await storage.createDeck('Dup')
    await storage.createDeck('Dup')

    await storage.importDecks([{ name: 'Dup', createdAt: 1, cards: [] }])

    const decks = await storage.listDecks()
    expect(decks.filter((d) => d.name === 'Dup')).toHaveLength(1)
  })
})
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `npm test -- --project unit src/data/dexie/DexieStorage.test.ts`
Expected: FAIL — `storage.importDecks is not a function`.

- [ ] **Step 3a: Add the interface method**

In `src/data/Storage.ts`, add the import and type near the top (after the existing import line):

```ts
import type { Card, Deck, ID, SchedulingState } from '../domain/models'
import type { DeckBackup } from './backup'

/** Outcome of an import: deck names added fresh vs. names that replaced existing decks. */
export interface ImportResult {
  added: string[]
  replaced: string[]
}
```

Then add to the `Storage` interface, just below `countDue(...)`:

```ts
  /** Insert decks+cards; any existing deck whose name matches an incoming deck
   *  is removed first (replace-by-name). IDs are regenerated. */
  importDecks(decks: DeckBackup[]): Promise<ImportResult>
```

- [ ] **Step 3b: Implement it in DexieStorage**

In `src/data/dexie/DexieStorage.ts`, update the imports:

```ts
import type { CardPatch, ImportResult, Storage } from '../Storage'
import { planImport, type DeckBackup } from '../backup'
```

Add the method at the end of the class (after `countDue`):

```ts
  async importDecks(decks: DeckBackup[]): Promise<ImportResult> {
    const incomingNames = decks.map((d) => d.name)
    return this.db.transaction('rw', this.db.decks, this.db.cards, async () => {
      const existing = await this.db.decks.toArray()
      const result = planImport(incomingNames, existing.map((d) => d.name))

      const toReplace = new Set(result.replaced)
      const deckIdsToDelete = existing.filter((d) => toReplace.has(d.name)).map((d) => d.id)
      for (const id of deckIdsToDelete) {
        await this.db.cards.where('deckId').equals(id).delete()
        await this.db.decks.delete(id)
      }

      for (const d of decks) {
        const deckId = crypto.randomUUID()
        await this.db.decks.add({ id: deckId, name: d.name, createdAt: d.createdAt })
        for (const c of d.cards) {
          await this.db.cards.add({
            id: crypto.randomUUID(),
            deckId,
            front: c.front,
            back: c.back,
            createdAt: c.createdAt,
            updatedAt: c.updatedAt,
            scheduling: c.scheduling,
          })
        }
      }
      return result
    })
  }
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `npm test -- --project unit src/data/dexie/DexieStorage.test.ts`
Expected: PASS (existing + new cases).

- [ ] **Step 5: Typecheck and commit**

Run: `npm run typecheck`
Expected: no errors.

```bash
git add src/data/Storage.ts src/data/dexie/DexieStorage.ts src/data/dexie/DexieStorage.test.ts
git commit -m "feat: add Storage.importDecks (replace-by-name, atomic)"
```

---

### Task 3: SettingsPage + route + header link + styles

**Files:**
- Create: `src/features/settings/SettingsPage.tsx`
- Create: `src/features/settings/SettingsPage.browser.test.tsx`
- Modify: `src/app/routes.tsx` (add `/settings` route)
- Modify: `src/ui/Layout.tsx` (header ⚙ link)
- Modify: `src/ui/styles.css` (append settings styles)

**Interfaces:**
- Consumes: `useStorage` (`../../data/StorageContext`); `collectBackup`, `serializeBackup`, `parseBackup`, `planImport`, `DeckBackup` (`../../data/backup`); `Storage.importDecks` (Task 2).
- Produces: `SettingsPage` (default-styled page); route `path: 'settings'`.

- [ ] **Step 1: Write the failing browser test**

Create `src/features/settings/SettingsPage.browser.test.tsx`:

```tsx
import { test, expect } from 'vitest'
import { page } from 'vitest/browser'
import { SettingsPage } from './SettingsPage'
import { freshStorage } from '../../test/seed'
import { renderRoute } from '../../test/renderRoute'
import { serializeBackup, type DeckBackup } from '../../data/backup'

function renderSettings(storage: ReturnType<typeof freshStorage>) {
  return renderRoute({ storage, entry: '/settings', path: '/settings', element: <SettingsPage /> })
}

test('export button enables only when a deck is selected', async () => {
  const storage = freshStorage()
  await storage.createDeck('TypeScript')
  await storage.createDeck('Spanish')
  renderSettings(storage)

  const exportBtn = page.getByRole('button', { name: 'Export selected' })
  await expect.element(exportBtn).toBeDisabled()
  await page.getByLabelText('Select all decks').click()
  await expect.element(exportBtn).toBeEnabled()
})

test('importing a same-named deck warns, then replaces on confirm', async () => {
  const storage = freshStorage()
  await storage.createDeck('Spanish')
  renderSettings(storage)

  const incoming: DeckBackup[] = [
    {
      name: 'Spanish',
      createdAt: 1,
      cards: [
        { front: 'hola', back: 'hello', createdAt: 1, updatedAt: 1, scheduling: { repetitions: 0, intervalDays: 0, easeFactor: 2.5, due: 1 } },
      ],
    },
  ]
  const file = new File([serializeBackup(incoming, 1000)], 'backup.json', { type: 'application/json' })

  // Locator.upload sets the file on the (real) <input type=file>.
  // Fallback if unavailable: import { userEvent } from 'vitest/browser'; userEvent.upload(locator, file)
  await page.getByLabelText('Import backup file').upload(file)

  await expect.element(page.getByText('will be replaced', { exact: false })).toBeVisible()
  await page.getByRole('button', { name: 'Replace' }).click()
  await expect.element(page.getByText('Imported', { exact: false })).toBeVisible()
})
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `npm test -- --project browser src/features/settings/SettingsPage.browser.test.tsx`
Expected: FAIL — cannot resolve `./SettingsPage`.

- [ ] **Step 3a: Write the component**

Create `src/features/settings/SettingsPage.tsx`:

```tsx
import { useState, type ChangeEvent } from 'react'
import { useLiveQuery } from 'dexie-react-hooks'
import { useStorage } from '../../data/StorageContext'
import {
  collectBackup,
  serializeBackup,
  parseBackup,
  planImport,
  type DeckBackup,
} from '../../data/backup'

function downloadJson(json: string, filename: string) {
  const url = URL.createObjectURL(new Blob([json], { type: 'application/json' }))
  const a = document.createElement('a')
  a.href = url
  a.download = filename
  a.click()
  URL.revokeObjectURL(url)
}

function todayStamp(): string {
  return new Date().toISOString().slice(0, 10)
}

export function SettingsPage() {
  const storage = useStorage()
  const decks = useLiveQuery(() => storage.listDecks(), [])

  const [selected, setSelected] = useState<Set<string>>(new Set())
  const [pending, setPending] = useState<{ decks: DeckBackup[]; replaced: string[] } | null>(null)
  const [message, setMessage] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)

  const list = decks ?? []
  const allSelected = list.length > 0 && list.every((d) => selected.has(d.id))

  function toggle(id: string) {
    setSelected((prev) => {
      const next = new Set(prev)
      if (next.has(id)) next.delete(id)
      else next.add(id)
      return next
    })
  }

  function toggleAll() {
    setSelected(allSelected ? new Set() : new Set(list.map((d) => d.id)))
  }

  async function onExport() {
    const ids = list.filter((d) => selected.has(d.id)).map((d) => d.id)
    const payload = await collectBackup(storage, ids)
    downloadJson(serializeBackup(payload, Date.now()), `rem-backup-${todayStamp()}.json`)
  }

  async function runImport(toImport: DeckBackup[]) {
    const result = await storage.importDecks(toImport)
    setPending(null)
    const replacedNote = result.replaced.length ? ` (replaced ${result.replaced.length})` : ''
    setMessage(`Imported ${toImport.length} deck${toImport.length === 1 ? '' : 's'}${replacedNote}.`)
  }

  async function onFile(e: ChangeEvent<HTMLInputElement>) {
    const file = e.target.files?.[0]
    e.target.value = '' // allow re-selecting the same file later
    if (!file) return
    setError(null)
    setMessage(null)
    let parsed: DeckBackup[]
    try {
      parsed = parseBackup(await file.text())
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Could not read the file.')
      return
    }
    const { replaced } = planImport(
      parsed.map((d) => d.name),
      list.map((d) => d.name),
    )
    if (replaced.length > 0) setPending({ decks: parsed, replaced })
    else await runImport(parsed)
  }

  return (
    <div className="stack">
      <h1 className="page-title">Settings</h1>

      <section className="settings-section">
        <h2>Export decks</h2>
        {list.length === 0 ? (
          <p className="settings-hint">No decks to export yet.</p>
        ) : (
          <>
            <label className="settings-check">
              <input
                type="checkbox"
                aria-label="Select all decks"
                checked={allSelected}
                onChange={toggleAll}
              />
              Select all
            </label>
            {list.map((d) => (
              <label key={d.id} className="settings-check">
                <input
                  type="checkbox"
                  checked={selected.has(d.id)}
                  onChange={() => toggle(d.id)}
                />
                {d.name}
              </label>
            ))}
            <button
              className="btn btn-primary"
              type="button"
              disabled={selected.size === 0}
              onClick={onExport}
            >
              Export selected
            </button>
          </>
        )}
      </section>

      <section className="settings-section">
        <h2>Import decks</h2>
        <p className="settings-hint">Decks with the same name will be replaced.</p>
        <input
          type="file"
          accept="application/json,.json"
          aria-label="Import backup file"
          onChange={onFile}
        />
        {error && <p className="settings-error">{error}</p>}
        {message && <p className="settings-ok">{message}</p>}
        {pending && (
          <div className="settings-warning" role="alertdialog" aria-label="Confirm replace">
            <p>These decks already exist and will be replaced:</p>
            <ul>
              {pending.replaced.map((name) => (
                <li key={name}>{name}</li>
              ))}
            </ul>
            <div className="add-row">
              <button className="btn btn-danger" type="button" onClick={() => runImport(pending.decks)}>
                Replace
              </button>
              <button className="btn btn-ghost" type="button" onClick={() => setPending(null)}>
                Cancel
              </button>
            </div>
          </div>
        )}
      </section>
    </div>
  )
}
```

- [ ] **Step 3b: Add the route**

In `src/app/routes.tsx`, add the import and the child route.

Add with the other feature imports:

```tsx
import { SettingsPage } from '../features/settings/SettingsPage'
```

Add as the last entry in the `children` array (after the `study` route):

```tsx
      { path: 'settings', element: <SettingsPage /> },
```

- [ ] **Step 3c: Add the header link**

Replace the body of `src/ui/Layout.tsx` with (adds a `Link` import and a `.header-actions` cluster):

```tsx
import { Link, Outlet } from 'react-router-dom'
import { ThemeToggle } from './ThemeToggle'

export function Layout() {
  return (
    <div className="app">
      <header className="app-header">
        <Link to="/" className="app-title">
          rem
        </Link>
        <div className="header-actions">
          <Link to="/settings" className="settings-link" aria-label="Settings">
            ⚙
          </Link>
          <ThemeToggle />
        </div>
      </header>
      <main className="app-main">
        <Outlet />
      </main>
    </div>
  )
}
```

- [ ] **Step 3d: Append the styles**

Append to `src/ui/styles.css`:

```css
/* Header actions cluster */
.header-actions {
  display: flex;
  align-items: center;
  gap: 8px;
}

.settings-link {
  width: 30px;
  height: 30px;
  border-radius: var(--radius-sm);
  border: 1px solid var(--border-strong);
  background: var(--surface);
  color: var(--muted);
  font-size: 14px;
  line-height: 1;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  text-decoration: none;
}

.settings-link:hover {
  color: var(--text);
}

/* Settings page */
.settings-section {
  display: flex;
  flex-direction: column;
  gap: 10px;
  padding: 16px 0;
  border-bottom: 1px solid var(--border);
}

.settings-section h2 {
  font-size: var(--text-lg);
  margin: 0;
}

.settings-hint {
  color: var(--muted);
  font-size: var(--text-sm);
  margin: 0;
}

.settings-check {
  display: flex;
  align-items: center;
  gap: 8px;
}

.settings-error {
  color: var(--danger);
  margin: 0;
}

.settings-ok {
  color: var(--muted);
  margin: 0;
}

.settings-warning {
  border: 1px solid var(--border-strong);
  border-radius: var(--radius-sm);
  padding: 12px;
  background: var(--surface);
}
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `npm test -- --project browser src/features/settings/SettingsPage.browser.test.tsx`
Expected: PASS (both tests).

- [ ] **Step 5: Typecheck and commit**

Run: `npm run typecheck`
Expected: no errors.

```bash
git add src/features/settings/SettingsPage.tsx src/features/settings/SettingsPage.browser.test.tsx src/app/routes.tsx src/ui/Layout.tsx src/ui/styles.css
git commit -m "feat: add settings page with deck export/import"
```

---

### Task 4: Add Settings to the screenshot sweep

**Files:**
- Modify: `src/test/screens.browser.test.tsx`

**Interfaces:**
- Consumes: `SettingsPage` (Task 3), existing `renderRoute` / `freshStorage` / `shoot` helpers.

- [ ] **Step 1: Add the scenario**

In `src/test/screens.browser.test.tsx`, add the import alongside the other page imports:

```tsx
import { SettingsPage } from '../features/settings/SettingsPage'
```

Add a scenario object to the `scenarios` array (e.g. after `deck-list`):

```tsx
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
```

- [ ] **Step 2: Run the sweep to verify it passes**

Run: `npm test -- --project browser src/test/screens.browser.test.tsx`
Expected: PASS — `settings — light` and `settings — dark` both green; screenshots land in `test-artifacts/settings.png` and `test-artifacts/settings-dark.png`.

- [ ] **Step 3: Commit**

```bash
git add src/test/screens.browser.test.tsx
git commit -m "test: add settings page to screenshot sweep"
```

---

### Final verification

- [ ] **Run the full suite**

Run: `npm test`
Expected: all unit + browser tests pass.

- [ ] **Build + typecheck**

Run: `npm run build`
Expected: `tsc --noEmit` clean and Vite build succeeds.

---

## Self-review notes

- **Spec coverage:** Settings surface (Task 3 route + header), backup format + IDs-omitted + full fidelity (Task 1), export with selection (Task 3), import replace-by-name + warning/confirm (Tasks 2 + 3), deterministic multi-match replace (Task 2 test), all error cases (Task 1 `parseBackup` tests + Task 3 inline error), and tests at unit/integration/browser levels (Tasks 1, 2, 3, 4). All mapped.
- **Type consistency:** `DeckBackup` / `CardBackup` / `BackupFile` defined in Task 1 and consumed unchanged in Tasks 2–4; `ImportResult { added, replaced }` defined in Task 2 and used by `SettingsPage`; `planImport` signature identical across `backup.ts`, `DexieStorage`, and `SettingsPage`.
- **Deferred (YAGNI, per spec):** merge-mode import, cross-device ID matching, empty future settings sections.
