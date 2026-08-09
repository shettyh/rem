---
name: rem-card-capture
description: Creates high-quality spaced-repetition cards in the user's local rem collection through the machine-readable rem CLI. Use when the user asks to capture notes, facts, questions, or study material as rem cards.
compatibility: Requires the rem CLI on PATH and an existing target deck.
---

# rem card capture

Use only the public `rem` command. Do not edit SQLite or Git files directly.

## Workflow

1. Find the target deck and retain its ID:

   ```sh
   rem deck list --output json
   ```

2. Turn the source material into cards:
   - test one recall target per card;
   - make the front specific and answerable without hidden context;
   - keep the back concise but sufficient;
   - preserve useful Markdown and code exactly;
   - use a small set of topical tags, not workflow/status tags;
   - do not create claims unsupported by the source material.

3. Validate the whole batch without writing:

   ```sh
   rem card add --deck <deck-id> --input-json - --dry-run --output json <<'JSON'
   [{"front":"...","back":"...","tags":["..."]}]
   JSON
   ```

4. Show the proposed cards to the user when their request calls for confirmation. Otherwise submit the same JSON without `--dry-run`.

5. Parse the JSON result. `created`, `duplicate`, and `wouldCreate` are normal statuses. Treat `duplicate` as a successful no-op. Never use `--allow-duplicate` unless the user explicitly wants another copy.

Capture is local. Do not claim it has synced to Git; it reaches Git only after the desktop app's next sync.

See [the full CLI reference](../../../docs/cli.md) for schemas, errors, and exit codes.
