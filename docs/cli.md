# rem CLI

The `rem` command uses the same local SQLite collection as the desktop app. Commands work while the
app is closed and do not start Git sync.

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
| <kbd>↑</kbd> / <kbd>↓</kbd> / <kbd>j</kbd> / <kbd>k</kbd> | Scroll long cards. |
| <kbd>q</kbd> | End the session cleanly. |

The header shows deck scope and progress, and grade choices show their next intervals. Markdown
headings, lists, emphasis, links, inline code, and fenced code use terminal styles. Images and GIFs
are represented by an explicit placeholder containing the asset hash. If another desktop or
terminal session changes the current card first, the stale grade is skipped and explained instead
of being recorded twice.

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

Drafts have no scheduling state or due date. They do not affect study counts, stats, backup, or Git
sync. Open **Drafts** in the desktop sidebar to try the front, reveal the proposal, edit its content
or target deck, and accept or reject it. Creating or rejecting a draft does not change the
synchronized collection revision; acceptance creates a normal due card.

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

Directly created cards are due immediately and reach Git on the desktop app's next sync.

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
| `3` | Invalid or unreadable input, including starting study without an interactive terminal. |
| `4` | Missing or ambiguous deck reference. |
| `5` | Collection/storage failure. |

## Installation notes

The release installer places the CLI at `~/.local/bin/rem` on macOS and Linux. On Linux the desktop
AppImage is a separate command, `rem-app`. Windows releases include a
`rem-cli-x86_64-pc-windows-msvc.zip`; extract `rem.exe` into a directory on `PATH`.

For isolated development and tests, `REM_DATABASE_PATH` can point the command at an explicit SQLite
file. Normal use should leave it unset so CLI and desktop share the standard collection.

The distributable `skills/rem-card-capture/SKILL.md` teaches agents the draft-first workflow
without duplicating persistence logic. Install it for supported local agents with
`npx skills add shettyh/rem --skill rem-card-capture --global`.
