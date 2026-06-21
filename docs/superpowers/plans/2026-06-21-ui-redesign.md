# UI Redesign Pass Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace rem's plain-CSS surface with the "Editorial / calm focus" design system — light + dark themes, a true 3D study-card flip, and per-surface fixes for every item in the 2026-06-20 UI issues list.

**Architecture:** A CSS custom-property token layer (`tokens.css`) drives every color, space, and type decision, with a `[data-theme='dark']` override block. A tiny `theme.ts` module persists the choice and an inline `index.html` script applies it before paint (no flash). Components change minimally — mostly class/markup tweaks; the study screen gains a `preserve-3d` flip whose card height is measured at runtime so long answers scroll inside the card while the grade bar stays pinned outside it.

**Tech Stack:** React 19, react-router 7, Vite 8, Vitest 4 (two projects: `unit`/jsdom + `browser`/Playwright Chromium @1280×800), Dexie, TipTap 3.

## Global Constraints

- **No `Storage` interface change.** Deck card-counts come from `listCards(deckId).length`; card status comes from existing `card.scheduling`.
- **Personality = "Editorial / calm focus."** Card content (review faces + editor content) uses the **serif** stack; all UI chrome uses **sans**; code stays **mono**. Code blocks are dark in both themes.
- **Theme key** is `localStorage['rem-theme']`, values `'light'` | `'dark'`; default follows `prefers-color-scheme`.
- **Token names are stable.** Components consume `var(--bg/--surface/--border/--text/--muted/--accent/--accent-hover/--radius/--shadow)` — keep these names so existing styles keep working.
- **Out of scope:** export/import, FSRS, embeddings, generation. Surface only data we already store.
- **Verify gates** (run from repo root): `npm test` (both projects), `npm run build` (`tsc --noEmit && vite build`). Single project: `npx vitest run --project unit <file>` / `--project browser <file>`.
- **Commit** after each task. Branch is `ui-redesign` (already created). End commit messages with the Co-Authored-By trailer used on this branch.

---

## File Structure

**New**
- `src/ui/tokens.css` — design tokens: color roles (light + `[data-theme='dark']`), spacing scale, type scale, radii, shadow, motion. The single source of truth for `:root` variables.
- `src/ui/theme.ts` — `Theme` type + `systemTheme`/`getStoredTheme`/`resolveInitialTheme`/`applyTheme` + `THEME_KEY`.
- `src/ui/ThemeToggle.tsx` — header button that flips light/dark via `applyTheme`.
- `src/ui/theme.test.ts` — unit tests for `theme.ts`.
- `src/ui/ThemeToggle.test.tsx` — unit test for the toggle.
- `src/test/theme-tokens.browser.test.tsx` — asserts `--bg` resolves to the light value by default and the dark value under `[data-theme='dark']`.

**Modified**
- `index.html` — inline no-FOUC theme bootstrap in `<head>`.
- `src/ui/styles.css` — `@import './tokens.css'`, remove the old `:root` block, token-driven component/surface/flip/kbd/empty-state styles.
- `src/ui/Layout.tsx` — tightened header containing `ThemeToggle`.
- `src/features/decks/DeckListPage.tsx` — card-count + conditional due chip, stronger empty state, neutral disabled button.
- `src/features/cards/DeckDetailPage.tsx` — study-control logic, per-card status tags, empty-state CTA.
- `src/features/cards/CardEditorPage.tsx` — `FRONT`/`BACK` labels, neutral disabled.
- `src/features/cards/RichMarkdownEditor.tsx` — (styling only via `styles.css`; no structural change required).
- `src/features/review/ReviewPage.tsx` — flip structure + measured height + internal scroll + pinned grade bar + `data-revealed`, stronger end states, ghost affordance.
- `src/features/review/GradeButtons.tsx` — kbd chips + token-derived grade colors.
- `src/test/screens.browser.test.tsx` — loop the sweep over light + dark; update changed-copy assertions.

---

## Task 1: Design tokens & dark-ready color layer

**Files:**
- Create: `src/ui/tokens.css`
- Create: `src/test/theme-tokens.browser.test.tsx`
- Modify: `src/ui/styles.css:1-12` (replace the `:root` block with an `@import`)

**Interfaces:**
- Produces: CSS custom properties on `:root` (light) and `[data-theme='dark']` (dark) — names listed in Global Constraints plus `--surface-inset`, `--border-strong`, `--accent-soft`, `--code-bg`, `--code-fg`, `--again`, `--hard`, `--good`, `--easy`, `--danger`, `--space-1..8`, `--text-xs..2xl`, `--font-sans/serif/mono`, `--radius-sm/md/lg`, `--shadow-sm/md`, `--flip-ms`.

- [ ] **Step 1: Write the failing test**

Create `src/test/theme-tokens.browser.test.tsx`:

```tsx
import { test, expect, afterEach } from 'vitest'
import '../ui/styles.css'

afterEach(() => {
  delete document.documentElement.dataset.theme
})

function bg(): string {
  return getComputedStyle(document.documentElement).getPropertyValue('--bg').trim()
}

test('--bg resolves to the light paper value by default', () => {
  expect(bg()).toBe('#f7f5f1')
})

test('--bg resolves to the warm-dark value under [data-theme=dark]', () => {
  document.documentElement.dataset.theme = 'dark'
  expect(bg()).toBe('#1a1815')
})
```

- [ ] **Step 2: Run test to verify it fails**

Run: `npx vitest run --project browser theme-tokens`
Expected: FAIL — default `--bg` is currently `#fafafa`, not `#f7f5f1`; the dark assertion gets an empty string (no `[data-theme]` rule exists).

- [ ] **Step 3: Create `src/ui/tokens.css`**

```css
/* Design tokens — the single source of truth for colors, spacing, type, motion.
   Light values on :root; warm-dark overrides under [data-theme='dark']. */
:root {
  /* color roles */
  --bg: #f7f5f1;
  --surface: #fffefb;
  --surface-inset: #efe9dd;
  --border: #e7e3da;
  --border-strong: #ddd6c9;
  --text: #1c1a17;
  --muted: #8a8578;
  --accent: #6534c9;
  --accent-hover: #5a2db0;
  --accent-soft: #efe9dd;
  --code-bg: #211f2e;
  --code-fg: #eceaf6;

  /* grade + semantic roles */
  --again: #d6553f;
  --hard: #cf8a3a;
  --good: #4c9a6a;
  --easy: var(--accent);
  --danger: #b91c1c;

  /* spacing scale */
  --space-1: 4px;  --space-2: 8px;  --space-3: 12px; --space-4: 16px;
  --space-5: 24px; --space-6: 32px; --space-7: 48px; --space-8: 64px;

  /* type scale */
  --text-xs: 12px; --text-sm: 13px; --text-base: 15px;
  --text-lg: 18px; --text-xl: 21px; --text-2xl: 25px;
  --font-sans: ui-sans-serif, system-ui, -apple-system, 'Segoe UI', Roboto, sans-serif;
  --font-serif: Georgia, 'Times New Roman', serif;
  --font-mono: ui-monospace, SFMono-Regular, 'SF Mono', Menlo, monospace;

  /* radii / shadow / motion */
  --radius-sm: 6px; --radius-md: 11px; --radius-lg: 16px;
  --radius: var(--radius-md);
  --shadow-sm: 0 1px 2px rgba(40, 30, 10, 0.05);
  --shadow-md: 0 4px 18px rgba(40, 30, 10, 0.06);
  --shadow: var(--shadow-sm);
  --flip-ms: 600ms;

  color-scheme: light;
}

[data-theme='dark'] {
  --bg: #1a1815;
  --surface: #232019;
  --surface-inset: #2c2820;
  --border: #34302a;
  --border-strong: #423d35;
  --text: #f2ede3;
  --muted: #a39d8f;
  --accent: #b9a0f5;
  --accent-hover: #c9b6f8;
  --accent-soft: #2c2820;
  --code-bg: #2a2733;
  --code-fg: #eceaf6;
  --again: #e87a63;
  --hard: #e0a85a;
  --good: #6fc28e;
  --danger: #f08a7a;
  --shadow-sm: 0 1px 2px rgba(0, 0, 0, 0.3);
  --shadow-md: 0 6px 22px rgba(0, 0, 0, 0.4);

  color-scheme: dark;
}
```

- [ ] **Step 4: Wire tokens into `styles.css`**

Replace the existing `:root { ... }` block at the top of `src/ui/styles.css` (lines 1–12) with a single import as the **first line** of the file:

```css
@import './tokens.css';
```

(Leave the rest of `styles.css` untouched in this task — every component already references the token names, so they now read from `tokens.css`.)

- [ ] **Step 5: Run tests to verify they pass**

Run: `npx vitest run --project browser theme-tokens`
Expected: PASS (both assertions).

- [ ] **Step 6: Confirm the existing sweep still renders**

Run: `npx vitest run --project browser screens`
Expected: PASS (screenshots in `test-artifacts/` regenerate with the warm paper background; copy assertions unchanged this task).

- [ ] **Step 7: Commit**

```bash
git add src/ui/tokens.css src/ui/styles.css src/test/theme-tokens.browser.test.tsx test-artifacts
git commit -m "$(printf 'feat: design token layer with light + dark color roles\n\nCo-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>')"
```

---

## Task 2: Theme persistence module

**Files:**
- Create: `src/ui/theme.ts`
- Create: `src/ui/theme.test.ts`

**Interfaces:**
- Produces:
  - `type Theme = 'light' | 'dark'`
  - `const THEME_KEY = 'rem-theme'`
  - `function systemTheme(): Theme` — `'dark'` iff `matchMedia('(prefers-color-scheme: dark)').matches`, else `'light'` (also `'light'` when `matchMedia` is unavailable).
  - `function getStoredTheme(): Theme | null` — reads `localStorage[THEME_KEY]`, returns it only if `'light'`/`'dark'`.
  - `function resolveInitialTheme(): Theme` — `getStoredTheme() ?? systemTheme()`.
  - `function applyTheme(theme: Theme): void` — sets `document.documentElement.dataset.theme = theme` and persists to `localStorage[THEME_KEY]`.

- [ ] **Step 1: Write the failing test**

Create `src/ui/theme.test.ts`:

```ts
import { describe, it, expect, beforeEach } from 'vitest'
import { THEME_KEY, getStoredTheme, resolveInitialTheme, applyTheme } from './theme'

describe('theme', () => {
  beforeEach(() => {
    localStorage.clear()
    delete document.documentElement.dataset.theme
  })

  it('getStoredTheme returns null when nothing valid is stored', () => {
    expect(getStoredTheme()).toBeNull()
    localStorage.setItem(THEME_KEY, 'banana')
    expect(getStoredTheme()).toBeNull()
  })

  it('getStoredTheme returns a stored valid theme', () => {
    localStorage.setItem(THEME_KEY, 'dark')
    expect(getStoredTheme()).toBe('dark')
  })

  it('resolveInitialTheme falls back to system (light in jsdom) when unset', () => {
    expect(resolveInitialTheme()).toBe('light')
  })

  it('applyTheme sets the data attribute and persists', () => {
    applyTheme('dark')
    expect(document.documentElement.dataset.theme).toBe('dark')
    expect(localStorage.getItem(THEME_KEY)).toBe('dark')
  })
})
```

- [ ] **Step 2: Run test to verify it fails**

Run: `npx vitest run --project unit theme.test`
Expected: FAIL — cannot resolve module `./theme`.

- [ ] **Step 3: Implement `src/ui/theme.ts`**

```ts
export type Theme = 'light' | 'dark'

export const THEME_KEY = 'rem-theme'

export function systemTheme(): Theme {
  if (typeof window === 'undefined' || !window.matchMedia) return 'light'
  return window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light'
}

export function getStoredTheme(): Theme | null {
  const stored = localStorage.getItem(THEME_KEY)
  return stored === 'light' || stored === 'dark' ? stored : null
}

export function resolveInitialTheme(): Theme {
  return getStoredTheme() ?? systemTheme()
}

export function applyTheme(theme: Theme): void {
  document.documentElement.dataset.theme = theme
  localStorage.setItem(THEME_KEY, theme)
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `npx vitest run --project unit theme.test`
Expected: PASS (4 tests).

- [ ] **Step 5: Commit**

```bash
git add src/ui/theme.ts src/ui/theme.test.ts
git commit -m "$(printf 'feat: theme persistence module (resolve/apply/store)\n\nCo-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>')"
```

---

## Task 3: No-FOUC bootstrap, ThemeToggle, and header

**Files:**
- Modify: `index.html:1-12` (add inline bootstrap to `<head>`)
- Create: `src/ui/ThemeToggle.tsx`
- Create: `src/ui/ThemeToggle.test.tsx`
- Modify: `src/ui/Layout.tsx`
- Modify: `src/ui/styles.css` (header + theme-toggle styles)

**Interfaces:**
- Consumes: `Theme`, `resolveInitialTheme`, `applyTheme` from `./theme` (Task 2).
- Produces: `<ThemeToggle />` (no props) rendering a `<button class="theme-toggle" aria-label="Toggle theme">`.

- [ ] **Step 1: Write the failing test**

Create `src/ui/ThemeToggle.test.tsx`:

```tsx
import { describe, it, expect, beforeEach } from 'vitest'
import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { ThemeToggle } from './ThemeToggle'
import { THEME_KEY } from './theme'

describe('ThemeToggle', () => {
  beforeEach(() => {
    localStorage.clear()
    document.documentElement.dataset.theme = 'light'
  })

  it('flips the theme and persists it on click', async () => {
    render(<ThemeToggle />)
    const btn = screen.getByRole('button', { name: /toggle theme/i })
    await userEvent.click(btn)
    expect(document.documentElement.dataset.theme).toBe('dark')
    expect(localStorage.getItem(THEME_KEY)).toBe('dark')
    await userEvent.click(btn)
    expect(document.documentElement.dataset.theme).toBe('light')
    expect(localStorage.getItem(THEME_KEY)).toBe('light')
  })
})
```

- [ ] **Step 2: Run test to verify it fails**

Run: `npx vitest run --project unit ThemeToggle`
Expected: FAIL — cannot resolve module `./ThemeToggle`.

- [ ] **Step 3: Implement `src/ui/ThemeToggle.tsx`**

```tsx
import { useState } from 'react'
import { type Theme, resolveInitialTheme, applyTheme } from './theme'

/** Header button that flips between light and dark and persists the choice. */
export function ThemeToggle() {
  const [theme, setTheme] = useState<Theme>(resolveInitialTheme)

  function toggle() {
    const next: Theme = theme === 'dark' ? 'light' : 'dark'
    applyTheme(next)
    setTheme(next)
  }

  return (
    <button
      type="button"
      className="theme-toggle"
      aria-label="Toggle theme"
      onClick={toggle}
    >
      {theme === 'dark' ? '☾' : '☀︎'}
    </button>
  )
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `npx vitest run --project unit ThemeToggle`
Expected: PASS.

- [ ] **Step 5: Add the no-FOUC bootstrap to `index.html`**

Insert this script inside `<head>`, immediately after the `<title>` line, so the theme attribute is set before the stylesheet paints:

```html
    <script>
      (function () {
        try {
          var t = localStorage.getItem('rem-theme')
          if (t !== 'light' && t !== 'dark') {
            t = window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light'
          }
          document.documentElement.dataset.theme = t
        } catch (e) {}
      })()
    </script>
```

- [ ] **Step 6: Mount the toggle in `Layout.tsx`**

Replace the contents of `src/ui/Layout.tsx` with:

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
        <ThemeToggle />
      </header>
      <main className="app-main">
        <Outlet />
      </main>
    </div>
  )
}
```

- [ ] **Step 7: Style the header + toggle**

In `src/ui/styles.css`, replace the `.app-header` and `.app-title` rules with the following (tightens the top-heavy header and lays out the toggle on the right):

```css
.app-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  height: 48px;
  border-bottom: 1px solid var(--border);
}

.app-title {
  font-weight: 800;
  font-size: var(--text-lg);
  letter-spacing: -0.03em;
  color: var(--text);
  text-decoration: none;
}

.theme-toggle {
  width: 30px;
  height: 30px;
  border-radius: var(--radius-sm);
  border: 1px solid var(--border-strong);
  background: var(--surface);
  color: var(--muted);
  cursor: pointer;
  font-size: 14px;
  line-height: 1;
  display: inline-flex;
  align-items: center;
  justify-content: center;
}

.theme-toggle:hover {
  color: var(--text);
}
```

- [ ] **Step 8: Verify build + the toggle test**

Run: `npm run build`
Expected: PASS (typecheck + bundle).
Run: `npx vitest run --project unit ThemeToggle theme.test`
Expected: PASS.

- [ ] **Step 9: Commit**

```bash
git add index.html src/ui/ThemeToggle.tsx src/ui/ThemeToggle.test.tsx src/ui/Layout.tsx src/ui/styles.css
git commit -m "$(printf 'feat: dark-mode toggle with no-flash bootstrap and tightened header\n\nCo-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>')"
```

---

## Task 4: Shared style patterns — buttons, kbd chip, empty state

**Files:**
- Modify: `src/ui/styles.css` (Buttons, plus new `.kbd` and `.empty-state` patterns)

**Interfaces:**
- Produces CSS classes consumed by later tasks: neutral `.btn:disabled`; visible `.btn-ghost`; `.kbd`; `.empty-state` (with `.empty-state .ico`, `.empty-state h3`, `.empty-state p`).

This task is pure CSS; verification is the screen sweep staying green plus visual inspection against the spec mockups (`.superpowers/brainstorm/`).

- [ ] **Step 1: Replace the Buttons section in `styles.css`**

Replace the disabled and ghost button rules so disabled reads as a deliberate neutral state and ghost buttons have a visible affordance:

```css
.btn:disabled {
  background: var(--surface-inset);
  border-color: transparent;
  color: var(--muted);
  cursor: not-allowed;
}

.btn-ghost {
  border-color: var(--border-strong);
  background: var(--surface);
}

.btn-ghost:hover {
  background: var(--surface-inset);
}
```

(Keep the existing `.btn`, `.btn-primary`, `.btn-danger` rules; they already use tokens.)

- [ ] **Step 2: Add the kbd chip pattern**

Append to `styles.css`:

```css
/* Keyboard hint chip */
.kbd {
  font-family: var(--font-mono);
  font-size: 11px;
  font-weight: 600;
  line-height: 1;
  padding: 2px 6px;
  border-radius: var(--radius-sm);
  background: var(--surface-inset);
  color: var(--muted);
}

.btn-primary .kbd {
  background: rgba(255, 255, 255, 0.22);
  color: #fff;
}
```

- [ ] **Step 3: Add the empty-state pattern**

Append to `styles.css`:

```css
/* Shared empty / completion state */
.empty-state {
  text-align: center;
  padding: var(--space-6) 0 var(--space-5);
}

.empty-state .ico {
  font-size: 30px;
  line-height: 1;
}

.empty-state h3 {
  font-family: var(--font-serif);
  font-size: var(--text-xl);
  color: var(--text);
  margin: var(--space-3) 0 var(--space-1);
}

.empty-state p {
  color: var(--muted);
  font-size: var(--text-base);
  margin: 0 auto;
  max-width: 38ch;
}

.empty-state .cta {
  margin-top: var(--space-4);
  display: inline-flex;
}
```

- [ ] **Step 4: Verify the sweep still passes**

Run: `npx vitest run --project browser screens`
Expected: PASS (no copy changed yet; buttons restyle in regenerated screenshots).

- [ ] **Step 5: Commit**

```bash
git add src/ui/styles.css test-artifacts
git commit -m "$(printf 'feat: shared button/kbd/empty-state patterns\n\nCo-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>')"
```

---

## Task 5: Deck list redesign

**Files:**
- Modify: `src/features/decks/DeckListPage.tsx`
- Modify: `src/ui/styles.css` (deck-row + add-row styles)
- Modify: `src/test/screens.browser.test.tsx` (empty-state copy assertion)

**Interfaces:**
- Consumes: `.empty-state`, neutral `.btn:disabled` (Task 4); `storage.listCards`, `storage.countDue`.

- [ ] **Step 1: Update the empty-state assertion (failing test first)**

In `src/test/screens.browser.test.tsx`, change the `deck list — empty` assertion from `'No decks yet.'` to the new copy:

```tsx
  await expect.element(page.getByText('No decks yet', { exact: false })).toBeVisible()
```

Run: `npx vitest run --project browser "screens"`
Expected: still PASS currently (substring matches), but the row/chip changes below need the new render. Proceed.

- [ ] **Step 2: Rewrite `DeckListPage.tsx`**

```tsx
import { useState, type FormEvent } from 'react'
import { Link } from 'react-router-dom'
import { useLiveQuery } from 'dexie-react-hooks'
import { useStorage } from '../../data/StorageContext'

export function DeckListPage() {
  const storage = useStorage()
  const [name, setName] = useState('')

  const decks = useLiveQuery(async () => {
    const all = await storage.listDecks()
    const now = Date.now()
    return Promise.all(
      all.map(async (deck) => ({
        deck,
        due: await storage.countDue(deck.id, now),
        count: (await storage.listCards(deck.id)).length,
      })),
    )
  }, [])

  async function addDeck(e: FormEvent) {
    e.preventDefault()
    const trimmed = name.trim()
    if (!trimmed) return
    await storage.createDeck(trimmed)
    setName('')
  }

  return (
    <div className="stack">
      <h1 className="page-title">Decks</h1>

      <form className="add-row" onSubmit={addDeck}>
        <input
          className="text-input"
          placeholder="New deck name…"
          value={name}
          onChange={(e) => setName(e.target.value)}
          aria-label="New deck name"
        />
        <button className="btn btn-primary" type="submit" disabled={!name.trim()}>
          Add deck
        </button>
      </form>

      {decks === undefined ? null : decks.length === 0 ? (
        <div className="empty-state">
          <div className="ico">🗂️</div>
          <h3>No decks yet</h3>
          <p>Name a deck above to start building your memory.</p>
        </div>
      ) : (
        <div className="stack">
          {decks.map(({ deck, due, count }) => (
            <Link key={deck.id} to={`/decks/${deck.id}`} className="deck-row">
              <div className="deck-text">
                <span className="deck-name">{deck.name}</span>
                <span className="deck-meta">
                  {count} card{count === 1 ? '' : 's'}
                </span>
              </div>
              {due > 0 ? (
                <span className="due-chip">{due} due</span>
              ) : (
                <span className="due-none">All caught up</span>
              )}
            </Link>
          ))}
        </div>
      )}
    </div>
  )
}
```

- [ ] **Step 3: Add deck-row + add-row styles to `styles.css`**

```css
.add-row {
  display: flex;
  gap: var(--space-3);
  align-items: center;
  justify-content: space-between;
}

.add-row .btn {
  white-space: nowrap;
  flex-shrink: 0;
  padding: 8px 18px;
}

.deck-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-3);
  padding: 15px 16px;
  background: var(--surface);
  border: 1px solid var(--border);
  border-radius: var(--radius-md);
  box-shadow: var(--shadow-sm);
  text-decoration: none;
  color: inherit;
  transition: transform 0.1s, border-color 0.1s;
}

.deck-row:hover {
  transform: translateY(-1px);
  border-color: var(--border-strong);
}

.deck-name {
  display: block;
  font-weight: 600;
  font-size: var(--text-base);
}

.deck-meta {
  display: block;
  font-size: var(--text-sm);
  color: var(--muted);
  margin-top: 2px;
}

.due-chip {
  flex-shrink: 0;
  font-size: var(--text-sm);
  font-weight: 700;
  color: #fff;
  background: var(--accent);
  padding: 3px 10px;
  border-radius: 999px;
}

.due-none {
  flex-shrink: 0;
  font-size: var(--text-sm);
  color: var(--muted);
}
```

- [ ] **Step 4: Run the sweep**

Run: `npx vitest run --project browser screens`
Expected: PASS — `deck list — with decks` and `deck list — empty` regenerate; chips/rows match the mockup.

- [ ] **Step 5: Commit**

```bash
git add src/features/decks/DeckListPage.tsx src/ui/styles.css src/test/screens.browser.test.tsx test-artifacts
git commit -m "$(printf 'feat: redesign deck list (rich rows, due chip, empty state)\n\nCo-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>')"
```

---

## Task 6: Deck detail redesign + card status tags

**Files:**
- Modify: `src/features/cards/DeckDetailPage.tsx`
- Modify: `src/ui/styles.css` (card-row + status-tag styles)
- Modify: `src/test/screens.browser.test.tsx` (`deck detail — nothing due` copy)

**Interfaces:**
- Consumes: `.empty-state` (Task 4), `.add-row` (Task 5), `MS_PER_DAY` from `../../domain/scheduler`.
- Produces: `cardStatus(scheduling, now)` returning `{ kind: 'new' | 'due' | 'scheduled'; label: string }`.

- [ ] **Step 1: Update the `nothing due` assertion (failing test)**

In `src/test/screens.browser.test.tsx`, change the `deck detail — nothing due` assertion to the new copy:

```tsx
  await expect.element(page.getByText('All caught up today', { exact: false })).toBeVisible()
```

Run: `npx vitest run --project browser "deck detail"`
Expected: FAIL — old UI renders "Nothing due", not "All caught up today".

- [ ] **Step 2: Rewrite `DeckDetailPage.tsx`**

```tsx
import { Link, useParams } from 'react-router-dom'
import { useLiveQuery } from 'dexie-react-hooks'
import { useStorage } from '../../data/StorageContext'
import type { SchedulingState } from '../../domain/models'
import { MS_PER_DAY } from '../../domain/scheduler'

/** First non-empty line of markdown, used as a one-line card preview. */
function firstLine(md: string): string {
  return md.split('\n').find((l) => l.trim())?.trim() ?? ''
}

/** A card's review status, derived from existing scheduling state. */
export function cardStatus(
  s: SchedulingState,
  now: number,
): { kind: 'new' | 'due' | 'scheduled'; label: string } {
  if (s.repetitions === 0) return { kind: 'new', label: 'new' }
  if (s.due <= now) return { kind: 'due', label: 'due' }
  const days = Math.max(1, Math.round((s.due - now) / MS_PER_DAY))
  const label = days >= 30 ? `${Math.round(days / 30)}mo` : `${days}d`
  return { kind: 'scheduled', label }
}

export function DeckDetailPage() {
  const { deckId } = useParams()
  const storage = useStorage()

  const deck = useLiveQuery(() => (deckId ? storage.getDeck(deckId) : undefined), [deckId])
  const cards = useLiveQuery(() => (deckId ? storage.listCards(deckId) : []), [deckId])
  const due = useLiveQuery(() => (deckId ? storage.countDue(deckId, Date.now()) : 0), [deckId])

  if (!deckId || deck === undefined || cards === undefined) return null

  const now = Date.now()

  return (
    <div className="stack">
      <div className="row between">
        <h1 className="page-title">{deck.name}</h1>
        {cards.length === 0 ? null : due && due > 0 ? (
          <Link to={`/decks/${deckId}/study`} className="btn btn-primary">
            Study {due}
          </Link>
        ) : (
          <span className="caught-up">All caught up today</span>
        )}
      </div>

      {cards.length === 0 ? (
        <div className="empty-state">
          <div className="ico">✏️</div>
          <h3>No cards yet</h3>
          <p>Add your first card — front, back, done.</p>
          <Link to={`/decks/${deckId}/cards/new`} className="btn btn-primary cta">
            + Add your first card
          </Link>
        </div>
      ) : (
        <>
          <div className="add-row">
            <span className="muted">
              {cards.length} card{cards.length === 1 ? '' : 's'}
            </span>
            <Link to={`/decks/${deckId}/cards/new`} className="btn btn-ghost">
              + Add card
            </Link>
          </div>
          <div className="stack">
            {cards.map((card) => {
              const status = cardStatus(card.scheduling, now)
              return (
                <Link
                  to={`/decks/${deckId}/cards/${card.id}`}
                  className="card-row"
                  key={card.id}
                >
                  <span className="card-front">
                    {firstLine(card.front) || <span className="muted">Untitled card</span>}
                  </span>
                  <span className={`status-tag status-${status.kind}`}>{status.label}</span>
                  <span className="card-edit">edit</span>
                </Link>
              )
            })}
          </div>
        </>
      )}
    </div>
  )
}
```

> `.add-row` (from Task 5) is `justify-content: space-between`, so the `N cards` text sits left and `+ Add card` sits right.

- [ ] **Step 3: Add card-row + status-tag + caught-up styles to `styles.css`**

```css
.caught-up {
  font-size: var(--text-sm);
  color: var(--muted);
}

.card-row {
  display: flex;
  align-items: center;
  gap: var(--space-3);
  padding: 13px 16px;
  background: var(--surface);
  border: 1px solid var(--border);
  border-radius: var(--radius-md);
  text-decoration: none;
  color: inherit;
}

.card-row:hover {
  border-color: var(--border-strong);
}

.card-front {
  flex: 1;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: var(--text-base);
}

.status-tag {
  flex-shrink: 0;
  font-size: 11.5px;
  font-weight: 600;
  padding: 2px 8px;
  border-radius: 999px;
}

.status-new {
  background: var(--accent-soft);
  color: var(--accent);
}

.status-due {
  background: color-mix(in srgb, var(--again) 16%, transparent);
  color: var(--again);
}

.status-scheduled {
  background: color-mix(in srgb, var(--good) 16%, transparent);
  color: var(--good);
}

.card-edit {
  flex-shrink: 0;
  font-size: var(--text-sm);
  color: var(--muted);
  opacity: 0;
  transition: opacity 0.1s;
}

.card-row:hover .card-edit {
  opacity: 1;
}
```

- [ ] **Step 4: Run the affected sweeps**

Run: `npx vitest run --project browser "deck detail"`
Expected: PASS — `with due cards` (shows `Study 2`), `nothing due` (shows `All caught up today`), `empty` (shows the CTA empty state).

- [ ] **Step 5: Add a unit test for `cardStatus`**

Append a unit test file `src/features/cards/cardStatus.test.ts`:

```ts
import { describe, it, expect } from 'vitest'
import { cardStatus } from './DeckDetailPage'
import { MS_PER_DAY } from '../../domain/scheduler'

const now = 1_000_000_000_000

describe('cardStatus', () => {
  it('marks unreviewed cards new', () => {
    expect(cardStatus({ repetitions: 0, intervalDays: 0, easeFactor: 2.5, due: now }, now).kind).toBe('new')
  })
  it('marks past-due reviewed cards due', () => {
    expect(cardStatus({ repetitions: 2, intervalDays: 1, easeFactor: 2.5, due: now - 1 }, now).kind).toBe('due')
  })
  it('labels future cards with a relative interval', () => {
    const s = cardStatus({ repetitions: 2, intervalDays: 3, easeFactor: 2.5, due: now + 3 * MS_PER_DAY }, now)
    expect(s.kind).toBe('scheduled')
    expect(s.label).toBe('3d')
  })
})
```

Run: `npx vitest run --project unit cardStatus`
Expected: PASS (3 tests).

- [ ] **Step 6: Commit**

```bash
git add src/features/cards/DeckDetailPage.tsx src/features/cards/cardStatus.test.ts src/ui/styles.css src/test/screens.browser.test.tsx test-artifacts
git commit -m "$(printf 'feat: redesign deck detail with study-control logic + card status tags\n\nCo-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>')"
```

---

## Task 7: Card editor redesign (labels, serif content, auto-grow)

**Files:**
- Modify: `src/features/cards/CardEditorPage.tsx`
- Modify: `src/ui/styles.css` (field labels, editor serif + auto-grow + inline-code spacing)

**Interfaces:**
- Consumes: `RichMarkdownEditor` (unchanged), `.btn`/`.btn-ghost`/`.btn-danger` (Task 4).

This task's behavior is unchanged (save/delete still work); verification is the existing `RichMarkdownEditor.test.tsx` staying green plus the editor screenshots matching the mockup.

- [ ] **Step 1: Update the field label markup in `CardEditorPage.tsx`**

Change the `CardField` helper so the label is an uppercase, spaced field label:

```tsx
function CardField({
  label,
  value,
  onChange,
}: {
  label: string
  value: string
  onChange: (value: string) => void
}) {
  return (
    <div className="field">
      <label className="field-label">{label}</label>
      <RichMarkdownEditor
        value={value}
        onChange={onChange}
        placeholder={`${label} (markdown)…`}
        ariaLabel={label}
      />
    </div>
  )
}
```

(Leave the rest of the file — `save`, `remove`, the action buttons — unchanged. The buttons already use `.btn-primary` / `.btn` / `.btn-ghost btn-danger`, which now read as neutral-disabled / visible-ghost from Task 4.)

- [ ] **Step 2: Update editor styles in `styles.css`**

Replace the `.field-label` rule and the `.rich-editor-content` sizing/serif rules:

```css
.field {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
}

.field-label {
  font-size: var(--text-sm);
  font-weight: 700;
  letter-spacing: 0.04em;
  text-transform: uppercase;
  color: var(--muted);
}

.rich-editor-content {
  min-height: 60px;
  padding: 12px 14px;
  outline: none;
  font-family: var(--font-serif);
  font-size: var(--text-lg);
  line-height: 1.55;
}

/* even spacing around inline code so following punctuation isn't pushed away */
.rich-editor-content :not(pre) > code,
.markdown :not(pre) > code {
  background: var(--accent-soft);
  color: var(--accent);
  font-family: var(--font-mono);
  padding: 1px 5px;
  margin: 0 1px;
  border-radius: var(--radius-sm);
}
```

Also update the editor code-block background to the token (replace the hard-coded `#1e1e2e` in `.rich-editor-content pre` and `.markdown pre`):

```css
.rich-editor-content pre,
.markdown pre {
  background: var(--code-bg);
  color: var(--code-fg);
  padding: 12px 14px;
  border-radius: var(--radius-md);
  overflow-x: auto;
  font-size: 13px;
}
```

- [ ] **Step 3: Verify editor tests + screenshots**

Run: `npx vitest run --project unit RichMarkdownEditor`
Expected: PASS (editor behavior unchanged).
Run: `npx vitest run --project browser "card editor"`
Expected: PASS — `new` and `edit with code block` regenerate; fields auto-grow, labels read as `FRONT`/`BACK`.

- [ ] **Step 4: Commit**

```bash
git add src/features/cards/CardEditorPage.tsx src/ui/styles.css test-artifacts
git commit -m "$(printf 'feat: redesign card editor (serif content, auto-grow, clear labels)\n\nCo-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>')"
```

---

## Task 8: Review screen — 3D flip, scroll, pinned grade bar, kbd hints

**Files:**
- Modify: `src/features/review/ReviewPage.tsx`
- Modify: `src/features/review/GradeButtons.tsx`
- Create: `src/features/review/reveal.browser.test.tsx`
- Modify: `src/ui/styles.css` (flip, faces, scroll, grade buttons, kbd)

**Interfaces:**
- Consumes: `MarkdownView`, `GradeButtons`, existing `revealed`/`grade` logic, `.kbd` (Task 4), `--flip-ms`.
- Produces: `.flip-inner[data-revealed='true']` as the revealed-state hook (set on the element tagged `data-testid="flip"`); grade markup with `.kbd` keys.

- [ ] **Step 1: Write the failing reveal-state test**

Create `src/features/review/reveal.browser.test.tsx` (a dedicated test so it survives the Task 10 sweep rewrite):

```tsx
import { test, expect } from 'vitest'
import { page } from 'vitest/browser'
import { ReviewPage } from './ReviewPage'
import { freshStorage } from '../../test/seed'
import { renderRoute } from '../../test/renderRoute'

test('revealing flips the card (data-revealed) and shows grade buttons', async () => {
  const storage = freshStorage()
  const deck = await storage.createDeck('TypeScript')
  await storage.createCard(deck.id, 'Q?', 'A — the answer.')
  await renderRoute({
    storage,
    entry: `/decks/${deck.id}/study`,
    path: '/decks/:deckId/study',
    element: <ReviewPage />,
  })
  await expect.element(page.getByTestId('flip')).toHaveAttribute('data-revealed', 'false')
  await page.getByRole('button', { name: 'Show answer', exact: false }).click()
  await expect.element(page.getByTestId('flip')).toHaveAttribute('data-revealed', 'true')
  await expect.element(page.getByRole('button', { name: 'Good', exact: false })).toBeVisible()
})
```

Run: `npx vitest run --project browser reveal`
Expected: FAIL — no element with `data-testid="flip"` exists yet.

- [ ] **Step 2: Rewrite `ReviewPage.tsx`**

Replace the `.review-card` block and the reveal/grade footer with the flip structure (the queue/keyboard/grade logic is unchanged from the current file; only the render and a measured-height effect are new):

```tsx
import { useCallback, useEffect, useLayoutEffect, useRef, useState } from 'react'
import { Link, useParams } from 'react-router-dom'
import type { Card, Grade } from '../../domain/models'
import { scheduler } from '../../domain/scheduler'
import { useStorage } from '../../data/StorageContext'
import { MarkdownView } from '../cards/MarkdownView'
import { GradeButtons } from './GradeButtons'

export function ReviewPage() {
  const { deckId } = useParams()
  const storage = useStorage()

  const [queue, setQueue] = useState<Card[] | null>(null)
  const [index, setIndex] = useState(0)
  const [revealed, setRevealed] = useState(false)

  const frontRef = useRef<HTMLDivElement>(null)
  const backRef = useRef<HTMLDivElement>(null)
  const [cardH, setCardH] = useState(220)

  useEffect(() => {
    if (!deckId) return
    let active = true
    storage.dueCards(deckId, Date.now()).then((cards) => {
      if (active) setQueue(cards)
    })
    return () => {
      active = false
    }
  }, [deckId, storage])

  const current = queue && index < queue.length ? queue[index] : null

  // Size the flip card to the taller face, capped to 65vh; the answer scrolls beyond that.
  useLayoutEffect(() => {
    if (!current) return
    const measure = () => {
      const f = frontRef.current?.scrollHeight ?? 0
      const b = backRef.current?.scrollHeight ?? 0
      const cap = Math.round(window.innerHeight * 0.65)
      setCardH(Math.max(160, Math.min(cap, Math.max(f, b))))
    }
    measure()
    window.addEventListener('resize', measure)
    return () => window.removeEventListener('resize', measure)
  }, [current])

  const grade = useCallback(
    async (g: Grade) => {
      if (!current) return
      const next = scheduler.next(current.scheduling, g, Date.now())
      await storage.updateCard(current.id, { scheduling: next })
      setIndex((i) => i + 1)
      setRevealed(false)
    },
    [current, storage],
  )

  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      if (!current) return
      if (!revealed) {
        if (e.code === 'Space' || e.key === 'Enter') {
          e.preventDefault()
          setRevealed(true)
        }
        return
      }
      const byKey: Record<string, Grade> = { '1': 'again', '2': 'hard', '3': 'good', '4': 'easy' }
      const g = byKey[e.key]
      if (g) {
        e.preventDefault()
        void grade(g)
      }
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [current, revealed, grade])

  if (!deckId || queue === null) return null

  if (queue.length === 0) {
    return (
      <div className="stack">
        <div className="empty-state">
          <div className="ico">🌙</div>
          <h3>Nothing due</h3>
          <p>This deck has no cards due right now.</p>
          <BackToDeck deckId={deckId} className="btn btn-ghost cta" />
        </div>
      </div>
    )
  }

  if (current === null) {
    return (
      <div className="stack">
        <div className="empty-state">
          <div className="ico">🎉</div>
          <h3>Review complete</h3>
          <p>
            {queue.length} card{queue.length === 1 ? '' : 's'} done. Nice work.
          </p>
          <BackToDeck deckId={deckId} className="btn btn-primary cta" />
        </div>
      </div>
    )
  }

  return (
    <div className="stack">
      <div className="row between">
        <span className="muted">
          {index + 1} / {queue.length}
        </span>
        <BackToDeck deckId={deckId} label="End session" className="btn btn-ghost" />
      </div>

      <div className="flip" style={{ height: `${cardH}px` }}>
        <div className="flip-inner" data-testid="flip" data-revealed={revealed}>
          <div className="face face-front">
            <div className="face-inner" ref={frontRef}>
              <div className="review-side">
                <MarkdownView source={current.front} />
              </div>
            </div>
            <button className="btn btn-primary show-btn" onClick={() => setRevealed(true)}>
              Show answer <span className="kbd">space</span>
            </button>
          </div>
          <div className="face face-back">
            <div className="scroll" ref={backRef}>
              <p className="answer-label">Answer</p>
              <div className="review-side">
                <MarkdownView source={current.back} />
              </div>
            </div>
          </div>
        </div>
      </div>

      {revealed && (
        <GradeButtons scheduling={current.scheduling} now={Date.now()} onGrade={grade} />
      )}
    </div>
  )
}

function BackToDeck({
  deckId,
  label = 'Back to deck',
  className = 'btn btn-ghost',
}: {
  deckId: string
  label?: string
  className?: string
}) {
  return (
    <Link to={`/decks/${deckId}`} className={className}>
      {label}
    </Link>
  )
}
```

- [ ] **Step 3: Update `GradeButtons.tsx` to use kbd chips**

Change the key `<span>` class from `grade-key` to `kbd`:

```tsx
          <span className="grade-label">{label}</span>
          <span className="grade-hint">{formatInterval(scheduler.next(scheduling, grade, now).intervalDays)}</span>
          <span className="kbd">{key}</span>
```

- [ ] **Step 4: Replace the Review-session styles in `styles.css`**

Replace the existing `.review-card`, `.review-side`, `.divider`, `.grade*` block with:

```css
/* Review — 3D flip card */
.flip {
  perspective: 1600px;
}

.flip-inner {
  position: relative;
  height: 100%;
  transform-style: preserve-3d;
  transition: transform var(--flip-ms) cubic-bezier(0.2, 0.7, 0.2, 1);
}

.flip-inner[data-revealed='true'] {
  transform: rotateX(180deg);
}

.face {
  position: absolute;
  inset: 0;
  backface-visibility: hidden;
  -webkit-backface-visibility: hidden;
  display: flex;
  flex-direction: column;
  background: var(--surface);
  border: 1px solid var(--border);
  border-radius: var(--radius-lg);
  box-shadow: var(--shadow-md);
  overflow: hidden;
}

.face-front {
  padding: 26px;
}

.face-front .face-inner {
  flex: 1;
}

.face-back {
  transform: rotateX(180deg);
}

.face-back .scroll {
  flex: 1;
  overflow-y: auto;
  padding: 24px 26px 30px;
}

.answer-label {
  font-family: var(--font-sans);
  font-size: 11px;
  letter-spacing: 0.08em;
  text-transform: uppercase;
  color: var(--muted);
  margin: 0 0 10px;
}

.review-side {
  font-family: var(--font-serif);
  font-size: var(--text-lg);
  line-height: 1.55;
  color: var(--text);
}

.show-btn {
  margin-top: auto;
  width: 100%;
  padding: 13px;
}

@media (prefers-reduced-motion: reduce) {
  .flip-inner {
    transition: none;
  }
}

/* Grade buttons */
.grade-row {
  display: grid;
  grid-template-columns: repeat(4, 1fr);
  gap: 10px;
}

.grade {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 2px;
  padding: 10px 6px;
  background: var(--surface);
  border: 1px solid var(--border-strong);
  border-radius: var(--radius-md);
  cursor: pointer;
  color: var(--text);
}

.grade:hover {
  background: var(--surface-inset);
}

.grade-label {
  font-weight: 600;
}

.grade-hint {
  font-size: var(--text-xs);
  color: var(--muted);
}

.grade-again { box-shadow: inset 3px 0 0 var(--again); }
.grade-hard  { box-shadow: inset 3px 0 0 var(--hard); }
.grade-good  { box-shadow: inset 3px 0 0 var(--good); }
.grade-easy  { box-shadow: inset 3px 0 0 var(--easy); }
```

Remove the now-unused `.grade-key` rule.

- [ ] **Step 5: Run the reveal test + review sweep**

Run: `npx vitest run --project browser reveal "review"`
Expected: PASS — the new `reveal` test asserts `data-revealed` flips `false`→`true`; the existing `review — question side` / `answer revealed` / `session complete` screenshots regenerate with the flip card, serif content, and kbd chips.

- [ ] **Step 6: Confirm the review-cycle integration test is unaffected**

Run: `npx vitest run --project unit reviewCycle`
Expected: PASS (grade/scheduling logic untouched).

- [ ] **Step 7: Commit**

```bash
git add src/features/review/ReviewPage.tsx src/features/review/GradeButtons.tsx src/features/review/reveal.browser.test.tsx src/ui/styles.css test-artifacts
git commit -m "$(printf 'feat: 3D flip study card with internal scroll and pinned grade bar\n\nCo-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>')"
```

---

## Task 9: Markdown content polish + remove dead styles

**Files:**
- Modify: `src/ui/styles.css` (markdown content rhythm; delete orphaned `.list-row`, `.badge*`, `.card-snippet`, `.review-card`, `.empty` if unused)

**Interfaces:**
- Consumes: tokens. No component change.

After Tasks 5–8, several old classes are orphaned (`.list-row`, `a.list-row`, `.badge`, `.badge-zero`, `.card-snippet`, `.empty`, `.page-title` still used). This task removes styles **our changes** orphaned and aligns markdown spacing.

- [ ] **Step 1: Grep for remaining usages before deleting**

Run:
```bash
grep -rnE "list-row|badge|card-snippet|review-card|className=\"empty\"|grade-key" src --include=*.tsx
```
Expected: no matches for `list-row`, `badge`, `card-snippet`, `review-card`, `grade-key`, or the bare `empty` class (replaced by `empty-state`). If a match remains, that surface still needs migrating — fix it before deleting its CSS.

- [ ] **Step 2: Delete the orphaned rules from `styles.css`**

Remove these blocks (orphaned by Tasks 5–8): `.list-row`, `a.list-row:hover`, `.badge`, `.badge-zero`, `.card-snippet`, `.card-snippet:hover`, and the old `.empty` rule. (`.review-card` and `.divider` were already removed in Task 8 when the review styles were replaced.) Keep `.page-title`, `.muted`, `.stack`, `.row`, `.between`, `.markdown*`, `.text-input`, editor and bubble-menu rules.

- [ ] **Step 3: Align markdown rhythm**

Ensure `.markdown` paragraphs use the content rhythm (append/adjust):

```css
.markdown p { margin: 0 0 var(--space-3); }
.markdown > :last-child { margin-bottom: 0; }
```

- [ ] **Step 4: Verify the full sweep still passes**

Run: `npx vitest run --project browser screens`
Expected: PASS (all 11 states; no visual regressions from the deletions).

- [ ] **Step 5: Commit**

```bash
git add src/ui/styles.css test-artifacts
git commit -m "$(printf 'refactor: drop styles orphaned by the redesign; align markdown rhythm\n\nCo-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>')"
```

---

## Task 10: Light + dark screenshot sweep and final gates

**Files:**
- Modify: `src/test/screens.browser.test.tsx` (loop scenarios over both themes)

**Interfaces:**
- Consumes: every redesigned surface. Produces dark-variant screenshots `*-dark.png` alongside the light ones.

- [ ] **Step 1: Refactor the sweep into a theme loop**

Rewrite `src/test/screens.browser.test.tsx` so each scenario is a function run once per theme. Replace the file with:

```tsx
import { test, expect, afterEach } from 'vitest'
import { page } from 'vitest/browser'
import { DeckListPage } from '../features/decks/DeckListPage'
import { DeckDetailPage } from '../features/cards/DeckDetailPage'
import { CardEditorPage } from '../features/cards/CardEditorPage'
import { ReviewPage } from '../features/review/ReviewPage'
import { freshStorage, MS_PER_DAY } from './seed'
import { renderRoute } from './renderRoute'
import { shoot } from './screenshot'

const CODE_BACK = 'Use a guard:\n\n```ts\nfunction f(x: unknown) {\n  if (typeof x === "string") return x\n}\n```'

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
      await expect.element(page.getByText('TypeScript')).toBeVisible()
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
      await storage.createCard(deck.id, 'What is the `never` type?', 'The empty type.')
      await storage.createCard(deck.id, 'What does `satisfies` do?', 'Checks without widening.')
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
      await expect.element(page.getByText('All caught up today', { exact: false })).toBeVisible()
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
    name: 'card-editor-new',
    run: async () => {
      const storage = freshStorage()
      const deck = await storage.createDeck('TypeScript')
      await renderRoute({ storage, entry: `/decks/${deck.id}/cards/new`, path: '/decks/:deckId/cards/new', element: <CardEditorPage /> })
      await expect.element(page.getByText('New card')).toBeVisible()
    },
  },
  {
    name: 'card-editor-edit',
    run: async () => {
      const storage = freshStorage()
      const deck = await storage.createDeck('TypeScript')
      const card = await storage.createCard(deck.id, 'How to narrow `unknown`?', CODE_BACK)
      await renderRoute({ storage, entry: `/decks/${deck.id}/cards/${card.id}`, path: '/decks/:deckId/cards/:cardId', element: <CardEditorPage /> })
      await expect.element(page.getByText('Edit card')).toBeVisible()
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
    name: 'review-nothing-due',
    run: async () => {
      const storage = freshStorage()
      const deck = await storage.createDeck('TypeScript')
      const card = await storage.createCard(deck.id, 'Already reviewed', 'Yes')
      await pushToFuture(storage, card.id)
      await renderRoute({ storage, entry: `/decks/${deck.id}/study`, path: '/decks/:deckId/study', element: <ReviewPage /> })
      await expect.element(page.getByText('Nothing due', { exact: false })).toBeVisible()
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
```

- [ ] **Step 2: Run the full themed sweep**

Run: `npx vitest run --project browser screens`
Expected: PASS — 22 tests; `test-artifacts/` now has both `*.png` and `*-dark.png` for all 11 states. Spot-check a few dark PNGs (e.g. `review-answer-dark.png`, `deck-list-dark.png`) render the warm-dark palette.

- [ ] **Step 3: Run all gates**

Run: `npm test`
Expected: PASS — both projects (unit + browser), all suites green.
Run: `npm run build`
Expected: PASS — `tsc --noEmit` clean, `vite build` succeeds.

- [ ] **Step 4: Update the roadmap**

In `docs/ROADMAP.md`, mark near-term #3 as shipped (change the `**next**` line to `✅ … **shipped**`) with a one-line summary (token system, light/dark, 3D flip, per-surface fixes). Bump `_Last updated:_` to 2026-06-21.

- [ ] **Step 5: Commit**

```bash
git add src/test/screens.browser.test.tsx docs/ROADMAP.md test-artifacts
git commit -m "$(printf 'test: capture all screens in light + dark; mark redesign shipped\n\nCo-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>')"
```

---

## Self-Review notes (for the implementer)

- **Spec coverage:** tokens (T1) · theming infra + toggle + no-FOUC (T2–T3) · shared button/kbd/empty patterns (T4) · deck list (T5) · deck detail + status tags (T6) · editor serif/auto-grow/labels (T7) · review flip + scroll + pinned grade bar + kbd + end states + ghost affordance (T8) · serif card content via `.review-side`/editor (T7–T8) · cleanup (T9) · light+dark sweep + gates (T10). Every issues-list item and spec section maps to a task.
- **Height nuance:** the flip needs a fixed container height (faces are `position: absolute`). T8 measures both faces and caps to 65vh; long answers scroll inside `.face-back .scroll`, and the grade bar is a sibling outside `.flip`, so it's always reachable.
- **`color-mix` fallback:** status-tag backgrounds use `color-mix`; it's supported in the Playwright Chromium used for tests and current browsers. If a flat tint is preferred, swap to explicit light/dark hex tokens.
- **Theme reset in tests:** every browser test that sets `data-theme` must clear it in `afterEach` (T1 and T10 do) so light scenarios aren't polluted by a prior dark test.
