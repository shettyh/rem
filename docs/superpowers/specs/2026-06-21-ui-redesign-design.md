# rem — UI redesign pass (design)

_Date: 2026-06-21 · Roadmap near-term #3_

## Goal

Replace rem's plain-CSS surface with a distinctive, opinionated design system and fix every
issue in `docs/superpowers/specs/2026-06-20-ui-issues.md`. The logic stays untouched; this is a
styling + small-markup pass behind the existing components, plus two new capabilities the redesign
introduces: a **light/dark theme** and a **3D card flip** on the study screen.

Validated visually with the brainstorming companion (mockups live in `.superpowers/brainstorm/`).

### In scope
- A real token system: spacing scale, type scale, semantic color roles, radii, shadow, motion.
- One chosen visual personality — **"Editorial / calm focus"** (below).
- Light **and** dark themes, with a header toggle and no flash-of-wrong-theme on load.
- A true 3D flip on the review screen, with long-answer scrolling and a pinned grade bar.
- Per-surface fixes: deck list, deck detail, card editor, review, and all empty/completion states.

### Out of scope (own later cycles)
- Export / import backup (near-term #4).
- FSRS, embeddings, generation (mid/long-term).
- No new domain features. Card-row status and deck card-counts only **surface data we already store**;
  no `Storage` interface change is required.

## Chosen personality — "Editorial / calm focus"

| Role | Light | Dark (warm) |
|---|---|---|
| `--bg` | `#f7f5f1` (warm paper) | `#1a1815` |
| `--surface` | `#fffefb` | `#232019` |
| `--surface-inset` | `#efe9dd` | `#2c2820` |
| `--border` | `#e7e3da` | `#34302a` |
| `--border-strong` | `#ddd6c9` | `#423d35` |
| `--text` | `#1c1a17` | `#f2ede3` |
| `--muted` | `#8a8578` | `#a39d8f` |
| `--accent` | `#6534c9` (deep violet) | `#b9a0f5` |
| `--accent-soft` | `#efe9dd` | `#2c2820` |
| `--code-bg` / `--code-fg` | `#211f2e` / `#eceaf6` | `#2a2733` / `#eceaf6` |
| grade `--again/--hard/--good/--easy` | `#d6553f` / `#cf8a3a` / `#4c9a6a` / `=accent` | `#e87a63` / `#e0a85a` / `#6fc28e` / `=accent` |

- **Type:** UI chrome in the system sans stack. **Card content** (review faces + editor content) in a
  **serif** stack (`Georgia, 'Times New Roman', serif`) so studying reads like prose. Code/mono stays
  monospace in both. Code blocks are dark in both themes (keep the existing `github-dark` highlight theme).
- **Personality cues:** warm paper background, one saturated accent, a serif content voice, quiet
  color "spines" on grade buttons (`inset 3px 0 0 <grade>`), gentle hover lift on rows.

The grade per-button colors derive from the semantic `--again/--hard/--good/--easy` tokens (no more
ad-hoc hexes), and those map onto general roles (`--danger`, `--warn`, `--good`, `--accent`).

## Token foundation

New file `src/ui/tokens.css`, imported before `src/ui/styles.css`. Defines, as CSS custom properties:

- **Color roles** on `:root` (light) with a `[data-theme='dark']` override block, per the table above.
- **Spacing scale:** `--space-1..-8` on a consistent rhythm (4 / 8 / 12 / 16 / 24 / 32 / 48 / 64).
- **Type scale:** `--text-xs..-2xl` (12 / 13 / 15 / 18 / 21 / 25) plus `--font-sans` and `--font-serif`.
- **Radii:** `--radius-sm/md/lg` (6 / 11 / 16). **Shadow:** `--shadow-sm/-md`. **Motion:** `--flip-ms: 600ms`.

`styles.css` is reworked to consume these tokens (no hard-coded colors/spacing) and reorganized into
banner-commented sections: Base · Buttons · Forms · Rows · Empty states · Markdown/content · Editor ·
Review/flip · Grade buttons. Splitting tokens out keeps the design system independently legible from
the component styles.

## Theming infrastructure

- `src/ui/theme.ts` — tiny module: `type Theme = 'light' | 'dark'`; `getStoredTheme()` reads
  `localStorage['rem-theme']`; `resolveInitialTheme()` returns the stored value or the
  `prefers-color-scheme` match; `applyTheme(t)` sets `document.documentElement.dataset.theme` and persists.
- `index.html` — a small **inline** bootstrap script in `<head>` runs `applyTheme(resolveInitialTheme())`
  logic before first paint, so there is no flash of the wrong theme. (Inline-duplicated, framework-free,
  a few lines — the one acceptable spot for that.)
- `src/ui/ThemeToggle.tsx` — a header button (`☀︎`/`☾`) that flips between light and dark and calls
  `applyTheme`. Two-state; the initial default follows the system preference until the user picks one.
- `Layout.tsx` — header becomes `wordmark … ThemeToggle`, tightened (≈48px, no oversized divider/gap)
  to fix the top-heavy header.

## Per-surface changes

Each item maps to a fix in the issues list. Markup changes are minimal; most work is CSS.

### Deck list (`DeckListPage.tsx`)
- Deck rows show **name + "N cards"** (count from `listCards(deck.id).length`, added to the existing
  live query) and a **violet "N due" chip only when `due > 0`**; otherwise quiet "All caught up" text.
- "Add deck" button: `white-space: nowrap; flex-shrink: 0`, full padding — never wraps. **Disabled =
  neutral token** (`--surface-inset` fill, muted text), not faded accent.
- Empty state uses the shared empty-state pattern (icon · serif heading · one-line nudge).

### Deck detail (`DeckDetailPage.tsx`)
- Study control logic, replacing today's always-rendered pill:
  - `cards.length === 0` → **no study control**; show the empty state with a single "Add your first card" CTA.
  - `cards.length > 0 && due === 0` → plain muted text **"All caught up today"** (not a button shape).
  - `due > 0` → primary **"Study N"** button.
- Card rows are visually distinct from deck rows (denser) and carry a **status tag** derived from
  `card.scheduling`: `repetitions === 0` → `new`; else `due <= now` → `due`; else a relative label
  (e.g. `2d`) via a small `formatDue(due, now)` helper. Plus an `edit` affordance on the row.

### Card editor (`CardEditorPage.tsx`, `RichMarkdownEditor.tsx`)
- Clear `FRONT` / `BACK` labels with more separation from the field.
- Editor fields **auto-grow** from a smaller `min-height` (~60px) instead of fixed 120px.
- Editor content renders in the **serif** content voice; inline-code chip horizontal margins tuned so
  punctuation after a chip no longer gets an awkward gap.
- Primary "Save card"; **neutral disabled** treatment; ghost "Cancel"; danger "Delete" when editing.

### Review (`ReviewPage.tsx`, `GradeButtons.tsx`, `MarkdownView` styling)
- **3D flip.** `.review-card` becomes a flip structure:
  ```
  .flip (perspective)
    .flip-inner (transform-style: preserve-3d; rotateX(180deg) when revealed)
      .face.face-front  → question (serif) + "Show answer" CTA pinned to the bottom
      .face.face-back    → .scroll wrapper: answer label + answer (serif), scrolls internally
  GradeButtons           → sibling BELOW the flip, fades/rises in when revealed
  ```
  - **Height:** measure both faces' content height on card change (`useLayoutEffect`) and set the flip
    container height to `clamp(160px, max(frontH, backH), 65vh)`. When the back exceeds the cap it
    **scrolls inside the card** (slim scrollbar + bottom fade); the grade bar stays reachable because it
    lives outside the card. The front pins its CTA to the bottom so short questions look composed, not
    top-left-hugging. Recompute on resize.
  - **Reduced motion:** `@media (prefers-reduced-motion: reduce)` removes the `.flip-inner` transition —
    the reveal becomes an instant swap. Reveal still works via the same state.
  - Reveal state is the existing `revealed` boolean; Space/Enter still reveals, 1–4 still grade.
- **Keyboard hints** become legible **kbd-style chips** (`space` on the CTA; `1`–`4` on grade buttons),
  not `opacity: 0.45` noise.
- **Ghost buttons** ("Back to deck", "End session") get a visible affordance: bordered, `--surface`
  background — they read as actions, not labels.
- **Completion / nothing-due** states use the shared empty-state pattern: icon, a real summary line
  (e.g. "Review complete — N cards done"), and one clear primary button.

### Shared components / patterns (fix once)
- **Empty state** — one CSS pattern (icon · serif heading · muted line · optional CTA) reused by deck
  list, deck detail, review-complete, and nothing-due.
- **Buttons** — primary / ghost (visible) / danger / **neutral disabled**, all token-driven.
- **kbd chip** — one class used by the CTA and grade keys.

## Testing & verification

- **Browser screen sweep** (`src/test/screens.browser.test.tsx`): parametrize over **light + dark** so the
  11 states are captured in both themes; regenerate `test-artifacts/`. This is the primary visual check.
- **New behavior tests:**
  - Theme toggle: clicking it flips `documentElement.dataset.theme` and persists `localStorage['rem-theme']`.
  - Review reveal/flip: after "Show answer", the answer content and grade buttons are present (the flip is
    CSS; assert the revealed state/markup, which also exercises the reduced-motion path).
- **Update assertions coupled to changed markup/text** — e.g. the old "Nothing due" pill, the
  disabled-button styling, the `grade-key` hint markup, deck/card row contents.
- **Green gates:** `npm test` (unit + browser), `tsc` typecheck, and `vite build` all pass; screenshots
  regenerated and re-inspected.

## File-change summary

**New**
- `src/ui/tokens.css` — design tokens (light + dark, spacing, type, radii, shadow, motion).
- `src/ui/theme.ts` — theme resolve/apply/persist helpers.
- `src/ui/ThemeToggle.tsx` — header theme button.

**Edited**
- `index.html` — inline no-FOUC theme bootstrap.
- `src/app/main.tsx` — import `tokens.css` before `styles.css` (or `@import` at the top of `styles.css`).
- `src/ui/styles.css` — token-driven rewrite + new component/surface/flip/kbd/empty-state styles.
- `src/ui/Layout.tsx` — tightened header with theme toggle.
- `src/features/decks/DeckListPage.tsx` — richer rows, due chip, empty state, neutral disabled.
- `src/features/cards/DeckDetailPage.tsx` — study-control logic, card-row status tags, empty-state CTA.
- `src/features/cards/CardEditorPage.tsx` — labels, actions, neutral disabled.
- `src/features/cards/RichMarkdownEditor.tsx` — serif content, auto-grow, inline-code spacing.
- `src/features/review/ReviewPage.tsx` — flip structure + height measurement, scroll, pinned grade bar,
  kbd hint, stronger completion/nothing-due states, ghost affordance.
- `src/features/review/GradeButtons.tsx` — kbd chips, token-derived grade colors.
- `src/test/screens.browser.test.tsx` (+ touched assertions in `smoke.browser.test.tsx`,
  `reviewCycle.test.ts`) — dark variants and updated expectations.

## Build order (for the plan)

1. **Token foundation + theming infra** — `tokens.css`, `theme.ts`, `index.html` bootstrap, `ThemeToggle`,
   header. Verify: toggle flips theme, no FOUC, existing screens still render.
2. **Shared patterns** — buttons (incl. neutral disabled + visible ghost), kbd chip, empty-state. Apply
   tokens across `styles.css`.
3. **Static surfaces** — deck list, deck detail, card editor. Verify each against the mockups in both themes.
4. **Review flip** — flip structure, measured height, internal scroll, pinned grade bar, kbd hints,
   reduced-motion, stronger end states.
5. **Tests & screenshots** — parametrize the sweep over themes, add toggle/flip tests, fix coupled
   assertions, regenerate artifacts; all gates green.

Each step is independently verifiable, keeping the redesign reviewable in slices.
