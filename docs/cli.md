# rem CLI

The `rem` command uses the same local SQLite collection as the desktop app. Commands work while the
app is closed; when Cloud sync is enabled, local mutations are captured atomically in the outbox and
can be synced with `rem sync` or automatically by the desktop app.

Agents should propose drafts for human approval. Direct card creation remains available for humans
and explicitly trusted automation.

## Decks

List decks before selecting a target:

```sh
rem deck list
rem deck list --output json
```

A deck ID takes precedence over name lookup. Names are case-sensitive and must match exactly. If
multiple decks have the same name, use an ID reported by `rem deck list`.

## Terminal study

Study due cards across the collection or in one deck:

```sh
rem study
rem study --deck <id-or-exact-name>
```

`rem study` requires an interactive terminal and never emits machine JSON. It uses the same Rust
`StudySession` as desktop review, so selection, daily caps, learning and relearning steps, FSRS
intervals, leech handling, review logs, and stale-card protection remain identical.

| Key | Action |
| --- | --- |
| <kbd>Space</kbd> / <kbd>Enter</kbd> | Reveal the answer. |
| <kbd>1</kbd> / <kbd>2</kbd> / <kbd>3</kbd> / <kbd>4</kbd> | Grade Again / Hard / Good / Easy after reveal. |
| <kbd>u</kbd> / <kbd>Ctrl+Z</kbd> | Undo the last grade atomically. |
| <kbd>s</kbd> | Suspend the current card. |
| <kbd>↑</kbd> / <kbd>↓</kbd> / <kbd>j</kbd> / <kbd>k</kbd> | Scroll long cards. |
| <kbd>q</kbd> / <kbd>Ctrl+C</kbd> | End the session cleanly. |

The header shows deck scope and progress, and grade choices show their next intervals. Markdown
headings, lists, emphasis, links, inline code, fenced code, and cloze deletions (`{{cN::...}}`) use
terminal styles. Images, GIFs, audio clips, and Image Occlusion blocks display explicit placeholders
rather than raw asset/JSON markup. Studying a parent deck (`Parent`) automatically includes all
nested subdecks (`Parent::Child`). If another desktop or terminal session changes the current card
first, the stale grade is skipped and explained instead of being recorded twice.

### Frozen scope

Desktop and iOS are the primary full-fidelity study surfaces. `rem study` is intentionally frozen to
lightweight terminal review of due text/Markdown cards (`--deck`, reveal, 1–4 grading, undo,
suspend, scroll, provenance, and clean exit). Rich media rendering/audio playback, interactive Image
Occlusion, LaTeX math typesetting (shown as raw `$...$` text in the terminal), custom study modes,
answer timers, mid-session card editing, and deck options are intentionally Desktop/iOS-only.

## Draft proposals

Add one pending draft with literal Markdown:

```sh
rem draft add --deck <id-or-exact-name> \
  --front 'Why does Rust ownership prevent use-after-free?' \
  --back 'A value has one owner, and dropping that owner ends the value lifetime.' \
  --tag rust --tag ownership \
  --rationale 'This is a reusable safety invariant.' \
  --source 'src/ownership.rs:10-25' \
  --producer pi
```

For multiline Markdown, use UTF-8 files:

```sh
rem draft add --deck <deck-id> \
  --front-file question.md --back-file answer.md \
  --source 'https://doc.rust-lang.org/book/ch04-01-what-is-ownership.html'
```

List pending drafts:

```sh
rem draft list
rem draft list --output json
```

Drafts have no scheduling state or due date. They do not affect study counts, stats, backup, or cloud
sync. Review pending proposals in the desktop sidebar (**Drafts**) or interactively in the terminal
with `rem draft review` to try the front, reveal the proposal, edit its content or target deck, and
accept or reject it. Creating or rejecting a draft does not change the synchronized collection
revision; acceptance creates a normal due card.

### Terminal draft review

Triage pending drafts front-first in the terminal across all decks or for a single deck:

```sh
rem draft review
rem draft review --deck <id-or-exact-name>
```

`rem draft review` requires an interactive terminal (exiting with code `3` otherwise) and never emits
machine JSON. Each draft shows its tags and question (`front`) first so you can test recall before
revealing the proposed `back` and provenance (`rationale`, `sources`, and `producer`).

| Key | Action |
| --- | --- |
| <kbd>Space</kbd> / <kbd>Enter</kbd> | Reveal the proposed answer and provenance. |
| <kbd>a</kbd> | Accept the revealed draft into due cards (preserving provenance; clears duplicate drafts when an identical card already exists). |
| <kbd>r</kbd> | Reject (discard) the current draft. |
| <kbd>e</kbd> | Edit the current draft in `$VISUAL` / `$EDITOR` (falls back to `vi`). |
| <kbd>n</kbd> / <kbd>Tab</kbd> | Skip the current draft for now (moves it to the end of the queue when more than one draft remains, preserving in-memory edits). |
| <kbd>↑</kbd> / <kbd>↓</kbd> / <kbd>j</kbd> / <kbd>k</kbd> | Scroll long drafts. |
| <kbd>q</kbd> / <kbd>Ctrl+C</kbd> | Exit and print a triage summary (`accepted`, `existing`, `rejected`, `remaining`). |

Pressing <kbd>e</kbd> suspends the TUI and opens a temporary `.md` buffer in `$VISUAL` (or `$EDITOR`,
falling back to `vi`; commands with arguments such as `code --wait` are supported):

```markdown
deck: Rust
tags: rust, ownership

--- FRONT ---
Why does Rust ownership prevent use-after-free?

--- BACK ---
A value has one owner, and dropping that owner ends the value lifetime.
```

Change `deck:` to another deck's exact unique name or ID to move the draft, edit `tags:` as a
comma-separated list, and save/close your editor to return to the TUI with the updated draft revealed
and ready to accept (<kbd>a</kbd>). Clearing the `--- FRONT ---` section cancels the edit. Draft
resolutions use optimistic concurrency (`revision`), so any draft already modified or resolved
elsewhere (for example in the desktop app) is skipped cleanly.

### Draft JSON input

`--input-json` accepts one object or an array from a file. Use `-` for stdin:

```sh
rem draft add --deck <deck-id> --producer pi \
  --input-json - --output json <<'JSON'
[
  {
    "front": "What does Rust's `move` keyword do?",
    "back": "It transfers ownership of captured values.",
    "tags": ["rust", "ownership"],
    "rationale": "This distinction commonly explains ownership errors.",
    "sources": [
      {
        "locator": "https://doc.rust-lang.org/std/keyword.move.html",
        "label": "Rust reference"
      }
    ]
  }
]
JSON
```

| Field | Type | Required | Notes |
| --- | --- | --- | --- |
| `front` | string | yes | Must contain non-whitespace; Markdown is preserved exactly. |
| `back` | string | no | Defaults to an empty string. |
| `tags` | string[] | no | Defaults to `[]`; trimmed and deduplicated case-insensitively. System tags such as `leech` are ignored. |
| `rationale` | string or null | no | Why the proposal is worth remembering. Blank values become null. |
| `sources` | object[] | no | Defaults to `[]`. Each source requires a non-blank `locator` and may have a `label`. |

`--producer` applies to the complete batch. Unknown fields are rejected. A batch is one transaction:
if any draft is invalid, none are created.

### Draft duplicates and dry runs

Draft identity is normalized deck/front/back/tags content. Rationale, sources, and producer do not
change identity. Exact matches produce:

- `duplicateDraft` when the proposal is already pending;
- `duplicateCard` when a normal card already has that content.

Normal-card duplicates take precedence. `--dry-run` validates and deduplicates without writing:

```sh
rem draft add --deck <deck-id> --input-json drafts.json --dry-run --output json
```

A new preview has status `wouldCreate`. Duplicate outcomes are successful no-ops, making ordinary
agent retries safe.

## Direct card creation

Use this path when immediate scheduling is intentional:

```sh
rem card add --deck <id-or-exact-name> \
  --front 'What does Rust ownership guarantee?' \
  --back 'Every value has one owner at a time.' \
  --tag rust --tag ownership
```

For multiline Markdown:

```sh
rem card add --deck <deck-id> \
  --front-file question.md --back-file answer.md \
  --tag rust
```

Card JSON accepts `front`, `back`, and `tags` with the same defaults and normalization as draft
input. Unknown fields are rejected. Exact duplicates have status `duplicate`; use
`--allow-duplicate` only when a second scheduled copy is intentional. Card batches are atomic and
support `--dry-run`.

Directly created cards are due immediately and reach the sync server on the next `rem sync` or desktop sync.

## Searching cards

Search scheduled cards across the collection or within a single deck:

```sh
rem card search 'ownership'
rem card search 'ownership' --deck <id-or-exact-name>
rem card search 'Pin<P>' --limit 25 --output json
```

| Flag | Default | Notes |
| --- | --- | --- |
| `<query>` | required | Matches card `front`, `back`, `tags`, and `rationale`. Blank queries return no hits. |
| `--deck <id-or-exact-name>` | all decks | Restrict search to one deck by ID or exact unique name. |
| `--limit <1-200>` | `50` | Maximum number of hits to return (`1`–`200`). |
| `--output <text\|json>` | `text` | Text mode prints tab-separated `<card-id>\t<deck-name>\t<first-line-of-front>` (or `No matching cards.`). |

`rem card search` queries the SQLite FTS5 index (`cards_fts`) with prefix matching per alphanumeric
term, ranked by BM25 and then `updated_at DESC`. When a query contains code punctuation or operators
(such as `Pin<P>`, `->`, or `::`)—or when FTS5 returns no hits—it automatically falls back to a
case-insensitive substring (`LIKE`) search across `front`, `back`, `tags`, and `rationale` ordered
by `updated_at DESC`.

## Importing Anki decks (`.apkg`)

Import Anki deck packages (`.apkg` — legacy `anki2`, `anki21`, and modern zstd-compressed `anki21b`
formats) directly into your local SQLite collection:

```sh
# Preview decks, card counts, media, and review logs without writing
rem import biology.apkg --dry-run
rem import biology.apkg --dry-run --output json

# Import decks, cloze/reversed siblings, math, images/audio, and FSRS-7 review history
rem import biology.apkg
rem import biology.apkg --output json
```

The importer converts Anki HTML to clean Markdown, strips `<script>` and `<style>` blocks, preserves
nested `Parent::Child` deck hierarchies, maps cloze, Anki 23.10+ built-in Image Occlusion, Anki
Image Occlusion Enhanced, and multi-template notes into `rem` sibling groups (respecting
`bury_related`), converts `<img>` and `[sound:...]` references to content-addressed `asset:<hash>`
blobs, normalizes `\(...\)` / `\[...\]` math delimiters, and replays `revlog` history into FSRS-7
scheduling state. Existing decks with the exact same name are replaced.

*(In `rem study`, Image Occlusion cards display a concise placeholder and a note directing you to
the desktop or iPhone app rather than raw JSON.)*

## Cloud sync

Sign in to hosted `rem Cloud` (default) or a self-hosted `rem-sync-server` using browser OAuth
(Google / Apple) or a personal/dev bearer token, then sync from the terminal:

```sh
rem sync status
rem sync login --provider google
rem sync login --server https://sync.example.com --provider google
rem sync login --server https://sync.example.com --token <personal-token>
rem sync
rem sync logout
```

Credentials are stored in the OS Keychain and shared with the desktop app (so signing in once works
for both). For headless scripts or CI, `REM_SYNC_TOKEN` can be set in the environment when running
`rem sync`. Passing `--reset-binding` to `rem sync login` explicitly unlinks a previous account
binding if switching accounts (local cards are never deleted).

## Machine output

`--output json` writes exactly one versioned JSON object to stdout. Human diagnostics go to stderr.

Deck list success:

```json
{
  "version": 1,
  "command": "deck.list",
  "data": {
    "decks": [{ "id": "...", "name": "Rust" }]
  }
}
```

Draft-add success:

```json
{
  "version": 1,
  "command": "draft.add",
  "data": {
    "deck": { "id": "...", "name": "Rust" },
    "dryRun": false,
    "drafts": [
      { "id": "...", "status": "created" },
      { "id": "...", "status": "duplicateDraft" },
      { "id": "...", "status": "duplicateCard" }
    ]
  }
}
```

Draft-list success returns full pending draft values under `data.drafts`, including target deck,
Markdown, tags, rationale, sources, producer, timestamps, and revision.

Card-add success:

```json
{
  "version": 1,
  "command": "card.add",
  "data": {
    "deck": { "id": "...", "name": "Rust" },
    "dryRun": false,
    "cards": [
      { "id": "...", "status": "created" },
      { "id": "...", "status": "duplicate" }
    ]
  }
}
```

Results follow input order. Dry-run inputs that would be inserted have status `wouldCreate` and no
`id`.

Card-search success:

```json
{
  "version": 1,
  "command": "card.search",
  "data": {
    "hits": [
      {
        "card": {
          "id": "...",
          "deckId": "...",
          "front": "Why does Rust ownership prevent use-after-free?",
          "back": "A value has one owner, and dropping that owner ends the value lifetime.",
          "createdAt": 1700000000000,
          "updatedAt": 1700000000000,
          "tags": ["rust", "ownership"],
          "suspended": false,
          "lastAgainAt": null,
          "scheduling": {
            "kind": "fsrs",
            "stability": 0.0,
            "difficulty": 0.0,
            "reps": 0,
            "lapses": 0,
            "state": 0,
            "step": 0,
            "lastReview": null,
            "due": 1700000000000
          }
        },
        "deckName": "Rust",
        "deckColor": "#4f6d7a"
      }
    ]
  }
}
```

Each hit's `card` object also includes optional provenance fields (`rationale`, `sources`,
`proposedBy`), sibling fields (`siblingGroupId`, `clozeIndex`), and `scheduling.stabilityFast` when
present.

Anki `.apkg` import success (`--dry-run` omits writes and returns preview keys `totalCards`,
`mediaCount`, and `reviewLogs` instead of `importedCards`, `importedReviews`, and `importedMedia`):

```json
{
  "version": 1,
  "command": "import",
  "data": {
    "dryRun": false,
    "added": ["Languages::Spanish"],
    "replaced": [],
    "importedCards": 120,
    "importedReviews": 480,
    "importedMedia": 64,
    "warnings": []
  }
}
```

Error:

```json
{
  "version": 1,
  "command": "draft.add",
  "error": {
    "code": "deck_not_found",
    "message": "deck not found: Missing",
    "candidates": []
  }
}
```

Stable exit codes:

| Code | Meaning |
| --- | --- |
| `0` | Success, including duplicate-only requests. |
| `2` | Invalid command arguments. |
| `3` | Invalid or unreadable input, including starting study or draft review without an interactive terminal. |
| `4` | Missing or ambiguous deck reference. |
| `5` | Collection/storage failure. |
| `6` | Cloud sync authentication, configuration, or network failure. |

## Installation notes

The release installer places the CLI at `~/.local/bin/rem` on macOS and Linux. On Linux the desktop
AppImage is a separate command, `rem-app`. Windows releases include a
`rem-cli-x86_64-pc-windows-msvc.zip`; extract `rem.exe` into a directory on `PATH`.

For isolated development and tests, `REM_DATABASE_PATH` can point the command at an explicit SQLite
file. Normal use should leave it unset so CLI and desktop share the standard collection.

The distributable `skills/rem-card-capture/SKILL.md` teaches agents the draft-first workflow
without duplicating persistence logic. Install it for supported local agents with
`npx skills add shettyh/rem --skill rem-card-capture --global`.
