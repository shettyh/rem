# rem — mockup reskin (`rem.dc.html`)

**Date:** 2026-06-24
**Status:** Approved design, pending implementation plan
**Source design:** Claude Design project `87928d48-56a1-4f63-a282-8e6f287e918a`, file `rem.dc.html`
("Modern Anki-like UI design")

## 1. Goal

Re-skin the existing rem desktop app to the visual identity in `rem.dc.html`: a distinct
editorial look (Instrument Serif display type, Space Grotesk body, Space Mono labels, a purple
accent, warm-dark default theme + warm-light theme). The app is already structurally close to the
mockup, so this is a **reskin plus three targeted feature changes**, not a rebuild.

The prototype's fake macOS traffic-light dots and in-memory data are scaffolding and are **not**
ported — the app keeps its real Tauri window chrome, Dexie storage, and FSRS/SM-2 scheduler.

### Decisions (locked with the user)

1. **Full reskin** — adopt the mockup's complete visual identity across all screens.
2. **Card editor: modal shape, keep the rich editor** — adopt the mockup's modal (Front/Back,
   Save/Cancel/Delete) but keep the TipTap rich-markdown editor inside each field (text/code/image
   is a core requirement from `idea.md`), not plain textareas.
3. **Today "Start review" = cross-deck** — add a real all-decks review session. Per-deck review
   stays.
4. **Settings is reskinned too**, for coherence — same editorial treatment, behavior unchanged.

### Non-goals

- No change to scheduling, storage, sync, backup, or import/export **logic**.
- No fake traffic lights; no Google Fonts CDN dependency.
- No unrelated refactoring of data/domain layers.

## 2. Approach

Rewrite the two stylesheets in place and make surgical JSX edits to add the markup the mockup has
that the app currently lacks. This preserves tested component structure, class hooks, and data
wiring. (Rejected alternative: rebuilding each screen with inline styles like the prototype — it
discards working, tested code for no benefit.)

## 3. Design tokens & fonts (`src/ui/tokens.css`)

Rewrite token **values** to the mockup's two palettes; keep token **names** so components don't
churn. Add one new role, `--faint` (used widely for Space Mono micro-labels).

Token mapping (mockup → existing name):

| Existing name      | Dark value | Light value | Mockup source |
|--------------------|------------|-------------|---------------|
| `--bg`             | `#0F0E13`  | `#FAF9F6`   | `--bg`        |
| `--sidebar`        | `#15141B`  | `#F1EFEA`   | `--sidebar`   |
| `--surface`        | `#1B1A23`  | `#FFFFFF`   | `--surface`   |
| `--surface-inset`  | `#232231`  | `#F4F2ED`   | `--surface2`  |
| `--bg-inset`       | `#232231`  | `#F4F2ED`   | `--surface2` (hover wash) |
| `--border`         | `#2A2836`  | `#E7E4DD`   | `--border`    |
| `--border-strong`  | `#3A3848`  | `#D9D5CD`   | `--border2`   |
| `--text`           | `#F4F3F8`  | `#1A1822`   | `--text`      |
| `--muted`          | `#9D99A8`  | `#76727E`   | `--muted`     |
| `--faint` *(new)*  | `#615D6E`  | `#A6A1AC`   | `--faint`     |
| `--accent`         | `#7E6CFF`  | `#5A47E8`   | `--accent`    |
| `--accent-soft`    | `#241F3D`  | `#EEEBFF`   | `--soft`      |
| `--accent-text`    | `#7E6CFF`  | `#5A47E8`   | accent-as-text|
| `--selection`      | `#241F3D`  | `#EEEBFF`   | `--soft`      |
| `--on-accent`      | `#ffffff`  | `#ffffff`   | `--accentfg`  |
| `--again`          | `#E5484D`  | `#DC2A2F`   | `--again`     |
| `--hard`           | `#E8922E`  | `#D97914`   | `--hard`      |
| `--good`           | `#2FA86B`  | `#1E9B5E`   | `--good`      |
| `--danger`         | = `--again`| = `--again` | —             |
| `--shadow-sm`      | `0 1px 2px rgba(0,0,0,0.45)` | `0 1px 3px rgba(20,18,30,0.08)` | `--shadow` |

`--accent-hover` is kept (derive as a slightly brighter accent). `--easy` stays `var(--accent)`.
Code block colors (`--code-bg`/`--code-fg`) keep a dark code surface in both themes.

Default theme flips to **dark** (mockup default). `color-scheme` follows the active theme. The
boot script in `index.html` already resolves stored/`prefers-color-scheme` theme — unchanged.

Font tokens remapped:

- `--font-sans` → `'Space Grotesk', system-ui, -apple-system, sans-serif`
- `--font-serif` → `'Instrument Serif', Georgia, serif`
- `--font-mono` → `'Space Mono', ui-monospace, SFMono-Regular, Menlo, monospace`

`body` font-family (currently hardcoded in `styles.css`) switches to `var(--font-sans)`.

### Fonts are self-hosted

This is an offline-first desktop app, so the three families are **vendored as `.woff2`** under
`src/ui/fonts/` and declared with `@font-face` (in `tokens.css` or a new `fonts.css` imported by
`styles.css`). No runtime Google Fonts request. Weights needed:

- Instrument Serif: 400 normal + 400 italic
- Space Grotesk: 400, 500, 600, 700
- Space Mono: 400, 700

Vite fingerprints and bundles them. `font-display: swap`.

## 4. Per-screen changes

All visual values below come from `rem.dc.html`. Behavior/data wiring is unchanged unless noted.

### 4.1 Sidebar (`src/ui/Sidebar.tsx`, `styles.css`)
Structure already matches (Today nav, Decks section + "+", deck list with due badges, Settings +
theme toggle footer). Restyle to mockup: `rem` wordmark (26px, -0.04em) + accent dot + right-aligned
`recall` Space-Mono micro-tag; nav items as 10px-radius rows with `--accent-soft` active state;
due badges as Space-Mono pill chips on `--accent`; Settings as a labeled row + a bordered theme
toggle showing `Dark`/`Light` with a ring dot. Width 264px (mockup) — update `--sidebar-w` (keep the
narrow-window media query, scaled). **No fake traffic lights**; keep `titlebar-spacer` drag region.
The Decks "+" keeps **add-deck** semantics (matching its tooltip): it navigates to Today and focuses
the add-deck input. (The prototype wired it to open a new card; that's a prototype inconsistency we
don't carry over — card creation lives in the deck view, §6.)

### 4.2 Today (`src/features/decks/DeckListPage.tsx`, `styles.css`)
Add the mockup's three blocks inside the content body (max-width 880px, `remRise` entry animation):
1. **Greeting** — Instrument Serif ~68px, time-of-day phrase (mockup adds "Still up." for <5h;
   adopt). Subtitle: "You have N cards due across M decks."
2. **Review band** — rounded surface split into: left = giant Instrument-Serif `totalDue` (accent) +
   "cards due right now" + NEW/REVIEW Space-Mono chips; right = gradient panel (accent →
   lighter accent via `color-mix`) "Start review" + "press ⏎" hint. Click / Enter → cross-deck
   review (§5). When nothing is due, show the caught-up state instead of the band.
3. **Your decks** — section label + count, then a responsive grid
   (`repeat(auto-fill,minmax(248px,1fr))`) of deck cards: top color bar, algo chip + total, deck
   name (Instrument Serif), due/new labels. Each card → deck view.
4. **Add-deck row** — restyled input + button (keep the FSRS/SM-2 scheduler `<select>` already
   present; place it consistently). Keeps `addDeck` behavior. This is the single "add deck"
   affordance, and the Sidebar "+" focuses it (§4.1).

Today needs aggregate counts (total due / new / review) and the deck grid's per-deck due/new — see
§5 for the cross-deck data helper.

### 4.3 Deck view (`src/features/cards/DeckDetailPage.tsx`, `styles.css`)
Header: deck color dot + name + algo chip; right actions `+ Add card` (opens editor modal, §6) and
`Study N` (primary). Body (max-width 880px): a row of **three stat cards** (Due / New / Total,
Instrument-Serif numbers, Space-Mono uppercase labels), then the card list as richer rows: front
preview (Instrument Serif) + back preview (muted) + state tag pill (`new`/`due`/`review`/scheduled
interval) using `cardStatus`. Row click opens the editor modal for that card. Empty state restyled.

### 4.4 Review (`src/features/review/ReviewPage.tsx`, `GradeButtons.tsx`, `styles.css`)
Header: `index+1 / total` (Space Mono) + deck name (or "All decks") + "End session". Front stage:
Instrument-Serif question card (centered, `remPop`) + full-width accent "Show answer / space"
button. Back stage (`remFade`): question (muted) → rule → "Answer" mono label → answer (Instrument
Serif) → **4 grade buttons** in a 4-col grid, each with a colored top border
(again/hard/good/accent), label, real interval from `GradeButtons`, and a number chip 1–4. Behavior,
keyboard handling, and scheduler calls unchanged. Empty/complete states restyled.

`GradeButtons` markup updates to the new column layout (label, interval, key chip) and colored
top-border classes; the interval computation is untouched.

### 4.5 Settings (`src/features/settings/SettingsPage.tsx`, `SyncSection.tsx`, `styles.css`)
Restyle to the mockup, **behavior unchanged**. Body max-width 680px. Three sections separated by
hairline rules, each with an Instrument-Serif (28px, weight 400) heading and Space-Mono uppercase
field labels:
- **Sync** — heading + hint, "Git remote URL" label, mono input, full-width accent "Sync now",
  mono "Last synced…" / status line. Keeps `SyncSection` logic (remote persistence, status states,
  git-not-installed message).
- **Export decks** — custom square checkbox rows (Select all + each deck), full-width "Export
  selected". Keeps selection state + `onExport`.
- **Import decks** — heading + "Same-named decks are replaced on import." hint, a "Choose file"
  styled file input + "no file selected" affordance, and the same-name **replace warning** dialog.
  Keeps `onFile`/`planImport`/`runImport` logic.

## 5. Cross-deck review + Today aggregates

The Storage seam stays untouched; cross-deck behavior is **composed in the UI/data layer**.

- New helper (e.g. `src/features/review/dueOverview.ts` and/or a `useDueOverview` hook) that:
  - maps `listDecks()` → per-deck `dueCards(deckId, now)` and concatenates, for the all-decks queue
    (shuffled), and
  - derives Today's aggregate counts: `totalDue` (sum of `countDue`), and a new/review split via
    `cardStatus`/`isNew` over the due cards (new = first-time cards, review = the rest), plus
    per-deck due/new for the grid.
- **Routing:** add `/study` (all decks). Generalize `ReviewPage` so the no-`deckId` case loads the
  cross-deck queue; "End session"/empty/complete return to Today (`/`). Per-deck
  `/decks/:deckId/study` is unchanged.
- Today's review band + deck grid consume these aggregates; the band's CTA and Enter key navigate to
  `/study`.

Shuffle uses a simple in-place Fisher–Yates over the concatenated queue.

## 6. Card editor → in-page modal

The editor currently renders inside `Sheet` but as a **route**, so it floats over a blank content
area. Convert to **in-page modal state** so it overlays the deck/today content like the mockup.

- Refactor `CardEditorPage` into a `CardEditorModal` component (`src/features/cards/`) taking
  `deckId`, optional `cardId` (or a `card`), and `onClose`. Same TipTap `RichMarkdownEditor`
  Front/Back fields; restyle the modal to the mockup (Instrument-Serif title, mono field labels,
  Save / Cancel / Delete footer, `remOverlay`/`remPop` animations, backdrop blur). Keeps
  create/update/delete via Storage.
- `DeckDetailPage` owns the modal state: `+ Add card` (new-card modal for that deck) and card-row
  clicks (edit modal for that card) open it. Card creation lives only in the deck view; Today and the
  Sidebar "+" handle **deck** creation (§4.1, §4.2), not cards.
- **Remove routes** `decks/:deckId/cards/new` and `decks/:deckId/cards/:cardId` from `routes.tsx`.
- The existing `Sheet` component can be reused or specialized; if `Sheet`'s generic styling diverges
  from the mockup modal, give the editor its own modal markup and keep `Sheet` as-is.

## 7. Test impact

- **Screenshot baselines regenerated:** `src/test/screens.browser.test.tsx`,
  `features/decks/DeckListPage.browser.test.tsx`, `features/review/reveal.browser.test.tsx`,
  `features/settings/SettingsPage.browser.test.tsx` (palette/layout change). Regenerate and
  eyeball each.
- **`src/ui/theme-tokens.browser.test.tsx`** — asserts token contrast/values; update expectations to
  the new palette while preserving its AA-legibility intent (muted/faint must stay legible on their
  backgrounds).
- **Editor-route tests** — any test navigating to `…/cards/new|:cardId` switches to opening the
  modal via its trigger. Verify `DeckListPage`/`DeckDetailPage`/`reveal`/`Settings` behavior specs
  still pass against restyled markup (selectors/labels preserved where tests rely on them).
- Run `npm test` and `npm run typecheck`; fix fallout. Manually verify in the desktop app
  (`npm run app:dev`) across both themes.

## 8. Files touched

Rewrites: `src/ui/tokens.css`, `src/ui/styles.css`.
Edits: `src/ui/Sidebar.tsx`, `src/features/decks/DeckListPage.tsx`,
`src/features/cards/DeckDetailPage.tsx`, `src/features/review/ReviewPage.tsx`,
`src/features/review/GradeButtons.tsx`, `src/features/settings/SettingsPage.tsx`,
`src/features/settings/SyncSection.tsx`, `src/app/routes.tsx`, `src/ui/ThemeToggle.tsx` (label/markup).
New: `src/ui/fonts/*.woff2` (+ `@font-face`), `src/features/cards/CardEditorModal.tsx`,
`src/features/review/dueOverview.ts` (cross-deck helper).
Removed: `src/features/cards/CardEditorPage.tsx` (folded into the modal) and its two routes.
Plus: test baselines/expectations as in §7.

## 9. Success criteria

1. App matches `rem.dc.html` across Sidebar, Today, Deck view, Review, Settings, and the edit modal,
   in both dark and light themes, using the self-hosted fonts (no network font fetch).
2. Card editing happens in an in-page modal that keeps the TipTap rich editor (text/code/image).
3. Today's "Start review" runs a shuffled all-decks session over genuinely-due cards; per-deck review
   still works.
4. All scheduling/storage/sync/import-export behavior is unchanged.
5. `npm run typecheck` passes; `npm test` passes with regenerated baselines; the app runs in
   `npm run app:dev`.
