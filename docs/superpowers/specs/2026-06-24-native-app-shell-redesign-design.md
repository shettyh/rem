# rem — Native app-shell redesign (design)

_Date: 2026-06-24_

## Goal

Make `rem` look and feel like a **native macOS desktop app**, not a webapp resized inside a
window. Today every screen is a centered 820px document floating in wide empty gutters, with a
web-style header, raw HTML form controls, and an inconsistent serif/sans + washed-out-disabled
type/color system. This pass replaces that with a real **sidebar + content app shell**, a
neutral macOS palette with a single system accent, native window chrome, and one coherent
control/type system.

Logic and data stay untouched: no changes to `Scheduler`, `Storage`, the Dexie schema, sync, or
the domain models. This is a UI/shell + Tauri-window pass.

**This supersedes** the prior "Editorial / calm focus" direction
(`2026-06-21-ui-redesign-design.md`): warm-paper background, deep-violet accent, and the 3D flip
card are all retired here.

### In scope
- A new app shell (`Layout`): translucent **sidebar** (nav + deck list + footer) and a
  window-filling **content area** with an integrated titlebar/toolbar.
- macOS window chrome via Tauri: overlay titlebar with traffic-light inset, sidebar **vibrancy**,
  sensible minimum window size — all gated so non-macOS gets a solid sidebar and a normal titlebar.
- A reworked token system: neutral light/dark palette, **macOS Indigo** accent, materials, radii,
  motion. Fixes the broken disabled-button style.
- One type rule: system sans for all chrome; **serif only for card faces** (review + editor content).
- Per-screen rebuild against the shell: **Today** (new home), **Deck detail**, **Card editor**
  (as a sheet), **Review** (simplified reveal, no 3D flip), **Settings** (styled native controls).
- Keep the existing light/dark theme toggle working; no flash-of-wrong-theme on load.

### Out of scope
- Any domain/logic/storage/sync change. Card status and counts keep surfacing only data we store.
- New features (stats dashboard, nested decks, images, generation) — separate cycles.
- A separate native Settings *window* — Settings stays an in-app view for now.

## Approach (chosen)

**A — Sidebar + content "source list" app** (Things / Bear / Apple Notes shape). Considered and
rejected: **B** toolbar-driven single column (navigation stays shallow, still feels page-like) and
**C** three-pane decks|cards|detail (over-built for short flashcard content).

## App shell

Replaces the centered-column `Layout.tsx`. Two regions inside a full-window flex row:

### Sidebar (~240px, fixed, vibrancy on macOS)
- **Top inset** (~28px) so nav never collides with the overlaid traffic lights; this strip is a
  drag region (`data-tauri-drag-region`).
- **Nav section:** `Today`. Active item gets the macOS selected-row treatment (accent-tinted
  fill, rounded). (No new screens are introduced; nav is Today + the deck list + Settings.)
- **Decks section:** the deck list (moved out of the old home page) — each row shows deck name and
  a due-count badge; clicking routes to the deck. This is the primary navigation surface.
- **Footer:** `＋ New deck` (opens inline create — name field + scheduler picker, styled) and a
  `Settings` gear routing to `/settings`.

### Content area (fills remaining width)
- **Toolbar / integrated titlebar:** a top bar that is a drag region; left = screen title,
  right = context actions (per screen). On macOS the traffic lights sit at its left over the
  sidebar; the toolbar content is inset to clear them.
- **Body:** the routed screen, with comfortable internal max-width for readability but the panel
  itself fills the window (no empty gutters).

### Cross-platform fallback
On Windows/Linux: solid sidebar background (no vibrancy), standard titlebar (no overlay), no
traffic-light inset. The shell layout is identical; only the chrome layer differs. Detection via a
small `isMac` check (Tauri OS plugin or `navigator`), applied as a `data-platform` attribute on the
root so CSS keys off it.

## Per-screen changes

### Today (new home — replaces the deck-list home content)
The deck list moves to the sidebar, so the home route renders a **dashboard**:
- Greeting + a refined summary (due-today / total-decks) using a cleaner stat treatment than the
  current tiles.
- A prominent **Start review** action when cards are due across decks; a calm "You're all caught
  up" state otherwise.
- Toolbar: title `Today`.

### Deck detail (`/decks/:deckId`)
- Card list as a tidy rows list: front preview (truncated) + status chip (`new` / `due` /
  `scheduled`) + an edit affordance on hover.
- Toolbar actions: **Study** (→ review) and **＋ Add card** (opens the editor sheet). These move
  out of the page body.
- Empty state preserved, restyled.

### Card editor (`/decks/:deckId/cards/new`, `/decks/:deckId/cards/:cardId`)
- Presented as a **sheet** over the deck detail (native add/edit pattern) rather than a full page
  swap. Implemented via the react-router **modal-route** pattern (background location), preserving
  the existing paths for deep-linking and existing tests.
- Keeps the TipTap rich-markdown editor unchanged; restyles the FRONT/BACK field labels and the
  Add/Cancel actions into the unified control system.

### Review (`/decks/:deckId/study`)
- Focused mode in the content area.
- **Remove the 3D flip.** Replace with a quick, clean **reveal** (short cross-fade / state swap),
  honoring `prefers-reduced-motion`. Long answers scroll; the grade bar stays pinned.
- Grade buttons: one coherent row (the four grades with their semantic spine colors + keyboard
  hint chips), styled consistently with the rest of the app.
- Progress (`n / m`) and **End session** live in the toolbar.

### Settings (`/settings`)
- Grouped "preference pane" sections (Sync, Export, Import) with consistent headers/hints.
- Replace raw controls with styled native-feeling equivalents: the scheduler/`<select>`, the
  export checkboxes, and the `Choose File` import button.

## Visual system

### Palette — neutral + system accent
Replace the warm-paper/violet tokens in `src/ui/tokens.css`. Neutral grays for light and dark; a
single accent = **macOS Indigo** (`#5856D6` light / `#5E5CE6` dark). Roles to define (light + dark):
`--bg`, `--surface`, `--surface-inset`, `--sidebar` (translucent base), `--border`,
`--border-strong`, `--text`, `--muted`, `--accent`, `--accent-hover`, `--accent-soft`, `--code-bg`,
`--code-fg`, and the grade roles `--again/--hard/--good/--easy` retuned to read against neutral
surfaces. Exact hex values chosen during implementation to hit WCAG AA contrast in both themes.

### Type — one rule
- **System sans** (`system-ui` → SF Pro on macOS) for *all* UI chrome, labels, lists, buttons.
- **Serif** (`Georgia` stack) **only** for **card faces** — the review front/back and the editor
  content — so studying reads like prose. This is the one deliberate exception; nothing else uses
  serif. Code/mono stays monospace.

### Controls — coherent hierarchy
- Button tiers: primary (accent fill), secondary/ghost (bordered), danger (text). **Fix the
  disabled state**: a proper muted, legible style — not the current washed beige that's unreadable.
- Inputs, selects, checkboxes, and the file picker share one styled treatment with a consistent
  focus ring (accent outline).

### Motion
Quiet and native: subtle hover/selection transitions, the simplified reveal. No 3D, no large
gimmicks. All motion respects `prefers-reduced-motion`.

## Native plumbing (Tauri / `src-tauri`)

macOS-targeted window config, applied so other platforms degrade gracefully. Exact Tauri v2 keys
verified against current docs during implementation (via context7); intended changes:
- `tauri.conf.json` window: overlay/transparent titlebar (`titleBarStyle: "Overlay"`,
  `hiddenTitle: true`), a **minimum window size** (e.g. 720×520) and a more app-appropriate default
  size, and the window effect for **sidebar vibrancy** (`windowEffects` sidebar material, or the
  `window-vibrancy` crate if the config route is insufficient; `macOSPrivateApi` enabled if
  transparency requires it).
- Frontend: the sidebar/toolbar top regions marked `data-tauri-drag-region`; content insets so the
  traffic lights never overlap interactive elements.

No new Tauri commands or capabilities needed (vibrancy/titlebar are window config). If the
`window-vibrancy` crate is used, it's added to `src-tauri/Cargo.toml` and called in `lib.rs` setup.

## Components & files

- `src/ui/Layout.tsx` → rebuilt shell; new `src/ui/Sidebar.tsx`, `src/ui/Toolbar.tsx`
  (and a small `useToolbar`/context so screens can set their title + actions).
- `src/ui/tokens.css`, `src/ui/styles.css` → reworked palette, materials, controls, type rule.
- `src/features/decks/DeckListPage.tsx` → split: deck list logic feeds the sidebar; a new
  **Today** dashboard becomes the home content.
- `src/features/cards/DeckDetailPage.tsx`, `CardEditorPage.tsx` → restyle + editor-as-sheet.
- `src/features/review/ReviewPage.tsx`, `GradeButtons.tsx` → simplified reveal + restyle.
- `src/features/settings/SettingsPage.tsx`, `SyncSection.tsx` → styled controls.
- `src/app/routes.tsx` → modal-route wiring for the card-editor sheet.
- `src-tauri/tauri.conf.json` (+ maybe `Cargo.toml`, `src/lib.rs`) → window chrome.

## Testing & verification

- Keep `npm test` (Vitest + browser tests) green. The existing `__screenshots__` baselines under
  `src/**` will need **re-generating** — expected, since this is a deliberate visual change, not a
  regression. Behavior assertions (reveal flips state, grade scheduling, create deck/card, export/
  import, sync) must keep passing, updated only where markup/structure moved.
- Refresh the `test-artifacts/*.png` reference screenshots to the new design.
- A coherence + a11y pass: consistent focus rings, contrast AA in both themes, no traffic-light
  overlap, keyboard review flow intact.

## Risks / notes

- **Large diff.** Structural change touches every screen and the global CSS; reviewed per-screen.
- **Editor-as-sheet routing** is the one area with react-router nuance (background location); if it
  proves fragile, fall back to the editor rendering in the content area styled as a focused pane.
- **Vibrancy/transparency** can interact with the theme background; the content panel stays opaque
  so only the sidebar shows the material. Verify in a real `tauri dev` build, not just the browser.
