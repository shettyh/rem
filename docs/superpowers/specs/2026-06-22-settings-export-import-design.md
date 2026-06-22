# Settings surface + deck export/import — design

_Date: 2026-06-22_

## Goal

Add JSON **export/import of decks** as backup insurance for browser-only data
(ROADMAP near-term item 4), housed in a new, reusable **Settings** surface that
future settings (FSRS config, AI keys, etc.) can extend.

## Decisions (from brainstorming)

- **Export is deck-scoped with selection** — the user picks which decks to
  export, including a "Select all".
- **Import replaces by name, with a warning** — if an incoming deck has the same
  **name** as an existing deck, the existing one is replaced; the user is warned
  (shown which decks) and confirms before any data changes.
- **Settings is a dedicated `/settings` page** reached from a header ⚙ link
  (not a dropdown), matching the app's route-based structure and giving future
  settings a home. The theme toggle stays in the header.

## Backup file format

Downloaded as `rem-backup-YYYY-MM-DD.json`, MIME `application/json`:

```json
{
  "format": "rem-backup",
  "version": 1,
  "exportedAt": 1719000000000,
  "decks": [
    {
      "name": "Spanish",
      "createdAt": 123,
      "cards": [
        { "front": "…", "back": "…", "createdAt": 1, "updatedAt": 2, "scheduling": { "repetitions": 0, "intervalDays": 0, "easeFactor": 2.5, "due": 3 } }
      ]
    }
  ]
}
```

- **Full fidelity**: scheduling / review progress is preserved on round-trip.
- **IDs are omitted** from the file. Since we match by name, deck/card IDs are
  not meaningful across imports; dropping them keeps the file clean and avoids
  implying IDs carry across. Fresh IDs are generated on import.

## Flows

### Export
1. SettingsPage lists decks with checkboxes + a "Select all" control.
2. "Export selected" assembles the payload via `collectBackup(storage, deckIds)`
   and triggers a Blob download. The button is disabled when nothing is selected.

### Import (replace-by-name, with warning)
1. User picks a file → read text → `parseBackup(text)` validates format / version
   / shape (throws on bad input → inline error, **no data touched**).
2. Compare incoming deck names against existing names (`listDecks`).
3. If any already exist, show a **warning** listing exactly which decks will be
   replaced, with **Confirm / Cancel**. (No collisions → import directly.)
4. On confirm, `storage.importDecks(decks)` runs transactionally:
   - Up front, collect the set of incoming deck names. Delete every existing
     deck whose name is in that set (and its cards). Deleting matches up front
     (rather than per-incoming-deck) makes the result deterministic and avoids
     re-deleting decks we just inserted.
   - Insert all incoming decks + cards with **fresh IDs** and preserved
     content/scheduling. A file that itself contains two decks with the same
     name is inserted as-is (both kept) — we don't dedupe the user's file.
   - `replaced` = names that existed before and were deleted; `added` = incoming
     deck names that were not replacements. (Names are de-duplicated for the
     count, so "replaced M" counts distinct names.)
5. Report the result: "Imported N decks (replaced M)".

> Note: deck names are not unique in the data model. "Replace by name" therefore
> means *all* existing decks sharing an incoming name are removed before insert.

## Code boundaries

Preserves the existing seam discipline (`Storage` / `Scheduler` interfaces).

- **`src/data/backup.ts`** — pure, DB-agnostic:
  - Types: `CardBackup`, `DeckBackup`, `BackupFile`.
  - `collectBackup(storage, deckIds): Promise<DeckBackup[]>` — composes existing
    `listDecks` / `listCards` reads.
  - `serializeBackup(decks: DeckBackup[], exportedAt: number): string`.
  - `parseBackup(text: string): DeckBackup[]` — validates `format === 'rem-backup'`,
    `version === 1`, and deck/card shape; throws a clear error on bad input.
- **One new `Storage` method**: `importDecks(decks: DeckBackup[]): Promise<ImportResult>`
  where `ImportResult = { added: string[]; replaced: string[] }` (deck names).
  Lives in the adapter because it must be atomic and preserve scheduling/timestamps
  (the existing `createCard` recomputes them). Implemented in `DexieStorage` with a
  single `rw` transaction over `decks` + `cards`. **Export needs no new method.**
- **`src/features/settings/SettingsPage.tsx`** — the UI (Export + Import sections).
- Wiring: `/settings` route in `routes.tsx`; ⚙ link in `Layout.tsx`.

## Error handling

- Invalid JSON / wrong `format` / wrong `version` / malformed shape → inline
  error message; no mutation.
- Empty export selection → "Export selected" disabled.
- File read failure → error message.

## Testing

- **`backup.test.ts`** (unit, no DB):
  - `serializeBackup` emits correct `format` / `version` / `exportedAt` / `decks`.
  - `parseBackup` accepts valid input and rejects: not JSON, missing `format`,
    wrong `version`, malformed deck, malformed card.
  - Round-trip: `collectBackup` → `serializeBackup` → `parseBackup` preserves data.
- **`DexieStorage.test.ts`** additions for `importDecks`:
  - Adds brand-new decks (with their cards).
  - Replaces a same-name deck — old cards are gone, new ones present.
  - Regenerates IDs (incoming IDs not assumed; new decks/cards get fresh IDs).
  - Preserves `scheduling` and `createdAt` / `updatedAt`.
  - Returns correct `{ added, replaced }`.
- **Browser test**: add Settings to the existing screen sweep, plus one
  interaction test for the import-with-warning happy path.

## Out of scope (YAGNI)

- Merge-mode import (keep both); cross-device ID matching.
- Empty/placeholder Settings sections for future features.
- Export of anything other than decks + cards (no app-level preferences yet).
