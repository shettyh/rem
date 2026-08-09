# rem CLI

The `rem` command captures cards directly into the same local SQLite collection used by the desktop app. It does not start Git sync; captured cards reach Git the next time sync runs in the desktop app.

## Commands

List decks before selecting a target:

```sh
rem deck list
rem deck list --output json
```

Add one card with literal Markdown:

```sh
rem card add --deck <id-or-exact-name> \
  --front 'What does Rust ownership guarantee?' \
  --back 'Every value has one owner at a time.' \
  --tag rust --tag ownership
```

For multiline Markdown, use UTF-8 files:

```sh
rem card add --deck <id-or-exact-name> \
  --front-file question.md --back-file answer.md \
  --tag rust
```

A deck ID takes precedence over name lookup. Names are case-sensitive and must match exactly. If multiple decks have the same name, use an ID reported by `rem deck list`.

## JSON input

`--input-json` accepts a single object or an array from a file. Use `-` to read stdin:

```sh
rem card add --deck <deck-id> --input-json - --output json <<'JSON'
[
  {
    "front": "What does Rust's `move` keyword do?",
    "back": "It transfers ownership of captured values.",
    "tags": ["rust", "ownership"]
  }
]
JSON
```

Each input object has this schema:

| Field | Type | Required | Notes |
| --- | --- | --- | --- |
| `front` | string | yes | Must contain non-whitespace; Markdown is preserved exactly. |
| `back` | string | no | Defaults to an empty string. |
| `tags` | string[] | no | Defaults to `[]`; values are trimmed and deduplicated case-insensitively. System-owned tags such as `leech` are ignored. |

Unknown fields are rejected. A JSON batch is one transaction: if any card is invalid, no cards are created.

## Duplicates and dry runs

By default, an exact deck/front/back/tags match is returned with status `duplicate` instead of creating another card. Use `--allow-duplicate` only when a second copy is intentional.

`--dry-run` resolves the deck and validates and deduplicates every input without writing:

```sh
rem card add --deck <deck-id> --input-json cards.json --dry-run --output json
```

## Machine output

`--output json` writes exactly one versioned JSON object to stdout. Human diagnostics are written to stderr.

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

Results follow input order. Dry-run inputs that would be inserted have status `wouldCreate` and no `id`.

Error:

```json
{
  "version": 1,
  "command": "card.add",
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
| `3` | Invalid or unreadable card input. |
| `4` | Missing or ambiguous deck reference. |
| `5` | Collection/storage failure. |

## Installation notes

The release installer places the CLI at `~/.local/bin/rem` on macOS and Linux. On Linux the desktop AppImage is a separate command, `rem-app`. Windows releases include a `rem-cli-x86_64-pc-windows-msvc.zip`; extract `rem.exe` into a directory on `PATH`.

For isolated development and tests, `REM_DATABASE_PATH` can point the command at an explicit SQLite file. Normal use should leave it unset so the CLI and desktop app share the standard collection.

This repository also includes the project-level agent skill at `.agents/skills/rem-card-capture/SKILL.md`. It teaches agents card-writing quality and the JSON CLI workflow without duplicating persistence logic.
