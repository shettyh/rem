# Deck options screen + `DeckSettings` model — design

_Date: 2026-06-28_

## Context

This is **sub-project #1** of the larger effort to (a) implement the full
Anki-grade per-deck "Deck options" screen from `rem.dc.html`, and (b) port
scheduling from `ts-fsrs` to `fsrs-rs` (Rust). The agreed decomposition:

1. **Deck options screen + `DeckSettings` data model ← this spec**
2. Rust FSRS scheduling core — ✅ done, merged to `main` (`0fc5f8d`)
3. Anki-grade review queue (learning/relearning steps, daily caps, insertion
   order, leech, burying)
4. Custom study
5. FSRS weight optimization

We build **#1 now** because it is the literal `rem.dc.html` deliverable and it
pins the settings schema every later phase consumes. #2 already left the hook:
`TauriFsrsScheduler.previewNextStates` sends a module-constant `DeckFsrsParams`;
#1 stores per-deck settings, and #3 threads them through that existing argument.

**Design source:** Claude Design project `87928d48-56a1-4f63-a282-8e6f287e918a`,
file `rem.dc.html`, the `deckSettings` view (reached via an **Options** button on
the deck header; back-arrow returns to the deck).

## Goal

Implement the per-deck **Deck options** screen for real — every section from the
comp (General, Daily limits, New cards, Lapses, Custom study, Burying & timer,
Danger zone) — and the `DeckSettings` data model behind it. **#1 persists every
setting and renders the whole screen; #3 makes the settings actually drive the
review queue.** That UI+storage / enforcement split is the core scoping decision.

Success = opening Deck options shows all sections seeded from the deck, editing
any control persists to storage, deck renames/color/settings survive a reload and
sync deterministically across machines, and old backups/snapshots still import.

## Decisions (from brainstorming)

- **Full Anki-grade settings, persisted now.** `DeckSettings` carries the whole
  schema (learning/relearning steps, graduating/easy intervals, daily caps,
  insertion order, leech threshold+action, burying, timer) plus FSRS params.
- **FSRS-only — the SM-2 toggle is dropped.** The codebase removed SM-2 (Dexie v5
  migration, `SchedulerKind = 'fsrs'`); the comp's FSRS/SM-2 toggle becomes a
  static "FSRS" line. Reviving SM-2 is out of scope.
- **Custom study renders inert now.** The 4 presets + N stepper + Start render per
  the comp, but **Start is disabled / no-op** until sub-project #4 wires behaviour.
- **Deck edits sync via last-write-wins.** `merge.ts` treats decks as immutable
  today (local wins). #1 makes name/color/settings editable, so `Deck` gains an
  `updatedAt` and merge switches to *newest `updatedAt` wins*.
- **A desired-retention control is added** (beyond the comp). `DeckSettings`
  stores `desiredRetention` + `maximumInterval`; the screen surfaces a retention
  stepper. `maximumInterval` is stored defaulted with no UI.
- **Full-screen route, not a modal.** `/decks/:deckId/options`, reached via an
  Options button in the deck header; back-arrow returns. Content is long and
  linkable; a `Sheet` would cramp it.

### Scope boundary (UI+storage now / enforce in #3)

Persisted by #1 but **not yet enforced** — that is #3's job:
learning/relearning steps, daily caps, insertion order, leech, burying, and the
**live threading** of `desiredRetention`/`maximumInterval` into the
`fsrs_next_states` command. The retention control is real and saves; it changes
scheduling once #3 threads the whole settings object through the existing
`DeckFsrsParams` argument. Custom study behaviour is #4.

## Data model (`src/domain/models.ts`)

`Deck` gains three fields; a `DeckSettings` type + `DEFAULT_DECK_SETTINGS` are new.

```ts
export interface Deck {
  id: ID
  name: string
  createdAt: number
  updatedAt: number          // NEW — drives sync last-write-wins
  color: string              // NEW — hex from the swatch palette
  schedulerKind: SchedulerKind
  settings: DeckSettings     // NEW
}

export type InsertionOrder = 'sequential' | 'random'
export type LeechAction = 'tag' | 'suspend'

export interface DeckSettings {
  // daily limits
  newPerDay: number
  maxReviews: number
  // new cards
  learnSteps: string          // space-separated, e.g. '1m 10m'
  graduatingInterval: number  // days
  easyInterval: number        // days
  insertionOrder: InsertionOrder
  // lapses
  relearnSteps: string        // space-separated, e.g. '10m'
  minimumInterval: number     // days
  leechThreshold: number      // lapses before flagged
  leechAction: LeechAction
  // burying & timer
  buryRelated: boolean
  showTimer: boolean
  // FSRS params (threaded into the scheduler in #3)
  desiredRetention: number    // 0..1
  maximumInterval: number     // days
}

export const DEFAULT_DECK_SETTINGS: DeckSettings = {
  newPerDay: 20, maxReviews: 200,
  learnSteps: '1m 10m', graduatingInterval: 1, easyInterval: 4,
  insertionOrder: 'sequential',
  relearnSteps: '10m', minimumInterval: 1, leechThreshold: 8, leechAction: 'suspend',
  buryRelated: true, showTimer: false,
  desiredRetention: 0.9, maximumInterval: 36500,
}
```

The `learnSteps`/`relearnSteps` raw strings are stored verbatim; #1 only splits
them on whitespace to render chips. Step-format validation (`1m`/`10m`/`1d`) is
#3's concern (the queue consumes them).

## Persistence (`Storage` seam + `DexieStorage`)

One new method, mirroring the existing `updateCard(id, patch)`:

```ts
updateDeck(id: ID, patch: { name?: string; color?: string; settings?: DeckSettings }): Promise<void>
```

Implementation stamps `updatedAt = Date.now()` alongside the patch. `createDeck`
now also assigns `updatedAt`, `color` (= `deckColor(newId)`), and
`settings: DEFAULT_DECK_SETTINGS`. `deleteDeck` already exists and is reused by
the Danger zone.

**Dexie v6 migration** (additive; schema/indexes unchanged — the new fields live
in the stored JSON payload, none are indexed): backfill every existing deck with
`updatedAt = createdAt`, `color = deckColor(id)`, `settings = DEFAULT_DECK_SETTINGS`.

## Sync + backup

- **`merge.ts`** — deck reconciliation changes from *"union by id, local wins"* to
  *"union by id, newest `updatedAt` wins"*, keeping the existing tombstone-drop
  logic (`deck` tombstone at/after `createdAt` drops the deck). This is the only
  behavioural merge change. `dbOps.upsertDecks` already carries the merged decks.
- **`snapshot.ts`** — `DeckRecord` gains `updatedAt`/`color`/`settings`.
  `deserializeSnapshot` normalizes older snapshots that lack them
  (`updatedAt → createdAt`, `color → deckColor(id)`, `settings → DEFAULT_DECK_SETTINGS`).
- **`backup.ts`** — `DeckBackup` carries optional `color`/`settings`; `BackupFile`
  stays `version: 1`. `parseDeck` defaults the new fields so **old backups still
  import**; `collectBackup` includes them so new backups round-trip.

## The screen (`src/features/decks/DeckSettingsPage.tsx`)

New route in `src/app/routes.tsx`: `decks/:deckId/options`. The page loads the
deck via `useLiveQuery`, seeds local form state from it, and writes back through
`updateDeck` — toggles / swatches / segmented controls / steppers persist
immediately; text inputs (deck name, learn/relearn steps) persist on blur.
Header: back-arrow
`‹ {deck.name}` + color dot + "Deck options".

Sections (each a card, matching the comp):

| Section | Controls |
|---|---|
| **General** | name input (rename); color swatches; static "Scheduler — FSRS" line (no toggle); **Desired retention** stepper |
| **Daily limits** | `newPerDay` stepper; `maxReviews` stepper |
| **New cards** | `learnSteps` input + parsed chips; `graduatingInterval` stepper; `easyInterval` stepper; insertion-order segmented (SEQ/RANDOM) |
| **Lapses** | `relearnSteps` input + parsed chips; `minimumInterval` stepper; `leechThreshold` stepper; leech-action segmented (TAG/SUSPEND) |
| **Custom study** | 4 preset cards + N stepper + Start — rendered, **inert** (Start disabled until #4) |
| **Burying & timer** | `buryRelated` toggle; `showTimer` toggle |
| **Danger zone** | Delete deck → inline two-step confirm → `deleteDeck` + navigate `/` |

Stepper increments / clamps:

| Control | Step | Clamp |
|---|---|---|
| `newPerDay` | 5 | 0–9999 |
| `maxReviews` | 10 | 0–9999 |
| `graduatingInterval` / `easyInterval` / `minimumInterval` | 1 | 1–365 |
| `leechThreshold` | 1 | 1–99 |
| Desired retention | 1% | 70%–99% (stored as 0.70–0.99) |

`DeckDetailPage` header gains an **Options** ghost button → the new route, shown
even for empty decks (the comp shows it unconditionally).

**Color swatches:** reuse the existing 5-color `DECK_PALETTE` from
`src/ui/deckColor.ts` as the swatch set (the comp shows 6; we map to our 5 to stay
consistent with current deck dots). `deckColor(id)` stays as the create-time /
fallback hash; the two call sites (`Sidebar.tsx`, `DeckDetailPage.tsx`) change
from `deckColor(deck.id)` to `deck.color ?? deckColor(deck.id)`.

## Reusable UI + CSS

Three small components in `src/ui/` (repetition justifies extraction):

- **`Stepper`** — `− value +`, used ~7×. Props: value, onChange, step, min, max,
  optional display formatter (e.g. `90%`, `4d`).
- **`SegToggle`** — 2-option segmented control, used 2× (insertion order, leech
  action). Props: options, value, onChange.
- **`Toggle`** — switch, used 2× (bury, timer). Props: checked, onChange.

New CSS in `src/ui/styles.css`, mapping the comp's tokens onto the app's design
system: `--soft → --accent-soft`, `--surface2 → --surface-inset`,
`--border2 → --border-strong`, `--accentfg → --on-accent`; Space Mono for
micro-labels and numeric values; section labels in faint uppercase; Danger zone
uses `--again`.

## Testing (TDD)

**Unit:**
- `DEFAULT_DECK_SETTINGS` shape; `updateDeck` stamps `updatedAt` and persists the
  patch; `createDeck` seeds defaults.
- Dexie v6 migration backfills `updatedAt`/`color`/`settings` on a pre-existing
  deck.
- `backup`: serialize→parse preserves `color`/`settings`; an old backup (no
  settings) parses to `DEFAULT_DECK_SETTINGS`.
- `snapshot`: round-trip preserves the new fields; old snapshot normalizes to
  defaults.
- `merge`: newest `updatedAt` deck wins; tombstone-drop still holds.
- steps-string → chips parse (whitespace split, empties dropped).

**Browser (`.browser.test.tsx`):**
- Deck options renders all sections from a seeded deck.
- Editing a stepper / rename / color swatch / toggle each persists (reflected via
  `useLiveQuery` / reload).
- Delete-with-confirm removes the deck and navigates to `/`.
- Options button on `DeckDetailPage` navigates to `/decks/:id/options`.

## Files touched

- `src/domain/models.ts` — `Deck` fields, `DeckSettings`, `DEFAULT_DECK_SETTINGS`.
- `src/data/Storage.ts` — `updateDeck` on the port.
- `src/data/dexie/DexieStorage.ts` — `updateDeck`, `createDeck` defaults.
- `src/data/dexie/db.ts` — v6 migration.
- `src/data/sync/merge.ts` — deck LWW.
- `src/data/sync/snapshot.ts` — `DeckRecord` fields + deserialize defaults.
- `src/data/backup.ts` — `DeckBackup` fields + parse defaults.
- `src/app/routes.tsx` — `decks/:deckId/options` route.
- `src/features/decks/DeckSettingsPage.tsx` — new screen.
- `src/features/cards/DeckDetailPage.tsx` — Options button + `deck.color`.
- `src/ui/Sidebar.tsx` — `deck.color` fallback.
- `src/ui/Stepper.tsx`, `src/ui/SegToggle.tsx`, `src/ui/Toggle.tsx` — new.
- `src/ui/styles.css` — deck-options styles.
- Tests alongside each.

## Out of scope (later sub-projects)

Queue enforcement of steps/caps/order/leech/burying and live FSRS-param threading
(#3); custom-study behaviour (#4); FSRS weight training (#5).
