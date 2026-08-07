# Per-deck settings + Rust FSRS — continuation / next steps

_Last updated: 2026-08-07_

> **Update 2026-08-07 (#5):** Per-deck FSRS weight optimization is implemented on
> `feat/fsrs-optimization`: atomic FSRS-effective review logs, Dexie v11, full backup/git-sync
> history, native fsrs-rs training, persisted per-deck weights, and Optimize/Reset UI. Automated
> verification is green: 278 TypeScript/browser tests, build, and 15 Rust tests. This completes the
> five-part per-deck-settings/Rust-FSRS effort. **Recommended next product slice = Stats screen,**
> which can now build on the durable review log.
>
> **Update 2026-08-07 (#4):** Custom study is DONE, merged to `main` (PR #7, merge
> `021b8f1`). It added temporary study-ahead, additional-new, forgotten-card, and
> non-rescheduling new-card preview sessions.
>
> **Update 2026-08-07 (#3c):** Leech handling is DONE, merged to `main` (PR #6, merge
> `0b71233`). It added durable `leech` tags/suspension, per-deck threshold actions, due-queue
> exclusion, and editor recovery. Related-card burying remains deferred until rem has a real
> note/template/sibling model.
>
> **Update 2026-08-07 (#3b):** Daily caps are DONE, merged to `main` (PR #3, merge
> `6f8e58c`). They added Dexie v8 daily counters, per-deck queue/Today caps, and grade accounting.
>
> **Update 2026-07-04:** Sub-project **#3 was sliced into #3a / #3b / #3c** (spec:
> `docs/superpowers/specs/2026-07-02-anki-review-queue-steps-design.md`). **#3a — learning/
> relearning steps + real per-deck FSRS params + insertion order — is DONE**, merged to `main`
> (PR #1, merge `8289d7d`; 213 tests + cargo green). It threaded the `DeckFsrsParams` hook,
> added `FSRSState.step` (Dexie v7), a TS step machine (`reviewScheduler.ts`), and a dynamic
> session queue (`session.ts`). **Still open in #3:** **#3b** daily caps (`newPerDay`/`maxReviews`,
> needs review-log/daily-counter infra — also unblocks #5) and **#3c** leech + burying (needs a
> card `suspended` flag / note model). A small **#3a follow-up** (completion-copy "N reviews done",
> double-grade in-flight guard, reviewScheduler cast/DRY cleanups) is on branch
> `fix/review-followup-cleanups`. **Recommended next real slice = #3b.**

A resume doc for the multi-part effort kicked off from the `rem.dc.html` design and the
"switch to Rust FSRS" idea. Sub-projects **#1 and #2 are shipped**; this captures the decisions and
the concrete next action for everything still pending so a future session can pick up cleanly.

> **Update 2026-06-28:** Sub-project **#1 (Deck options screen + `DeckSettings`) is DONE** — merged to
> `main` (`b4bf3b9`) and pushed. Spec: `docs/superpowers/specs/2026-06-28-deck-options-screen-design.md`;
> plan: `docs/superpowers/plans/2026-06-28-deck-options-screen.md`. The `DeckSettings` schema,
> `Storage.updateDeck`, the Dexie v6 migration, backup/snapshot round-trip, deck last-write-wins sync,
> and the full Deck options UI all landed. **Recommended next step is now #3** (Anki-grade review queue),
> which consumes #1's settings. Settings are persisted but **not yet enforced** — #3 threads them into
> `fsrs_next_states` (the `DeckFsrsParams` hook in `src/domain/scheduler/tauriFsrs.ts`) and drives the
> queue from learning/relearning steps, daily caps, insertion order, leech, and burying.

## The two original asks
1. Implement the per-deck **"Deck options"** screen from the design comp `rem.dc.html`.
2. Switch scheduling from `ts-fsrs` (JS) to **fsrs-rs** (Rust).

**Design source:** Claude Design project `87928d48-56a1-4f63-a282-8e6f287e918a`, file `rem.dc.html`
(read via the `claude_design` MCP / DesignSync `get_file`).

## Decisions locked in (do NOT re-litigate without a reason)
- **Full Anki-grade deck options** — implement the whole screen for real (learning/relearning steps,
  graduating/easy intervals, daily caps, insertion order, leech detection+action, burying, custom
  study), not a reduced FSRS-only subset.
- **Full port to Rust** — all scheduling runs in fsrs-rs via Tauri; review-time scheduling is
  **async + native-only** (accepted trade-off). TS tests run on a `FakeScheduler`; the FSRS math is
  cargo-tested.
- **Open question to confirm at #1:** the design shows a **FSRS / SM-2** scheduler toggle, but SM-2
  was already removed from the repo (FSRS-only). Default plan = **drop the toggle, FSRS-only**.
  Revisit only if you actually want SM-2 back.

## The 5 sub-projects

| # | Sub-project | Status | Depends on |
|---|---|---|---|
| 1 | **Deck options screen + `DeckSettings` data model** | ✅ **DONE — merged to `main` (`b4bf3b9`)** | — |
| 2 | **Rust FSRS scheduling core** | ✅ **DONE — merged to `main` (`0fc5f8d`)** | 1 (for real params) |
| 3a | **Review-queue steps** (learning/relearning steps, real per-deck params, insertion order) | ✅ **DONE — merged to `main` (`8289d7d`, PR #1)** | 1, 2 |
| 3b | **Daily caps** (`newPerDay`/`maxReviews` + daily-counter infra) | ✅ **DONE — merged to `main` (`6f8e58c`, PR #3)** | 3a |
| 3c.1 | **Leech handling** (tag/suspend + recovery) | ✅ **DONE — merged to `main` (`0b71233`, PR #6)** | 3a |
| 3c.2 | **Related-card burying** | **DEFERRED — requires note/template/sibling model** | note model |
| 4 | **Custom study** (study-ahead / increase-new / review-forgotten / preview-new) | ✅ **DONE — merged to `main` (`021b8f1`, PR #7)** | 3 |
| 5 | **FSRS weight optimization** (train personalised weights from review logs) | ✅ **IMPLEMENTED — awaiting merge from `feat/fsrs-optimization`** | 2, 3 |

Each sub-project is its own cycle: **brainstorming → writing-plans → subagent-driven-development →
finishing-a-development-branch.** Specs go in `docs/superpowers/specs/`, plans in
`docs/superpowers/plans/`. The SDD scratch ledger (git-ignored) is `.superpowers/sdd/progress.md`.

## What #2 already shipped (and the hooks it left for the rest)
Merged on `main` (`0fc5f8d`). Spec: `docs/superpowers/specs/2026-06-28-rust-fsrs-scheduling-core-design.md`;
plan: `docs/superpowers/plans/2026-06-28-rust-fsrs-scheduling-core.md`.

- **Rust command** `fsrs_next_states(state, now, params)` in `src-tauri/src/fsrs_sched.rs`
  (fsrs crate 6.6.1, FSRS-6). `params: DeckFsrsParams { desiredRetention, maximumInterval, weights }`.
  Returns all four grade outcomes as complete next-states. cargo-tested.
- **TS seam** (`src/domain/scheduler/`): `getScheduler()` → `TauriFsrsScheduler` (real `invoke`) in the
  app, `FakeScheduler` in tests, chosen by `isTauri()`. `initial()` is sync; `previewNextStates()` is
  the single async round-trip; `ReviewPage` caches the four outcomes on reveal and shows a recoverable
  "Couldn't schedule this card." + **Retry** if the `invoke` rejects. `ts-fsrs` is gone.
- **HOOK for #1/#3 — per-deck params:** `TauriFsrsScheduler.previewNextStates` currently sends a
  module constant `DEFAULT_DECK_FSRS_PARAMS = { desiredRetention: 0.9, maximumInterval: 36500, weights: null }`
  from `src/domain/scheduler/tauriFsrs.ts`. **#1 stores the deck's settings; #1/#3 thread them here**
  (and into the command) — the `DeckFsrsParams` argument already exists, so no signature change.
- **NOT yet built (deliberately):** only `state = 2` is emitted — no learning/relearning steps, daily
  caps, leech, or burying (that's #3). No review-log capture yet (#3 must add it; #5 needs it). Weights
  are always defaults (#5 trains them).

## Historical implementation notes — Sub-project #1
Build the **Deck options screen + `DeckSettings` model**. It's the literal `rem.dc.html` deliverable
and it pins the settings schema every later phase consumes. Start with the **brainstorming** skill.

What the design's "Deck options" view (`deckSettings`) contains — reached via an **Options** button on
the deck header; back-arrow returns to the deck:
- **General** — deck name (rename), color swatches, scheduler toggle (see open question above).
- **Daily limits** — new cards/day, maximum reviews/day (steppers).
- **New cards** — learning steps (`1m 10m 1d`, space-separated, chips), graduating interval (days),
  easy interval (days), insertion order (SEQ / RANDOM).
- **Lapses** — relearning steps (`10m`), minimum interval (days), leech threshold (count), leech action
  (TAG / SUSPEND).
- **Custom study** — 4 presets (study ahead / increase new / review forgotten / preview new) + N
  stepper + Start. (Behaviour is #4; #1 can render the UI.)
- **Burying & timer** — bury related new cards (toggle), show answer timer (toggle).
- **Danger zone** — delete this deck (with a confirm step). `Storage.deleteDeck` already exists.

Design `DEFAULT_CFG` from the comp: `newPerDay 20, maxReviews 200, learnSteps '1m 10m', graduate 1,
easyInt 4, relearnSteps '10m', minInt 1, leech 8, leechAction 'suspend', order 'sequential',
bury true, timer false`. Map the comp's tokens/colours onto the app's existing design system rather
than copying its inline styles.

Brainstorm questions to settle for #1:
- In full-Anki-grade, **#1 persists ALL settings** on the deck; **#3 makes steps/leech/limits/burying
  actually drive the review queue.** Confirm that split (UI+storage now, enforcement in #3) so #1
  doesn't balloon into the queue rewrite.
- `Deck` model needs `color` + a `settings: DeckSettings` object; `Storage` needs rename / set-settings
  (deleteDeck exists). Decide sync/backup handling of the new fields (the `Storage` seam + `backup.ts`
  + snapshot format already round-trip decks).

## Outstanding for #2 (optional, user-side)
- **Live GUI smoke** of the real `invoke('fsrs_next_states')` path: `npm run app:dev` → review a card →
  confirm four ascending intervals + reschedule + no console error (automated tests use the fake, so
  this path is the only thing not exercised end-to-end).
- To see the new error UI, temporarily make the Rust command return `Err` and confirm the
  "Couldn't schedule this card." + Retry appears.
- `main` was **not pushed** to origin (local-first). Push when ready.
