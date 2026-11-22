# rem

**Local-first spaced repetition for durable understanding across desktop, terminal, and iPhone — with agent-friendly CLI capture, FSRS-7 scheduling, Markdown cards, and optional Cloud sync.**

[![CI](https://github.com/shettyh/rem/actions/workflows/ci.yml/badge.svg)](https://github.com/shettyh/rem/actions/workflows/ci.yml)
[![Latest release](https://img.shields.io/github/v/release/shettyh/rem)](https://github.com/shettyh/rem/releases/latest)
[![License](https://img.shields.io/github/license/shettyh/rem?cacheSeconds=300)](LICENSE)
[![Tauri 2](https://img.shields.io/badge/Tauri-2-24C8DB?logo=tauri&logoColor=white)](https://tauri.app)

[Website & interactive tour](https://shettyh.github.io/rem/) · [Download](https://github.com/shettyh/rem/releases/latest) · [CLI reference](docs/cli.md) · [iOS companion](ios/README.md)

Use rem to build a durable understanding of the system you are working in—or any topic. Agents can turn code, documentation, research, or other source material into a few source-grounded recall prompts; you decide what enters your study plan, and accepted cards use the same FSRS-7 scheduler whether you review in the desktop app, terminal, or iPhone companion.

```text
explore a system or topic → agent proposes drafts → you try, edit, and approve → FSRS schedules → study on desktop, terminal, or iPhone
```

- **Human-approved agent capture** — agents can generate cards from the material at hand, but each proposal remains an unscheduled local draft until you accept it.
- **One review engine** — desktop, terminal (`rem study`), and iOS all share the same Rust `StudySession`, scheduling rules, limits, and history.
- **Local by default** — your SQLite collection needs no account or hosted service and remains exportable.
- **See it in action** — explore the interactive Desktop, iPhone, and Terminal showcase at **[shettyh.github.io/rem](https://shettyh.github.io/rem/)**.

> [!IMPORTANT]
> rem ships as a native [Tauri](https://tauri.app) desktop app for macOS, Linux, and Windows, a companion `rem` CLI, and a native SwiftUI iOS companion (`ios/`). The browser build is only the source for Tauri's webview and is not a supported version of the app.

## Features

- **Local-first by default** — decks, cards, assets, and review history live on your device in SQLite.
- **Modern scheduling** — FSRS-7 runs in Rust, with configurable retention, learning steps, daily limits, and per-deck parameter optimization.
- **Rich Markdown cards** — a WYSIWYG desktop editor and paper-canvas iOS editor with syntax-highlighted code, images and GIFs, links, lists, and tags.
- **Fast review flow** — study in the desktop app, terminal, or iPhone with keyboard shortcuts or touch grading, four grades, live interval previews, and shared FSRS scheduling.
- **Native iOS companion** — SwiftUI iPhone app backed by `rem-core` and `rem-sync` via UniFFI, with local-first SQLite, focused review, Quick Capture with Photos, 7D/30D/90D stats, local daily reminders, and Cloud sync.
- **Actionable statistics** — review activity, recall rate, streaks, grade distribution, and per-deck breakdowns across desktop and iOS.
- **Portable data** — import and export full-fidelity JSON backups.
- **Agent-first CLI capture** — agents turn codebases, research, and other source material into source-grounded drafts for human approval; trusted workflows can still create Markdown cards directly.
- **Optional Cloud sync** — sync decks, reviews, and media across desktop, CLI, and iPhone using hosted [rem Cloud](https://rem-sync.shettyh.fyi/cloud) or your own self-hosted open-source [Cloudflare Worker](sync-server/README.md).
- **Calm Quiet Paper design** — light and dark themes, a collapsible icon-rail sidebar on desktop, and distraction-free review keep focus on recall.

## Installation

Download the latest installer for your platform from [GitHub Releases](https://github.com/shettyh/rem/releases/latest).

| Platform | Desktop packages | CLI archives |
| --- | --- | --- |
| macOS | Apple Silicon and Intel DMG | Apple Silicon and Intel `.tar.gz` |
| Linux | AppImage, `.deb`, and `.rpm` | x86_64 `.tar.gz` |
| Windows | `.msi` and `.exe` | x86_64 `.zip` |

### macOS and Linux installer

```sh
curl -fsSL https://raw.githubusercontent.com/shettyh/rem/main/install.sh | sh
```

This installs both the desktop app and the `rem` CLI. On Linux, launch the desktop AppImage with `rem-app`; `rem` is reserved for the CLI. The AppImage requires FUSE. Debian and Ubuntu users can install it with `sudo apt install libfuse2`.

Windows releases include a separate CLI zip. Extract `rem.exe` into a directory on `PATH`.

### Agent skill

After installing the `rem` CLI, install the `rem-card-capture` skill for your local AI agents:

```sh
npx skills add shettyh/rem --skill rem-card-capture --global
```

The [Skills CLI](https://github.com/vercel-labs/skills) detects supported agents such as Claude Code and Codex and installs the skill in their user-level directories. Omit `--global` to install it only in the current project.

### iOS companion

The native SwiftUI iPhone app lives in [`ios/`](ios/README.md) (iOS 17+, Xcode 16+). TestFlight and App Store distribution are planned; for now, build and run it locally on Simulator or your iPhone using the steps in [`ios/README.md`](ios/README.md).

### Unsigned build notice

Current macOS and Windows builds are not notarized or code-signed with paid platform certificates. On macOS, right-click **rem** and choose **Open**, or clear quarantine after moving the app to `/Applications`:

```sh
xattr -dr com.apple.quarantine /Applications/rem.app
```

## Getting started

1. Create a deck from the **Today** screen.
2. Add cards with a front, back, and optional tags or images.
3. Start a review and reveal each answer with <kbd>Space</kbd>.
4. Grade your recall with the on-screen controls or keyboard shortcuts.
5. Optionally configure JSON backups or Cloud sync from **Settings**.

Cards and approval-gated drafts can also be captured while the app is closed:

```sh
rem deck list
rem card add --deck <deck-id> --front 'Question' --back 'Answer' --tag topic
rem draft add --deck <deck-id> --front 'Question' --back 'Proposed answer' --producer pi
rem study                       # all decks
rem study --deck <id-or-name>   # one deck
```

Open **Drafts** in the desktop sidebar to try each proposed question, inspect its source and rationale, edit it, and accept or reject it. See the [CLI reference](docs/cli.md) for JSON batches, dry runs, and stable result schemas. AI agents can use the installable [`rem-card-capture` skill](skills/rem-card-capture/SKILL.md) to propose source-grounded drafts through the same public CLI.

## Data and sync

rem stores local data in SQLite through its native Rust layer (`rem-core`) across desktop, CLI, and iOS. JSON backup files preserve decks, cards, scheduling state, and review history. Unapproved drafts stay local and are excluded from backup and cloud sync.

Cloud sync is optional and local-first by default (`Local only` vs. `Cloud sync` in **Settings**). When enabled, mutations are recorded in a transactional SQLite outbox with hybrid logical clocks (HLCs) and synced incrementally across desktop, CLI (`rem sync`), and iPhone. You can sign in to hosted [rem Cloud](https://rem-sync.shettyh.fyi/cloud) (Google or Apple OAuth, with tokens stored only in your OS Keychain) or deploy the Apache-2.0 [`sync-server/`](sync-server/README.md) on your own Cloudflare account and point `rem` at your server URL.

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
2. Release Please opens or updates a `chore: release vX.Y.Z` pull request with the generated changelog and synchronized Node, Rust, and Tauri versions.
3. Review that pull request and wait for CI to pass. Merge it when the accumulated changes are ready to ship.
4. The release workflow creates a draft GitHub Release and builds the macOS, Linux, and Windows installers and CLI archives.
5. The workflow publishes the release only after every app and CLI build succeeds. A failed build leaves the release as a draft so it cannot become `latest` accidentally.

Do not edit versions or create release tags manually. `feat` commits produce minor releases, `fix` commits produce patch releases, and breaking changes produce major releases. The repository must allow GitHub Actions to create pull requests under **Settings → Actions → General → Workflow permissions**.

## Architecture

rem keeps domain logic and infrastructure behind small interfaces so that product features do not depend directly on a database or scheduling implementation.

```text
src/
  app/         application entry point and routes
  domain/      card/deck models and the Scheduler interface
  data/        Storage interface, native adapter, test persistence, backup, and Cloud sync client
  features/    decks, cards, drafts, review, settings, and statistics
  ui/          app shell, shared components, themes, and design tokens
src-tauri/     native shell, rem-core (SQLite + FSRS-7 + sync engine), rem-sync, rem-ffi (UniFFI), and rem-cli
ios/           native SwiftUI iOS companion app backed by RemCore.xcframework
sync-server/   open-source Cloudflare Workers + Durable Objects + D1 + R2 sync backend
site/          landing page and interactive Desktop + iPhone showcase
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
