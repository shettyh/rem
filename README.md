# rem

**Local-first spaced repetition across desktop, terminal, and iPhone — with an AI agent skill and CLI for source-grounded card capture, FSRS-7 scheduling, Markdown cards, and optional Cloud sync.**

[![CI](https://github.com/shettyh/rem/actions/workflows/ci.yml/badge.svg)](https://github.com/shettyh/rem/actions/workflows/ci.yml)
[![Latest release](https://img.shields.io/github/v/release/shettyh/rem)](https://github.com/shettyh/rem/releases/latest)
[![License](https://img.shields.io/github/license/shettyh/rem?cacheSeconds=300)](LICENSE)
[![Tauri 2](https://img.shields.io/badge/Tauri-2-24C8DB?logo=tauri&logoColor=white)](https://tauri.app)

[Website & interactive tour](https://shettyh.github.io/rem/) · [Download](https://github.com/shettyh/rem/releases/latest) · [Agent skill](skills/rem-card-capture/SKILL.md) · [CLI reference](docs/cli.md) · [iOS companion](ios/README.md) · [rem Premium](https://rem-sync.shettyh.fyi/premium)

Use **rem** to turn real engineering work, debugging sessions, specs, or any topic into durable understanding. Pair the bundled `rem` CLI with the installable [`rem-card-capture`](skills/rem-card-capture/SKILL.md) skill so any local coding or research agent can propose a few crisp, source-grounded drafts right from the context it already has — you try, edit, and approve what enters your study plan, and accepted cards use the same Rust FSRS-7 scheduler whether you review in the desktop app, terminal (`rem study`), or native iPhone companion.

```text
work in your agent → skill + CLI propose ≤3 grounded drafts → you try, edit & approve → FSRS-7 schedules → study on desktop, terminal, or iPhone
```

- **Agent Skill + CLI with zero plumbing** — works with Claude Code, Codex, pi, Cursor, or any CLI-capable agent over local SQLite. No MCP server, no API key lock-in, and works even while the desktop app is closed.
- **Human-approved by default** — agent proposals land in a local-only **Drafts** inbox with rationale and source paths/URLs; nothing is scheduled, backed up, or synced until you accept it.
- **One Rust FSRS-7 engine** — desktop, terminal (`rem study`), and iPhone all share the same Rust `StudySession`, Anki/FSRS-aligned steps, daily limits, optimistic concurrency, and history.
- **Local by default, sync when you want** — SQLite on your device needs no account, stays exportable to JSON, and optionally syncs via hosted [rem Premium](https://rem-sync.shettyh.fyi/premium) or your own self-hosted Cloudflare Worker.
- **See it in action** — explore the interactive Desktop, iPhone, Terminal, and Agent showcase at **[shettyh.github.io/rem](https://shettyh.github.io/rem/)**.

> [!IMPORTANT]
> rem ships as a native [Tauri](https://tauri.app) desktop app for macOS, Linux, and Windows, a bundled `rem` CLI, and a native SwiftUI iOS companion (`ios/`). The browser build is only the source for Tauri's webview and is not a supported version of the app.

## Working with AI agents (Skill + CLI)

Most spaced-repetition workflows break down at capture time: copying context into a separate flashcard app interrupts flow, while unguided LLMs generate bloated cards you dread reviewing.

Combining the **`rem` CLI** with the **`rem-card-capture` skill** turns any CLI-capable agent (Claude Code, Codex, pi, Cursor, Amp, or your own scripts) into a disciplined study partner using the codebase, diff, docs, or paper already in its context.

### 1. Install the CLI and agent skill

```sh
# macOS (installs rem.app + links `rem` CLI onto PATH)
brew install --cask shettyh/tap/rem

# Teach your local agents the capture workflow & card-writing craft
npx skills add shettyh/rem --skill rem-card-capture --global
```

*(On Linux or Windows, see [Installation](#installation) below. Omit `--global` to install the skill only in the current project.)*

### 2. Capture naturally from real work

Because the agent already sees your files, terminal output, and conversation, you can ask in plain language:

- **After fixing a tricky bug:** *"Capture the locking invariant we just debugged into my `Rust internals` deck."*
- **While reading a repo or RFC:** *"Distill the key design trade-offs from this spec into rem drafts, skipping anything already covered."*
- **Opted-in ambient capture:** Tell your agent (or project instructions) to propose up to 3 rem drafts after substantial PRs or deep dives — the skill explicitly forbids unrequested ambient capture unless you opt in.

### 3. What the skill + CLI enforce behind the scenes

| Layer | What it guarantees |
| --- | --- |
| **Card-writing craft** ([`SKILL.md`](skills/rem-card-capture/SKILL.md)) | Caps batches at **≤3 durable cards** (zero is valid and preferred over filler). Applies the **3-month / 5-second test** (self-contained fronts with no *"we"* or *"this PR"*, crisp answer-first backs), uses **mechanism / contrast / diagnosis / command** prompts instead of vague essays, yes/no coin flips, or list dumps, and formats plain Markdown that renders cleanly on desktop, iPhone, and `rem study`. |
| **Provenance & rationale** | Attaches `rationale` (*why this matters*, kept out of the answer) and exact `sources` (file paths with line ranges, commits, or URLs) so every proposal is auditable in **Drafts**. |
| **Machine-safe CLI contract** ([`docs/cli.md`](docs/cli.md)) | Lists decks (`rem deck list --output json`), inspects pending drafts (`rem draft list --output json`) to avoid near-duplicates, validates the entire atomic batch with `--dry-run --output json` first, handles exact duplicates as retry-safe exit-0 no-ops (`duplicateCard` / `duplicateDraft`), and checks `version: 1` JSON responses. |
| **Human authority** | Agent-authored drafts stay strictly local (no scheduling state, no stats impact, excluded from backup and Cloud sync) until you try the front, edit if needed, and click **Accept** in the desktop **Drafts** inbox. |

### 4. Direct CLI scripting, terminal review, and sync

The same `rem` binary works while the desktop app is closed — for agent drafts, direct trusted scripts, terminal study, and cloud sync:

```sh
# Inspect decks & pending drafts (human-readable or --output json)
rem deck list --output json
rem draft list --output json

# Validate & submit source-grounded agent drafts atomically
rem draft add --deck <deck-id> --producer claude-code \
  --input-json - --dry-run --output json <<'JSON'
[{
  "front": "Why don't pending rem drafts sync to Cloud or appear in backups?",
  "back": "Drafts are unapproved local operational state without scheduling data; a card enters sync only after human acceptance.",
  "tags": ["rem", "sync"],
  "rationale": "Clarifies the trust boundary between agent proposals and synced study data.",
  "sources": [{ "locator": "docs/cli.md#L77-L80", "label": "CLI reference" }]
}]
JSON

# Directly add scheduled cards when you explicitly want to bypass Drafts
rem card add --deck <deck-id> --front 'Question' --back 'Answer' --tag topic

# Study in your terminal with the shared Rust FSRS-7 StudySession
rem study
rem study --deck <id-or-exact-name>

# Import existing Anki decks (.apkg) with media, cloze, math, and review history
rem import deck.apkg --dry-run
rem import deck.apkg

# Sync incrementally to desktop & iPhone (OS Keychain or REM_SYNC_TOKEN)
rem sync status
rem sync
```

See the full [`rem-card-capture` skill](skills/rem-card-capture/SKILL.md) and [CLI reference](docs/cli.md) for JSON schemas, error codes, and stable exit codes.

## Features

- **Local-first by default** — decks, cards, media assets, and review history live on your device in SQLite (WAL mode).
- **FSRS-7 in Rust** — modern scheduling in `rem-core` with Anki/FSRS-aligned learning steps and Hard-step behavior, configurable retention, daily limits, sibling burying (`bury_related`), optional answer timer, leech handling, and per-deck parameter optimization across desktop and iOS.
- **Anki `.apkg` import & migration** — pure-Rust importer (`anki2`, `anki21`, and zstd `anki21b`) on desktop, CLI (`rem import`), and iOS that converts HTML to clean Markdown while preserving nested `::` decks, cloze/reversed and Image Occlusion siblings, LaTeX math, image & audio assets, and FSRS-7 review history.
- **Agent Skill + CLI capture** — installable [`rem-card-capture`](skills/rem-card-capture/SKILL.md) skill and versioned JSON CLI for atomic dry-run draft batches, retry-safe deduplication, and front-first approval in the desktop **Drafts** inbox or `rem draft review` TUI.
- **Rich Markdown, Cloze, Image Occlusion, LaTeX & Audio** — WYSIWYG TipTap desktop editor, custom-SVG Image Occlusion canvas (rectangles, ellipses, grouping, Hide All / Hide One), and paper-canvas iOS editor supporting `{{c1::cloze::hint}}` deletions, LaTeX math (KaTeX on desktop, native SwiftUI math on iOS), syntax-highlighted code, offline images/GIFs, inline audio clips, nested `Parent::Child` decks, and tags.
- **Shared study engine across every surface** — study with full rich-media fidelity on desktop and iPhone (keyboard shortcuts or haptic touch grading, live interval previews, optimistic concurrency via `local_revision`, mid-session editing, in-review suspend, and atomic grade undo), or do quick text/Markdown review in the lightweight, scope-frozen terminal companion (`rem study`).
- **Native iOS companion** — SwiftUI iPhone app (`ios/`) backed by `rem-core` and `rem-sync` via UniFFI, featuring native `TabView`/`Form`/`List`, collapsible nested deck trees, top-right Quick Capture with Photos & cloze button, native math & audio playback, pinch-to-zoom image lightbox, full deck editing & per-deck FSRS-7 options, 7D/30D/90D Swift Charts stats, native Sign in with Apple, local daily reminders, and JSON / `.apkg` import.
- **Actionable statistics** — review activity, recall rate, streaks, grade distribution, and per-deck breakdowns across desktop and iOS.
- **Optional Cloud sync** — incremental HLC-based sync for decks, reviews, and media across desktop, CLI, and iPhone using hosted [rem Premium](https://rem-sync.shettyh.fyi/premium) (30-day no-card free trial, $5/year on desktop/web or $4.99/year on iOS) or your own free self-hosted Apache-2.0 [Cloudflare Worker](sync-server/README.md).
- **Portable data** — import and export full-fidelity JSON backups preserving scheduling state and review history.
- **Calm Quiet Paper design** — light and dark themes, collapsible icon-rail sidebar with deck progress rings on desktop, and distraction-free review.

## Installation

### macOS (Homebrew — recommended)

```sh
brew install --cask shettyh/tap/rem
```

This single installation installs `/Applications/rem.app` (signed & notarized) and links the bundled `rem` CLI onto your `PATH`. In-app updates keep both the desktop app and the `rem` CLI in sync automatically.

### macOS and Linux installer

```sh
curl -fsSL https://raw.githubusercontent.com/shettyh/rem/main/install.sh | sh
```

On macOS, this downloads a single `.dmg`, installs `/Applications/rem.app`, and symlinks the bundled `rem` CLI into `~/.local/bin/rem`. If you drag `rem.app` from a `.dmg` manually, you can link the CLI in one click from **Settings → App & updates**.

On Linux, this installs the desktop AppImage as `rem-app` and the CLI as `rem`. The AppImage requires FUSE (`sudo apt install libfuse2` on Debian/Ubuntu).

### Direct downloads

Download installers for any platform from [GitHub Releases](https://github.com/shettyh/rem/releases/latest):

| Platform | Desktop packages (includes bundled `rem` CLI) | Standalone CLI archives |
| --- | --- | --- |
| macOS | Apple Silicon and Intel DMG (signed & notarized) | Apple Silicon and Intel `.tar.gz` |
| Linux | AppImage, `.deb`, and `.rpm` | x86_64 `.tar.gz` |
| Windows | `.msi` and `.exe` | x86_64 `.zip` |

Windows releases also include a standalone CLI zip if you want `rem.exe` directly on `PATH`.

### Automatic updates

Desktop builds automatically check GitHub Releases (`latest.json`) for signed updates, download them in the background, and prompt you in the sidebar and **Settings → App & updates** when ready to restart. You can toggle **Automatic updates** off in **Settings → App & updates** to check and install updates manually instead. Because the `rem` CLI is bundled inside the app package, updating `rem` updates both the desktop app and linked CLI together.

### Agent skill

After installing the `rem` CLI, install the `rem-card-capture` skill for your local AI agents:

```sh
npx skills add shettyh/rem --skill rem-card-capture --global
```

The [Skills CLI](https://github.com/vercel-labs/skills) detects supported agents such as Claude Code, Codex, and pi and installs the skill in their user-level directories. Omit `--global` to install it only in the current project.

### iOS companion

The native SwiftUI iPhone app lives in [`ios/`](ios/README.md) (iOS 17+, Xcode 16+). iOS builds and Simulator unit tests run in CI, and versioned releases automatically archive and upload to TestFlight via App Store Connect cloud signing (see [`ios/README.md`](ios/README.md) for local builds and manual TestFlight dispatch).

### Code signing

macOS desktop releases (`.dmg`) and CLI archives are signed with an Apple Developer ID certificate and notarized with Apple. Windows installers and CLI binaries are currently unsigned (SmartScreen may prompt on first run).

## Getting started

1. Create a deck from the **Today** screen.
2. Add cards directly in the editor — or install the [`rem-card-capture` skill](#working-with-ai-agents-skill--cli) so your AI agents can propose source-grounded drafts into **Drafts**.
3. Start a review (desktop, `rem study`, or iPhone) and reveal each answer with <kbd>Space</kbd> or a tap.
4. Grade your recall with the on-screen controls or keyboard shortcuts (<kbd>1</kbd>–<kbd>4</kbd>).
5. Optionally configure JSON backups or Cloud sync from **Settings**.

## Data and sync

rem stores local data in SQLite through its native Rust layer (`rem-core`) across desktop, CLI, and iOS. JSON backup files preserve decks, cards, scheduling state, and review history. Unapproved drafts stay strictly local and are excluded from backup and cloud sync.

Cloud sync is optional and local-first by default (`Local only` vs. `Cloud sync` in **Settings**). When enabled, mutations are recorded in a transactional SQLite outbox with per-domain hybrid logical clocks (HLCs) and synced incrementally across desktop, CLI (`rem sync`), and iPhone without halting conflict dialogs:

- **Hosted [rem Premium](https://rem-sync.shettyh.fyi/premium)** — sign in with Google or Apple (native Sign in with Apple on iOS, OAuth + PKCE on desktop/CLI, tokens stored only in your OS Keychain). Includes a one-time 30-day free trial with no card required, then $5/year (Polar on desktop/web) or $4.99/year (App Store on iOS).
- **Self-hosted (free, open source)** — deploy the Apache-2.0 [`sync-server/`](sync-server/README.md) (Cloudflare Workers + per-user SQLite Durable Objects + D1 + R2) on your own Cloudflare account and point `rem` at your server URL.

## Development

### Prerequisites

- [Node.js 22](https://nodejs.org)
- [Rust stable](https://www.rust-lang.org/tools/install)
- The [Tauri v2 platform prerequisites](https://v2.tauri.app/start/prerequisites/)

### Run locally

```sh
git clone https://github.com/shettyh/rem.git
cd rem
npm ci
npm run app:dev
```

Use `npm run app:dev`, not `npm run dev`. The latter starts only the internal Vite server consumed by the native webview.

### Useful commands

| Command | Purpose |
| --- | --- |
| `npm run app:dev` | Run the native app in development mode |
| `npm run app:build` | Build native installers for the current platform |
| `npm test` | Run unit and real-browser UI tests |
| `npm run typecheck` | Type-check the frontend |
| `npm run build` | Type-check and build the frontend webview |
| `cd src-tauri && cargo test` | Run Rust tests |
| `cd src-tauri && cargo run -p rem-cli -- --help` | Run the CLI from source |
| `cd sync-server && npm test` | Run Cloudflare Workers sync server tests |
| `bash tools/build-ios-xcframework.sh --debug` | Build `RemCore.xcframework` & Swift UniFFI bindings for iOS |

Browser tests require Playwright's Chromium binary:

```sh
npx playwright install chromium
```

## Releasing

Releases are gated by a Release Please pull request:

1. Give feature and fix pull requests [Conventional Commit](https://www.conventionalcommits.org/) titles and squash-merge them into `main` so each change produces one clean changelog entry.
2. Release Please opens or updates a `chore: release vX.Y.Z` pull request with the generated changelog and synchronized Node, Rust, Tauri, and iOS versions.
3. Review that pull request and wait for CI to pass. Merge it when the accumulated changes are ready to ship.
4. The release workflow creates a draft GitHub Release, builds the macOS, Linux, and Windows installers and CLI archives, and uploads the iOS companion build to TestFlight.
5. The workflow publishes the desktop release only after every desktop/CLI build succeeds (leaving it as a draft on failure so it cannot become `latest` accidentally) and updates the Homebrew tap Cask.

Do not edit versions or create release tags manually. `feat` commits produce minor releases, `fix` commits produce patch releases, and breaking changes produce major releases. The repository must allow GitHub Actions to create pull requests under **Settings → Actions → General → Workflow permissions**.

macOS release signing/notarization and iOS TestFlight uploads require repository secrets under **Settings → Secrets and variables → Actions**: `APPLE_CERTIFICATE` (base64-encoded Developer ID `.p12`), `APPLE_CERTIFICATE_PASSWORD`, `APPLE_SIGNING_IDENTITY`, `APPLE_API_KEY`, `APPLE_API_ISSUER`, and `APPLE_API_KEY_PRIVATE_KEY` (contents of `AuthKey_<KEY_ID>.p8`). iOS TestFlight uploads reuse the three `APPLE_API_*` secrets via Xcode Cloud Signing (no separate iOS `.p12` certificate needed).

## Architecture

rem keeps domain logic and infrastructure behind small interfaces so that product features do not depend directly on a database or scheduling implementation.

```text
src/
  app/         application entry point and routes
  domain/      card/deck models and FSRS step/optimizer helpers
  data/        Storage interface, native Tauri adapter, backup, updater, and Cloud sync client
  features/    decks, cards, drafts, review, settings, and statistics
  ui/          app shell, shared components, themes, and design tokens
src-tauri/     native shell, rem-core (SQLite + FSRS-7 + sync engine), rem-sync, rem-ffi (UniFFI), and rem-cli
ios/           native SwiftUI iOS companion app backed by RemCore.xcframework
sync-server/   open-source Cloudflare Workers + Durable Objects + D1 + R2 sync backend
skills/        installable AI agent skills (rem-card-capture)
site/          landing page and interactive Desktop + iPhone + Agent showcase
```

The desktop frontend uses React, TypeScript, and Vite inside a Tauri v2 native shell, and the iPhone companion is built in native SwiftUI (`ios/`). The shared Rust `rem-core` crate owns SQLite, FSRS-7 calculation and optimization, the transactional sync outbox, and the complete `StudySession` state machine used by desktop, `rem study` terminal UI, and iOS (via UniFFI in `rem-ffi`); `rem-sync` owns HTTPS sync, asset upload/download, and OAuth/Keychain sessions across all three surfaces.

## Contributing

Issues and pull requests are welcome. For substantial changes, please open an issue first so the approach can be discussed before implementation.

Before submitting a pull request, run the same core checks used by CI:

```sh
npm test
npm run build
(
  cd src-tauri
  cargo fmt --all --check
  cargo clippy --workspace --all-targets -- -D warnings
  cargo test --workspace
)
```

## License

Licensed under the [Apache License 2.0](LICENSE).
