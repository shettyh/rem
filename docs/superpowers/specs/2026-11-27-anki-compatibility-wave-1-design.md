# Wave 1 — Markdown-Native Anki Compatibility & Migration Design

_Date: 2026-11-27 · Branch: `feat/anki-compatibility-wave-1`_

## Goal

Make `rem` a practical daily driver for existing Anki users while preserving `rem`'s calm, local-first Markdown model, shared Rust core (`rem-core`), HLC cloud sync, and native SwiftUI iOS experience. TUI compatibility is graceful fallback rather than a hard constraint.

Wave 1 delivers five coordinated pillars:

1. **Nested decks (`::` namespace convention)**
2. **LaTeX / Math rendering (`$...$` / `$$...$$`, plus Anki `\(...\)` / `\[...\]` normalization)**
3. **Cloze deletions + lightweight siblings (`sibling_group_id`, `cloze_index`, and `bury_related` enforcement)**
4. **Audio assets + pragmatic `.apkg` importer (`anki2` / `anki21` / zstd `anki21b`, media, `revlog` → FSRS-7)**
5. **Show answer timer** (wiring up the persisted `DeckSettings.show_timer` setting)

---

## 1. Nested Decks (`::` Namespace Convention)

- **No schema or sync-protocol change:** `decks.name` continues to store the full deck path using Anki's `::` separator (e.g. `Languages::Japanese::Core`).
- **Canonical helpers (`rem-core/src/deck_tree.rs`):**
  - `normalize_deck_name`: trims outer whitespace and cleans empty/whitespace padding around `::` segments (rejects blank names or names with empty segments like `A::::B` or `::A`).
  - `deck_is_descendant_or_self(candidate_name, root_name)`: true when `candidate == root` or `candidate` starts with `{root}::`.
  - `Collection::deck_and_descendant_ids(&self, deck_id: &str)`: returns `[deck_id]` plus all descendant deck IDs.
- **Study & stats semantics:**
  - `StudySession::start` with `deck_id: Some(id)` loads cards from `id` **and all its descendant decks** (both normal study and custom study), applying each card's own deck settings (`DeckSettings`) and daily caps.
  - `compute_today_summary` returns exact per-deck counts (preventing double-counting in `total_due`); UI tree builders roll up descendant totals onto parent rows for display.
- **Surfaces:**
  - **Desktop (`DeckListPage`, `Sidebar`, `DeckDetailPage`):** Collapsible deck tree supporting both real parent decks and virtual parent folders (when `A::B` exists without a literal `A` deck), rolled-up due/new/total/studied badges, persisted collapse state, and a Subdecks section + "Add subdeck" affordance on `DeckDetailPage`.
  - **iOS (`TodayView`, `DecksView`, `DeckDetailView`):** Collapsible hierarchical deck rows with indentation, rolled-up due/card counts, and Subdecks section on `DeckDetailView`.

---

## 2. LaTeX / Math Rendering

- **Canonical Markdown syntax:** `$inline$` and `$$display$$` (and `\(...\)` / `\[...\]` normalized cleanly outside fenced/inline code blocks).
- **Desktop (`MarkdownView.tsx`):** `remark-math` + `rehype-katex` + bundled `katex/dist/katex.min.css` (100% offline). Currency like `$5 and $10` and shell variables inside code spans/fences remain untouched.
- **iOS (`RemMarkdownView.swift` + `MarkdownBlockParser.swift` + `MathRenderer.swift`):** 100% native SwiftUI math formatting (no WebView, no external SPM packages):
  - Display math blocks (`$$...$$`) render in a calm inset math card with stacked fractions (`\frac{a}{b}`), superscripts/subscripts, Greek symbols, and standard math operators.
  - Inline math (`$...$`) renders inline in serif italic math styling with Unicode symbol translation and superscripts/subscripts.
- **TUI:** Renders raw math expression cleanly as text.

---

## 3. Cloze Deletions + Lightweight Siblings (`bury_related`)

- **Schema v6 (`rem-core/src/collection.rs`):**
  - Bump `CURRENT_SCHEMA_VERSION` from `5` to `6`.
  - Add nullable `sibling_group_id TEXT` and `cloze_index INTEGER CHECK (cloze_index IS NULL OR cloze_index >= 1)` columns to `cards`, plus partial index `cards_sibling_group ON cards(sibling_group_id) WHERE sibling_group_id IS NOT NULL`.
  - Add `#[serde(default, skip_serializing_if = "Option::is_none")]` `sibling_group_id` and `cloze_index` to `Card`, `CardBackup`, and `SyncOp::CardCreate` (backwards-compatible with existing sync servers and backups; `UserSyncDO` stores ops opaquely).
  - In `import_decks`: remap each distinct incoming `sibling_group_id` to a fresh UUID per import so siblings stay grouped without cross-import collisions.
- **Cloze parser & renderer (`rem-core/src/cloze.rs`, `src/features/cards/cloze.ts`, `ios/RemApp/Markdown/ClozeRenderer.swift`):**
  - Parses Anki syntax `{{cN::answer}}` and `{{cN::answer::hint}}` (`N >= 1`), supporting nested braces inside LaTeX (e.g. `{{c1::\frac{1}{2}}}`).
  - `extract_cloze_indices(text)` returns sorted unique `N` values.
  - **Creation:** `Collection::create_card_or_cloze` (used by Desktop/iOS card editors) detects cloze markers in `front`:
    - If no cloze markers: creates 1 normal card (`cloze_index: None`).
    - If cloze markers `c1, c2, ...`: creates 1 independently scheduled `Card` per distinct index sharing one `sibling_group_id`, with `cloze_index = Some(n)`.
  - **Editing:** `Collection::update_cloze_note` updates shared `front`/`back`/`tags` across sibling cloze cards in the same `sibling_group_id` and creates cards for any newly introduced cloze indices.
  - **Display:**
    - Unrevealed question: active `cN` renders as `[...]` or `[hint]`; other `cM` render as plain text.
    - Revealed: active `cN` highlights `answer` in place; if `back` (Extra) is non-empty, renders below divider as **Extra**.
    - Lists/search previews strip cloze braces cleanly.
- **Sibling burying (`bury_related` in `StudySession`):**
  - When `settings.bury_related == true` on normal study sessions (`request.custom.is_none()`):
    - At `StudySession::start`: compute `day_start_ms` for the current local day. Any sibling group that already has a card with `last_review >= day_start_ms` buries its *other* cards (`card.id != reviewed_card_id`) from today's queue (while allowing an in-progress learning card itself to finish its steps).
    - In `StudySession::grade`: if the graded card has `sibling_group_id` and `settings.bury_related`, remove other cards with the same `sibling_group_id` from `self.queue` and store them in `LastGradeUndo.buried_siblings`.
    - In `StudySession::undo_last_grade`: restore `buried_siblings` back into `self.queue` (and restoring `last_review` automatically unburies them for subsequent sessions too).
    - `compute_today_summary` mirrors the same already-buried filter so due counts match what `StudySession` will actually serve.

---

## 4. Audio Assets & Pragmatic `.apkg` Importer

### Audio Assets
- Content-addressed audio files (`audio/mpeg`, `audio/mp4`, `audio/ogg`, `audio/wav`, `audio/webm`, `audio/aac`, `audio/flac`) are stored in `assets` and synced through R2 with zero server changes.
- Markdown representation: `[Audio: filename](asset:<64-hex-hash>)`.
- **Desktop (`MarkdownView.tsx`):** Resolves `asset:<hash>` links whose MIME starts with `audio/` into a clean inline `<AudioPlayer>` component. Card editor allows attaching audio files alongside images.
- **iOS (`RemMarkdownView.swift`):** Parses `[label](asset:<hash>)` audio links into an inline `AssetAudioPlayerView` backed by `AVAudioPlayer`.
- **TUI:** Displays `[Audio: filename]`.

### Pragmatic `.apkg` Importer (`rem-core/src/apkg.rs`)
- Pure-Rust ZIP (`zip`) + pure-Rust zstd (`ruzstd`) + `rusqlite` read-only temp file extraction (no C cross-compilation dependencies, safe on macOS/Linux/Windows/iOS).
- **Archive & format support:**
  - Selects `collection.anki21b` (zstd-compressed SQLite) > `collection.anki21` > `collection.anki2`.
  - Decodes both classic JSON `media` manifests and modern `anki21b` zstd-compressed protobuf `media` manifests (plus zstd-decompression of individual media blobs when magic bytes `28 B5 2F FD` are present).
  - Enforces strict ZIP-bomb & size limits (max DB 256 MiB, max single media 25 MiB, max total media 512 MiB, max entries 100,000).
- **Schema & content mapping:**
  - Supports both legacy (`col.decks` / `col.models` JSON) and modern (`decks`, `notetypes`, `fields`, `templates` tables + protobuf configs).
  - Normalizes `\x1f` deck separators to `::`.
  - Handles Basic, Basic & Reversed (grouped by note `sibling_group_id`), Cloze (`cloze_index = ord + 1`, grouped by note `sibling_group_id`), and multi-field templates (substituting `{{Field}}`, `{{FrontSide}}`, `{{cloze:Field}}`, conditional `{{#Field}}`/`{{^Field}}`, with clean fallback to field 0 / remaining fields).
  - Converts HTML → clean Markdown (strips `<style>`/`<script>`, maps headings, lists, bold/italic, code/pre, links, entities, math `\(...\)`/`\[...\]`/`[latex]` → `$...$`/`$$...$$`, `<img src="...">` → `![](asset:<hash>)`, `[sound:...]` → `[Audio: ...](asset:<hash>)`).
  - Reconstructs FSRS-7 scheduling state by replaying imported `revlog` entries through `migrate_card_scheduling_to_fsrs7`, preserving `due` dates, `lapses`, `reps`, `suspended`, and `tags` (falling back to Anki FSRS `data` JSON `s`/`d` or `ivl`/`factor` when `revlog` is absent).
- **Public API across surfaces:**
  - `rem-core`: `Collection::preview_apkg_bytes` / `Collection::import_apkg_bytes` returning `ApkgImportPreview` / `ApkgImportResult` (added/replaced decks, imported cards, review logs, media assets, warnings).
  - **CLI:** `rem import <file.apkg> [--dry-run] [--output text|json]`.
  - **Desktop (`SettingsPage.tsx`):** Dedicated "Import Anki deck (.apkg)" button alongside JSON backup import, with replace-confirmation dialog and import summary.
  - **iOS (`SettingsView.swift`):** "Import Anki deck (.apkg)" via `.fileImporter` with replace-confirmation dialog and summary toast.

---

## 5. Show Answer Timer

- Expose `show_timer: bool` on `StudyView` (`rem-core`), `StudyViewDto` (`rem-ffi`), and TypeScript `StudyView`, derived from the current card's deck `DeckSettings.show_timer`.
- **Desktop (`ReviewPage.tsx`):** Quiet monospace elapsed timer (`0:12`) shown when `view.showTimer` is true; starts on card load, pauses when revealed, resets on next card.
- **iOS (`ReviewScreen.swift`):** Quiet monospace elapsed timer in the question eyebrow when `viewState.showTimer` is true; pauses on reveal, resets on next card.
- **TUI (`rem study`):** Displays elapsed time in the header when `view.show_timer` is true.
