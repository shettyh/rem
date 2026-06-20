# Real-browser UI Testing Capability — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give `rem` a committed, repeatable way to render components and full pages in real Chromium with screenshots, then use it to produce an enumerated list of the UI issues that feeds the redesign sub-project.

**Architecture:** Add Vitest Browser Mode as a second Vitest *project* (`browser`) alongside the existing `unit` (jsdom) project, so `npm test` runs both and the 22 existing tests are untouched. Tests render React pages with `vitest-browser-react` against a freshly-seeded `DexieStorage` (real IndexedDB in the browser) and capture screenshots into a gitignored `test-artifacts/` folder. A human/agent reads those PNGs to enumerate concrete UI issues.

**Tech Stack:** Vitest 4 browser mode, `@vitest/browser-playwright` (Playwright provider), `vitest-browser-react`, Playwright Chromium, React 19, react-router 7, Dexie 4.

## Global Constraints

- Existing unit tests (scheduler, storage, review cycle, editor — 22 tests) MUST remain unmodified and green. New work only adds a second project + new files; the one exception is a backward-compatible optional prop on `StorageProvider`.
- `npm test` MUST remain the single entry point and run both projects.
- All screenshot-producing test files MUST live in `src/test/` so the screenshot `path` (resolved relative to the test file) consistently reaches repo-root `test-artifacts/` via `../../test-artifacts/`.
- Screenshots go to `test-artifacts/` which is gitignored (never commit PNGs).
- Surgical changes only — match existing code style; do not refactor unrelated code.
- Browser test files are named `*.browser.test.tsx`; the `unit` project excludes them, the `browser` project includes only them.

---

### Task 1: Stand up the Vitest browser-mode pipeline

Proves the whole chain works end-to-end: real Chromium launches, a React component mounts, and a screenshot lands on disk — before any app-specific code.

**Files:**
- Modify: `package.json` (dev deps, via `npm i -D`)
- Modify: `vite.config.ts`
- Create: `src/test/browser-setup.ts`
- Create: `src/test/smoke.browser.test.tsx`
- Modify: `.gitignore`

**Interfaces:**
- Consumes: nothing (first task).
- Produces:
  - A `browser` Vitest project that runs `src/**/*.browser.test.tsx` in headless Chromium at viewport 1280×800.
  - `src/test/browser-setup.ts` — setup file that imports the app stylesheet so real CSS applies in browser tests.
  - The convention: screenshot tests live in `src/test/` and write to `../../test-artifacts/<name>.png`.

- [ ] **Step 1: Write the smoke test (will fail — no browser project yet)**

Create `src/test/smoke.browser.test.tsx`:

```tsx
import { test, expect } from 'vitest'
import { render } from 'vitest-browser-react'
import { page } from 'vitest/browser'

test('browser mode mounts a component and writes a screenshot', async () => {
  render(<button data-testid="smoke">Hello rem</button>)
  await expect.element(page.getByTestId('smoke')).toBeVisible()
  const path = await page.getByTestId('smoke').screenshot({ path: '../../test-artifacts/smoke.png' })
  expect(path).toBeTruthy()
})
```

- [ ] **Step 2: Run it to confirm it fails**

Run: `npx vitest run --project browser`
Expected: FAIL — Vitest reports no project named `browser` (or cannot resolve `vitest-browser-react` / `vitest/browser`). This confirms the pipeline isn't wired yet.

- [ ] **Step 3: Install browser-mode dependencies + Chromium**

Run:
```bash
npm i -D @vitest/browser-playwright vitest-browser-react playwright
npx playwright install chromium
```
Expected: packages added to `devDependencies`; Playwright downloads a Chromium build (one-time, large download).

Note: if a later step errors that `vitest/browser` cannot be resolved, also run `npm i -D @vitest/browser` and re-run; on Vitest 4 the `page` export usually resolves from the `vitest/browser` subpath without it.

- [ ] **Step 4: Split the Vitest config into `unit` + `browser` projects**

Replace the entire contents of `vite.config.ts` with:

```ts
/// <reference types="vitest/config" />
import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import { playwright } from '@vitest/browser-playwright'

export default defineConfig({
  plugins: [react()],
  test: {
    projects: [
      {
        extends: true,
        test: {
          name: 'unit',
          globals: true,
          environment: 'jsdom',
          setupFiles: './src/test/setup.ts',
          include: ['src/**/*.test.{ts,tsx}'],
          exclude: ['src/**/*.browser.test.tsx', 'node_modules/**'],
        },
      },
      {
        extends: true,
        test: {
          name: 'browser',
          globals: true,
          setupFiles: './src/test/browser-setup.ts',
          include: ['src/**/*.browser.test.tsx'],
          browser: {
            enabled: true,
            provider: playwright(),
            headless: true,
            instances: [{ browser: 'chromium', viewport: { width: 1280, height: 800 } }],
          },
        },
      },
    ],
  },
})
```

Note: the `unit` project's `exclude` is what keeps `*.browser.test.tsx` (which also matches `*.test.tsx`) out of jsdom; the `browser` project's `include` is what keeps everything else out of Chromium.

- [ ] **Step 5: Create the browser setup file**

Create `src/test/browser-setup.ts`:

```ts
// Applies the app's real CSS inside the browser test project so screenshots
// reflect production styling. The unit (jsdom) project uses ./setup.ts instead.
import '../ui/styles.css'
```

- [ ] **Step 6: Ignore the screenshot output and create the folder**

Append to `.gitignore`:

```
test-artifacts
```

Run: `mkdir -p test-artifacts`
Expected: folder exists (so the first screenshot write has a destination even if the provider does not auto-create it).

- [ ] **Step 7: Run the smoke test to confirm it passes**

Run: `npx vitest run --project browser`
Expected: PASS (1 test). A file `test-artifacts/smoke.png` now exists.

Verify the PNG is real: `ls -la test-artifacts/smoke.png` (non-zero size).

- [ ] **Step 8: Run the full suite to confirm both projects are green**

Run: `npm test`
Expected: PASS — 23 tests total (22 existing `unit` + 1 `browser` smoke). The existing unit tests are unchanged.

- [ ] **Step 9: Commit**

```bash
git add package.json package-lock.json vite.config.ts src/test/browser-setup.ts src/test/smoke.browser.test.tsx .gitignore
git commit -m "test: add Vitest browser-mode project (real Chromium + screenshots)"
```

---

### Task 2: Page-render test seam + helpers (proven on the deck list)

Adds a dependency-injection seam so tests can supply a freshly-seeded storage, plus three small helpers (`freshStorage`, `renderRoute`, `shoot`). Proven by rendering the real `DeckListPage` with seeded decks and screenshotting it.

**Files:**
- Modify: `src/data/StorageContext.tsx` (add optional `storage` prop — backward compatible)
- Create: `src/test/seed.ts`
- Create: `src/test/screenshot.ts`
- Create: `src/test/renderRoute.tsx`
- Create: `src/test/screens.browser.test.tsx` (deck-list case only; expanded in Task 3)

**Interfaces:**
- Consumes: the `browser` project + `src/test/` screenshot convention from Task 1.
- Produces:
  - `StorageProvider` now accepts an optional `storage?: Storage` prop (defaults to the app singleton). `useStorage()` is unchanged.
  - `freshStorage(): Storage` — a new isolated `DexieStorage` backed by a uniquely-named IndexedDB, for one test. Re-exports `MS_PER_DAY`.
  - `shoot(testId: string, name: string): Promise<void>` — screenshots the element with `data-testid={testId}` to `test-artifacts/<name>.png`.
  - `renderRoute({ storage, entry, path, element }): RenderResult` — mounts a page element under `Layout` + `MemoryRouter`, wrapped in `<div data-testid="screen">` for screenshotting.

- [ ] **Step 1: Write the failing deck-list screenshot test**

Create `src/test/screens.browser.test.tsx`:

```tsx
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
```

- [ ] **Step 2: Run it to confirm it fails**

Run: `npx vitest run --project browser screens`
Expected: FAIL — cannot resolve `./seed`, `./renderRoute`, or `./screenshot` (modules don't exist yet).

- [ ] **Step 3: Add the optional `storage` prop to `StorageProvider`**

Replace the contents of `src/data/StorageContext.tsx` with:

```tsx
import { createContext, useContext, type ReactNode } from 'react'
import { scheduler } from '../domain/scheduler'
import type { Storage } from './Storage'
import { DexieStorage } from './dexie/DexieStorage'
import { RemDB } from './dexie/db'

/** The single app-wide storage instance (IndexedDB via Dexie). */
const defaultStorage: Storage = new DexieStorage(new RemDB(), scheduler)

const StorageContext = createContext<Storage>(defaultStorage)

export function StorageProvider({
  children,
  storage = defaultStorage,
}: {
  children: ReactNode
  /** Override the storage instance — used by tests to inject seeded data. */
  storage?: Storage
}) {
  return <StorageContext.Provider value={storage}>{children}</StorageContext.Provider>
}

/** Access the app-wide {@link Storage}. */
export function useStorage(): Storage {
  return useContext(StorageContext)
}
```

This only renames the private singleton (`storage` → `defaultStorage`) and adds an optional prop. Existing callers — `<StorageProvider>{...}</StorageProvider>` in `main.tsx` — are unaffected.

- [ ] **Step 4: Create the seed helper**

Create `src/test/seed.ts`:

```ts
import { DexieStorage } from '../data/dexie/DexieStorage'
import { RemDB } from '../data/dexie/db'
import { scheduler, MS_PER_DAY } from '../domain/scheduler'
import type { Storage } from '../data/Storage'

let counter = 0

/**
 * A fresh, isolated IndexedDB-backed Storage for a single test. Each call uses a
 * unique database name so tests never share state.
 */
export function freshStorage(): Storage {
  counter += 1
  const name = `rem-test-${Date.now()}-${counter}`
  return new DexieStorage(new RemDB(name), scheduler)
}

export { MS_PER_DAY }
```

- [ ] **Step 5: Create the screenshot helper**

Create `src/test/screenshot.ts`:

```ts
import { page } from 'vitest/browser'

/**
 * Screenshot the element tagged with `data-testid={testId}` into
 * test-artifacts/<name>.png. Paths are relative to the calling test file, so all
 * screenshot tests must live in src/test/ for `../../` to reach repo root.
 */
export async function shoot(testId: string, name: string): Promise<void> {
  await page.getByTestId(testId).screenshot({ path: `../../test-artifacts/${name}.png` })
}
```

- [ ] **Step 6: Create the route-render helper**

Create `src/test/renderRoute.tsx`:

```tsx
import type { ReactElement } from 'react'
import { MemoryRouter, Routes, Route } from 'react-router-dom'
import { render } from 'vitest-browser-react'
import { Layout } from '../ui/Layout'
import { StorageProvider } from '../data/StorageContext'
import type { Storage } from '../data/Storage'

/**
 * Mount a page element at `path` (visited via `entry`) under the real Layout and a
 * MemoryRouter, with `storage` injected. Wrapped in a screenshot target div.
 */
export function renderRoute(opts: {
  storage: Storage
  /** Route pattern, e.g. '/decks/:deckId'. */
  path: string
  /** URL actually visited, e.g. '/decks/abc'. */
  entry: string
  element: ReactElement
}) {
  return render(
    <div data-testid="screen">
      <StorageProvider storage={opts.storage}>
        <MemoryRouter initialEntries={[opts.entry]}>
          <Routes>
            <Route element={<Layout />}>
              <Route path={opts.path} element={opts.element} />
            </Route>
          </Routes>
        </MemoryRouter>
      </StorageProvider>
    </div>,
  )
}
```

- [ ] **Step 7: Run the deck-list test to confirm it passes**

Run: `npx vitest run --project browser screens`
Expected: PASS (1 test). `test-artifacts/deck-list.png` exists and shows the "rem" header plus two deck rows with due badges.

- [ ] **Step 8: Typecheck + full suite**

Run: `npm run typecheck && npm test`
Expected: typecheck clean; all tests pass (`unit` 22 + `browser` smoke + deck-list).

- [ ] **Step 9: Commit**

```bash
git add src/data/StorageContext.tsx src/test/seed.ts src/test/screenshot.ts src/test/renderRoute.tsx src/test/screens.browser.test.tsx
git commit -m "test: page-render seam + screenshot helpers, proven on deck list"
```

---

### Task 3: Capture every screen + write the UI issues document

Expands `screens.browser.test.tsx` to cover every primary screen and state, then reads the PNGs and writes the enumerated issues list that feeds the redesign.

**Files:**
- Modify: `src/test/screens.browser.test.tsx` (add all remaining cases)
- Create: `docs/superpowers/specs/2026-06-20-ui-issues.md`

**Interfaces:**
- Consumes: `freshStorage`/`MS_PER_DAY`, `renderRoute`, `shoot` from Task 2.
- Produces: `test-artifacts/*.png` for every screen/state below, and a committed issues document.

- [ ] **Step 1: Replace `src/test/screens.browser.test.tsx` with the full screen sweep**

Replace the entire file contents with:

```tsx
import { test, expect } from 'vitest'
import { page } from 'vitest/browser'
import { DeckListPage } from '../features/decks/DeckListPage'
import { DeckDetailPage } from '../features/cards/DeckDetailPage'
import { CardEditorPage } from '../features/cards/CardEditorPage'
import { ReviewPage } from '../features/review/ReviewPage'
import { freshStorage, MS_PER_DAY } from './seed'
import { renderRoute } from './renderRoute'
import { shoot } from './screenshot'

const CODE_BACK = 'Use a guard:\n\n```ts\nfunction f(x: unknown) {\n  if (typeof x === "string") return x\n}\n```'

async function pushToFuture(storage: ReturnType<typeof freshStorage>, cardId: string) {
  const card = await storage.getCard(cardId)
  if (!card) throw new Error('seed card missing')
  await storage.updateCard(cardId, { scheduling: { ...card.scheduling, due: Date.now() + 10 * MS_PER_DAY } })
}

test('deck list — with decks', async () => {
  const storage = freshStorage()
  await storage.createDeck('TypeScript')
  await storage.createDeck('Spanish vocabulary')
  renderRoute({ storage, entry: '/', path: '/', element: <DeckListPage /> })
  await expect.element(page.getByText('TypeScript')).toBeVisible()
  await shoot('screen', 'deck-list')
})

test('deck list — empty', async () => {
  const storage = freshStorage()
  renderRoute({ storage, entry: '/', path: '/', element: <DeckListPage /> })
  await expect.element(page.getByText('No decks yet.', { exact: false })).toBeVisible()
  await shoot('screen', 'deck-list-empty')
})

test('deck detail — with due cards', async () => {
  const storage = freshStorage()
  const deck = await storage.createDeck('TypeScript')
  await storage.createCard(deck.id, 'What is the `never` type?', 'The empty type.')
  await storage.createCard(deck.id, 'What does `satisfies` do?', 'Checks without widening.')
  renderRoute({ storage, entry: `/decks/${deck.id}`, path: '/decks/:deckId', element: <DeckDetailPage /> })
  await expect.element(page.getByText('TypeScript')).toBeVisible()
  await expect.element(page.getByText('Study', { exact: false })).toBeVisible()
  await shoot('screen', 'deck-detail')
})

test('deck detail — nothing due', async () => {
  const storage = freshStorage()
  const deck = await storage.createDeck('TypeScript')
  const card = await storage.createCard(deck.id, 'Reviewed already', 'Yes')
  await pushToFuture(storage, card.id)
  renderRoute({ storage, entry: `/decks/${deck.id}`, path: '/decks/:deckId', element: <DeckDetailPage /> })
  await expect.element(page.getByText('Nothing due')).toBeVisible()
  await shoot('screen', 'deck-detail-nothing-due')
})

test('deck detail — empty', async () => {
  const storage = freshStorage()
  const deck = await storage.createDeck('Empty deck')
  renderRoute({ storage, entry: `/decks/${deck.id}`, path: '/decks/:deckId', element: <DeckDetailPage /> })
  await expect.element(page.getByText('No cards yet.', { exact: false })).toBeVisible()
  await shoot('screen', 'deck-detail-empty')
})

test('card editor — new', async () => {
  const storage = freshStorage()
  const deck = await storage.createDeck('TypeScript')
  renderRoute({
    storage,
    entry: `/decks/${deck.id}/cards/new`,
    path: '/decks/:deckId/cards/new',
    element: <CardEditorPage />,
  })
  await expect.element(page.getByText('New card')).toBeVisible()
  await shoot('screen', 'card-editor-new')
})

test('card editor — edit with code block', async () => {
  const storage = freshStorage()
  const deck = await storage.createDeck('TypeScript')
  const card = await storage.createCard(deck.id, 'How to narrow `unknown`?', CODE_BACK)
  renderRoute({
    storage,
    entry: `/decks/${deck.id}/cards/${card.id}`,
    path: '/decks/:deckId/cards/:cardId',
    element: <CardEditorPage />,
  })
  await expect.element(page.getByText('Edit card')).toBeVisible()
  await expect.element(page.getByText('narrow', { exact: false })).toBeVisible()
  await shoot('screen', 'card-editor-edit')
})

test('review — question side', async () => {
  const storage = freshStorage()
  const deck = await storage.createDeck('TypeScript')
  await storage.createCard(deck.id, 'What is the `never` type?', 'The empty type.')
  renderRoute({ storage, entry: `/decks/${deck.id}/study`, path: '/decks/:deckId/study', element: <ReviewPage /> })
  await expect.element(page.getByRole('button', { name: 'Show answer', exact: false })).toBeVisible()
  await shoot('screen', 'review-question')
})

test('review — answer revealed', async () => {
  const storage = freshStorage()
  const deck = await storage.createDeck('TypeScript')
  await storage.createCard(deck.id, 'What is the `never` type?', 'The empty type — no value is assignable to it.')
  renderRoute({ storage, entry: `/decks/${deck.id}/study`, path: '/decks/:deckId/study', element: <ReviewPage /> })
  await page.getByRole('button', { name: 'Show answer', exact: false }).click()
  await expect.element(page.getByText('no value is assignable', { exact: false })).toBeVisible()
  await shoot('screen', 'review-answer')
})

test('review — nothing due', async () => {
  const storage = freshStorage()
  const deck = await storage.createDeck('TypeScript')
  const card = await storage.createCard(deck.id, 'Already reviewed', 'Yes')
  await pushToFuture(storage, card.id)
  renderRoute({ storage, entry: `/decks/${deck.id}/study`, path: '/decks/:deckId/study', element: <ReviewPage /> })
  await expect.element(page.getByText('Nothing due in this deck', { exact: false })).toBeVisible()
  await shoot('screen', 'review-nothing-due')
})

test('review — session complete', async () => {
  const storage = freshStorage()
  const deck = await storage.createDeck('TypeScript')
  await storage.createCard(deck.id, 'Only card', 'Done')
  renderRoute({ storage, entry: `/decks/${deck.id}/study`, path: '/decks/:deckId/study', element: <ReviewPage /> })
  await page.getByRole('button', { name: 'Show answer', exact: false }).click()
  await page.getByRole('button', { name: 'Good', exact: false }).click()
  await expect.element(page.getByText('Review complete', { exact: false })).toBeVisible()
  await shoot('screen', 'review-complete')
})
```

- [ ] **Step 2: Run the full screen sweep**

Run: `npx vitest run --project browser screens`
Expected: PASS (11 tests). Note: if a `getByText` matcher is too strict for highlighted/markdown-split text, loosen it with `{ exact: false }` (already used on the risky ones) rather than changing app code.

- [ ] **Step 3: Confirm every screenshot was written**

Run: `ls -1 test-artifacts/`
Expected to include: `deck-list.png`, `deck-list-empty.png`, `deck-detail.png`, `deck-detail-nothing-due.png`, `deck-detail-empty.png`, `card-editor-new.png`, `card-editor-edit.png`, `review-question.png`, `review-answer.png`, `review-nothing-due.png`, `review-complete.png` (plus `smoke.png`).

- [ ] **Step 4: Inspect each screenshot and enumerate issues**

Read each PNG in `test-artifacts/` (view the image). For each screen, note concrete visual/layout problems: spacing, alignment, contrast, truncation, cramped/overflowing elements, weak hierarchy, code-block legibility, empty-state weakness, button affordance, anything that looks unfinished. Be specific (which element, what's wrong, what it should be).

- [ ] **Step 5: Write the issues document**

Create `docs/superpowers/specs/2026-06-20-ui-issues.md` using this structure (fill every screen; one bullet per concrete issue, referencing the screenshot):

```markdown
# rem — UI issues (baseline)

_Date: 2026-06-20 — captured from `test-artifacts/` via the browser-mode screen sweep._

Input for the UI redesign sub-project (roadmap #2). Each issue references the screenshot it came from.

## Deck list (`deck-list.png`, `deck-list-empty.png`)
- [issue] …

## Deck detail (`deck-detail.png`, `deck-detail-nothing-due.png`, `deck-detail-empty.png`)
- [issue] …

## Card editor (`card-editor-new.png`, `card-editor-edit.png`)
- [issue] …

## Review (`review-question.png`, `review-answer.png`, `review-nothing-due.png`, `review-complete.png`)
- [issue] …

## Cross-cutting (tokens, type scale, spacing, color, motion)
- [issue] …
```

- [ ] **Step 6: Full suite green + commit**

Run: `npm test`
Expected: PASS — `unit` (22) + `browser` (smoke + 11 screen tests).

```bash
git add src/test/screens.browser.test.tsx docs/superpowers/specs/2026-06-20-ui-issues.md
git commit -m "test: capture all screens in Chromium + enumerate UI issues"
```

---

## Self-Review

**Spec coverage:**
- "Render components and full pages in real Chromium" → Task 1 (component smoke) + Task 2/3 (full pages via `renderRoute`). ✓
- "Screenshot capture" → `shoot` helper (Task 2), used across all screens (Task 3). ✓
- "`npm test` one command, existing tests untouched & green" → project split (Task 1), verified in Task 1 Step 8 and Task 2 Step 8. ✓
- "Baseline screenshots of every primary screen + state" → Task 3 covers deck list (2), deck detail (3), card editor (2), review (4). ✓
- "Enumerated UI issues list as deliverable" → Task 3 Steps 4–5, `2026-06-20-ui-issues.md`. ✓
- Out-of-scope items (redesign, export/import, VRT pixel-diffing, migrating the jsdom editor test) → not included. ✓

**Placeholder scan:** No TBD/TODO in code steps; all code is complete. The issues-document content is genuine discovery output (the plan fixes its format and required sections, which is the most that can be pre-written).

**Type consistency:** `freshStorage()` returns `Storage`; `renderRoute` consumes `Storage`; `shoot(testId, name)` matches all call sites; `StorageProvider`'s new `storage?: Storage` prop is consumed by `renderRoute`. `MS_PER_DAY` is imported from `../domain/scheduler` (verified exported) and re-exported by `seed.ts`. Test ids: `renderRoute` emits `data-testid="screen"`; every `shoot('screen', …)` call matches.
