# Signed releases, curl install, and release process

**Date:** 2026-06-28
**Status:** Approved, pending implementation

## Goal

Three related improvements to `rem`'s distribution:

1. Make released binaries (macOS DMG etc.) as "signed" as possible.
2. Add a one-line `curl` installer.
3. Document how to cut a GitHub Release with attached binaries.

## Constraints / context

- App is Tauri v2 (React + Rust). Release pipeline already exists in
  `.github/workflows/release.yml`: pushing a `v*` tag runs `tauri-apps/tauri-action`,
  which builds macOS (aarch64 + x86_64, separate DMGs), Linux (x86_64), and Windows,
  then opens a **draft** GitHub Release with the installers attached.
- **No Apple Developer account** (and not paying for one now). Same assumption for
  Windows Authenticode. So genuine code signing / notarization is off the table.
- No Tauri auto-updater is configured (no `tauri-plugin-updater`), so updater signing
  (`TAURI_SIGNING_PRIVATE_KEY`) is irrelevant.

## Key insight on signing without a paid account

- Notarization is impossible without a Developer ID, so a **browser-downloaded** DMG
  will always hit a Gatekeeper "unidentified developer" prompt. We cannot remove that.
- We **can** force **ad-hoc signing** of the macOS bundle. This gives a stable signature
  so the app launches cleanly on Apple Silicon and avoids the occasional "app is damaged"
  error. It does **not** remove the Gatekeeper warning for quarantined downloads.
- The **curl installer is the real win**: files downloaded with `curl` never receive the
  `com.apple.quarantine` flag (only browsers/quarantine-aware apps set it), so installing
  via the script produces **no Gatekeeper warning at all**.
- The manual DMG-download path gets a documented workaround instead:
  right-click → Open, or `xattr -dr com.apple.quarantine /Applications/rem.app`.

## Components

### 1. `install.sh` (repo root)

POSIX `sh`, invoked via:

```sh
curl -fsSL https://raw.githubusercontent.com/shettyh/rem/main/install.sh | sh
```

Behavior:

1. Detect OS via `uname -s`, arch via `uname -m`.
2. Query `https://api.github.com/repos/shettyh/rem/releases/latest` and extract the
   matching asset's `browser_download_url`. Match **loosely** by extension + arch keyword
   so it survives version changes:
   - macOS arm64 → asset ending `.dmg` containing `aarch64`
   - macOS x86_64 → asset ending `.dmg` containing `x64` (fallback `x86_64`)
   - Linux x86_64 → asset ending `.AppImage`
3. **macOS:** download DMG to a temp file → `hdiutil attach -nobrowse -quiet` →
   `cp -R` the `rem.app` to `/Applications` (remove any prior copy first) →
   `hdiutil detach` → clean up temp file. No `sudo`. Also run
   `xattr -dr com.apple.quarantine` on the installed app as a harmless belt-and-suspenders.
4. **Linux:** download AppImage → `chmod +x` → install to `~/.local/bin/rem`.
   Warn if `~/.local/bin` is not on `PATH`. Note that AppImage needs FUSE.
   x86_64 only (matches what the release builds).
5. Fail with a clear message when: unsupported OS/arch, no matching asset found, or no
   published release exists yet.

Implementation notes:
- Avoid a `jq` dependency — parse the API JSON with `grep`/`sed`.
- Use `set -e`; download with `curl -fL`.

### 2. `src-tauri/tauri.conf.json`

Add ad-hoc signing:

```json
"bundle": {
  "macOS": { "signingIdentity": "-" }
}
```

This is the only config change.

### 3. `.github/workflows/release.yml`

Replace the stale/misleading signing comment block. The current comment lumps Tauri's
updater key in with "Windows code signing"; they are different. New comment accurately
states: macOS signing/notarization is intentionally disabled (no Developer ID), and lists
the `APPLE_*` secrets that would enable it later. No behavioral change to the build.

### 4. `README.md`

Add an **Install** section:
- The `curl` one-liner (macOS + Linux).
- Manual download note + the Gatekeeper workaround for browser-downloaded DMGs.

Add a **Cutting a release** section (see below).

## Release process (documentation, already wired)

1. Bump the version in all three files so they match:
   `package.json`, `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json`.
2. Commit, then tag and push:
   ```sh
   git tag v0.1.1 && git push origin v0.1.1
   ```
3. `release.yml` builds all four targets and opens a **draft** Release with the
   DMGs / AppImage / MSI attached.
4. **Publish the draft** on GitHub. This is required: `install.sh` uses
   `/releases/latest`, which ignores drafts and prereleases, so the curl install only
   finds a release once it is published.

## Out of scope (YAGNI)

- Homebrew cask / tap.
- Windows one-line (PowerShell) install.
- Tauri auto-updater.
- Automating the three-file version bump.
- Real macOS notarization / Windows Authenticode (require paid accounts).
