# Phase 2 — Daily-Driver Parity & Developer/Agent Wedge Design

_Date: 2026-11-26 · Branch: `feat/phase-2-parity-and-wedge`_

## Goal

Sharpen `rem`'s daily study and agent-capture loop across Desktop, Terminal CLI/TUI, and iOS without expanding cloud infrastructure or billing surface area. Each slice delivers one user-visible outcome with tests at the public seams and keeps `rem-core` as the single source of truth.

---

## Slice 2.1 — Desktop & TUI Study Parity: Undo (`⌘Z`/`u`) & Suspend (`s`)

### Core & Protocol
- Expose `can_undo: bool` directly on `rem_core::StudyView` (mirroring `StudySession::can_undo()`).
- Add Tauri commands `study_undo_last_grade` and `study_suspend_current` in `src-tauri/src/study.rs`, registered in `src-tauri/src/lib.rs`, delegating to `StudySession::undo_last_grade` and `StudySession::suspend_current`.
- Extend TypeScript `StudyView` (`canUndo: boolean`), `Storage` (`undoStudyGrade`, `suspendStudyCard`), `TauriStorage`, and the browser-test adapter (`DexieStorage` / `BrowserStudySession`).

### Desktop UI (`ReviewPage.tsx`)
- Add **Undo** (`⌘Z` / `Ctrl+Z` / `u`) and **Suspend** (`s`) header actions during active non-preview review; disable Undo when `!view.canUndo`.
- Allow Undo (`⌘Z` / `Ctrl+Z` / `u` and an **Undo last grade** button) on the **Review complete** screen when `view.canUndo` is true so accidentally grading the last card is recoverable.
- Keep session summary stats (`total`, `recalled`) accurate on undo, and surface a subtle status notice (`Last grade undone.` / `Card suspended.`).

### Terminal TUI (`rem-cli/src/tui.rs` & `main.rs`)
- Add `StudyInput::Undo` (`u` or `Ctrl+Z`) and `StudyInput::Suspend` (`s`).
- Allow `Undo` whenever `view.can_undo` (including on completion), and `Suspend` whenever a current card exists.
- Render status feedback in the header notice line and key hints in the footer.

---

## Slice 2.2 — Retained Draft Provenance & Terminal Draft Triage (`rem draft review`)

### Schema v5 & Core Provenance (`rem-core`)
- Bump `CURRENT_SCHEMA_VERSION` from `4` to `5`.
- Add `rationale TEXT`, `sources_json TEXT NOT NULL DEFAULT '[]'`, and `proposed_by TEXT` columns to `cards` (plus FTS5 index for Slice 2.3 in the same v5 migration so we only bump the schema once).
- Add `rationale: Option<String>`, `sources: Vec<DraftSource>`, and `proposed_by: Option<String>` to `Card` and `CardBackup` (all `#[serde(default)]` for backwards compatibility).
- Add `#[serde(default)]` `rationale`, `sources`, `proposed_by` to `SyncOp::CardCreate` and apply them in `sync_store.rs` and `import_store.rs`. `UserSyncDO` stores op payloads opaquely, so no server changes are required.
- On `Collection::resolve_draft` (`DraftDecision::Accept`), automatically copy `draft.rationale`, `draft.sources`, and `draft.proposed_by` onto the newly created `Card`.

### Provenance Inspection Across Surfaces
- **Desktop (`ReviewPage.tsx`):** When a card is revealed and has provenance, render a quiet collapsible **Source & rationale** disclosure below the answer.
- **TUI (`rem study`):** When a card is revealed and has provenance, render a muted `PROVENANCE` section below the answer.
- **iOS (`CardDto`, `ReviewMenuSheet`, `ReviewScreen`):** Expose `rationale`, `sources`, and `proposedBy` on `CardDto` and display provenance in the Card Info sheet (and subtle expandable section on reveal when present).

### Terminal Draft Triage (`rem draft review`) & CLI Decomposition
- Extract CLI commands and interactive flows out of `src-tauri/crates/rem-cli/src/main.rs` into focused modules (`draft_tui.rs`, `editor.rs`, etc.).
- Add `rem draft review [--deck <id-or-exact-name>]`:
  - Front-first Ratatui TUI mirroring Desktop's Drafts inbox:
    - Shows `Draft i/N · <Deck>`, tags, and **Question** first.
    - `Space`/`Enter`: reveal proposed **Answer**, **Rationale**, **Sources**, and **Proposed by**.
    - `a`: accept draft (creates due card with retained provenance; handles conflicts & existing exact duplicates cleanly).
    - `r`: reject draft.
    - `e`: edit draft (`deck`, `tags`, `front`, `back`) in `$VISUAL` / `$EDITOR` via a clean Markdown temp file, returning to the TUI with edits applied.
    - `n` / `Tab`: skip to next draft without deciding.
    - `↑`/`↓`/`j`/`k`: scroll; `q`: exit cleanly with triage summary.

---

## Slice 2.3 — Global Cross-Deck Search (`⌘K`, iOS Search, `rem card search`)

### Core FTS5 Search (`rem-core/src/search.rs`)
- In schema v5, create `cards_fts` (`fts5(card_id UNINDEXED, front, back, tags, rationale, tokenize='unicode61')`), backfill existing rows, and keep it synchronized via SQLite triggers (`AFTER INSERT`, `AFTER DELETE`, `AFTER UPDATE OF front, back, tags_json, rationale`) so every write/sync/import/cascade path stays automatically consistent.
- Add `Collection::search_cards(query: &str, deck_id: Option<&str>, limit: usize) -> Result<Vec<CardSearchHit>, CollectionError>`, where `CardSearchHit` carries `card`, `deck_name`, and `deck_color`.
- Use FTS5 prefix queries (`"token"*`) ranked by `bm25`, falling back to case-insensitive `LIKE` when queries contain punctuation/code operators that FTS5 tokenizers strip.

### Surfaces
- **Desktop:** Add `storage_search_cards` Tauri command, `Storage.searchCards`, and a global `SearchModal` (`⌘K` / `Ctrl+K` + Sidebar Search button) with keyboard navigation (`↑`/`↓`/`Enter`/`Esc`) jumping directly to the matched card.
- **CLI:** Add `rem card search <query> [--deck ...] [--limit N] [--output text|json]`.
- **iOS:** Expose `search_cards` via `rem-ffi` and upgrade `DecksView`'s `.searchable` bar ("Search decks and cards") to show both matching decks and cross-deck card hits.

---

## Slice 2.4 — Unified Stats in `rem-core` & iOS Study/Backup Parity

### Unified Stats (`rem-core/src/stats.rs`)
- Move `compute_stats` and `compute_today_summary` into `rem-core/src/stats.rs` as the single implementation for Desktop and iOS, including 7-day upcoming review forecast (`upcoming_7d`) and `history_count`.
- Thin `rem-ffi/src/stats.rs` to delegate directly to `rem-core` (retaining only interval formatting helpers).
- Expose `storage_get_stats` in `src-tauri/src/storage.rs` and `Storage.getStats` so Desktop's `StatsPage` reads stats directly from `rem-core` in a single native call (keeping `DexieStorage` delegating to `buildStats` for browser tests).

### iOS Custom Study
- Expose `CustomStudyModeDto` (`StudyAhead`, `IncreaseNew`, `ReviewForgotten`, `PreviewNew`), `start_custom_study` on `RemClient`, and `advance_preview` + `preview: bool` on `RemStudySession` / `StudyViewDto`.
- Update `ReviewScreen.swift` for non-rescheduling preview mode (`Next card` dock + `Preview complete` summary).
- Add `CustomStudySheet.swift` accessible from `DeckDetailView`'s deck actions menu.

### iOS JSON Backup Import
- Expose `preview_backup_json` and `import_backup_json` in `rem-ffi` backed by `Collection::import_decks`.
- Add **Import backup (JSON)** in `SettingsView.swift` using native `.fileImporter`, confirming before replacing any same-named decks, then refreshing state and triggering silent sync.

---

## Validation Plan

- `cargo fmt --check`, `cargo clippy --workspace --all-targets`, and `cargo test --workspace` (including new unit/integration tests for undo/suspend, schema v4→v5 migration, provenance sync/backup round-trips, FTS5 search + triggers, unified stats, `rem-ffi` boundary tests, and `rem-cli` PTY/command tests).
- `npm test` (unit + Playwright browser tests) and `npm run build`.
- Rebuild `RemCore.xcframework` + UniFFI Swift bindings via `tools/build-ios-xcframework.sh` and run `xcodebuild test` for `RemAppTests` on the iOS Simulator.
