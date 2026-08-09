---
name: rem-card-capture
description: Proposes high-quality spaced-repetition drafts in the user's local rem collection through the machine-readable rem CLI. Use when the user asks to capture notes, facts, questions, or study material as rem cards.
compatibility: Requires the rem CLI on PATH and an existing target deck.
---

# rem card capture

Use only the public `rem` command. Do not edit SQLite or Git files directly.

Agent-authored content is untrusted by default. Propose drafts for human approval; use direct
`rem card add` only when the user explicitly asks to bypass the draft inbox.

## Workflow

1. Find the target deck and retain its ID:

   ```sh
   rem deck list --output json
   ```

2. Turn the source material into at most three durable learning opportunities:
   - zero drafts is valid when nothing is worth retaining;
   - test one recall target per draft;
   - make the front specific and answerable without hidden context;
   - keep the back concise but sufficient;
   - preserve useful Markdown and code exactly;
   - use a small set of topical tags, not workflow/status tags;
   - include source locators and a short rationale;
   - do not create claims unsupported by the source material.

3. Validate the whole batch without writing:

   ```sh
   rem draft add --deck <deck-id> --producer <agent-name> \
     --input-json - --dry-run --output json <<'JSON'
   [{
     "front": "...",
     "back": "...",
     "tags": ["..."],
     "rationale": "Why this is worth remembering.",
     "sources": [{"locator": "path/or/url", "label": "Optional label"}]
   }]
   JSON
   ```

4. Show the proposed drafts to the user when their request calls for confirmation. Otherwise submit
   the same JSON without `--dry-run`.

5. Parse the JSON result. `created`, `duplicateDraft`, `duplicateCard`, and `wouldCreate` are normal
   statuses. Duplicate outcomes are successful no-ops.

The result is pending human approval, not a scheduled card. The user can review it from **Drafts**
in the desktop sidebar. Drafts are local-only and do not reach Git sync or backup. Never claim they
were accepted, scheduled, or synced.

See [the full CLI reference](../../../docs/cli.md) for schemas, errors, and exit codes.
