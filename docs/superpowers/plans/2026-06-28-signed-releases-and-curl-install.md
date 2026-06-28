# Signed Releases & Curl Install Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a one-line `curl` installer for `rem`, ad-hoc sign the macOS bundle, and document the release process.

**Architecture:** A POSIX `sh` installer (`install.sh`) resolves the latest GitHub release asset by OS+arch and installs it (DMG → /Applications on macOS, AppImage → ~/.local/bin on Linux). The fragile part — parsing the GitHub API for the right asset URL — is isolated into a sourceable function with a fixture test. Config/CI/docs changes are small and independent.

**Tech Stack:** POSIX shell, Tauri v2 config (JSON), GitHub Actions (YAML), Markdown.

## Global Constraints

- Target only macOS + Linux for the curl install (Windows uses the Releases page).
- Linux build is **x86_64 only** (matches `release.yml`).
- No `jq` dependency in `install.sh` — parse JSON with `grep`/`sed`.
- Repo is `shettyh/rem`; raw script URL is `https://raw.githubusercontent.com/shettyh/rem/main/install.sh`.
- Tauri asset names: macOS arm64 = `*aarch64.dmg`, macOS x86_64 = `*x64.dmg`, Linux = `*.AppImage`.
- Commit as `shettyh <manjunathshetty@live.com>` — every commit uses
  `git -c user.name='shettyh' -c user.email='manjunathshetty@live.com' commit`.
- We are on branch `release-signing-and-install`. Stay on it.

---

### Task 1: `install.sh` with tested asset-selection

**Files:**
- Create: `install.sh`
- Test: `tools/test-install.sh`

**Interfaces:**
- Produces: `select_asset_url <json> <ext_regex> <arch_keyword>` — prints the first
  `browser_download_url` whose filename matches `ext_regex` and (if non-empty)
  `arch_keyword`; prints nothing if no match. Sourceable when `REM_INSTALL_LIB=1`.

- [ ] **Step 1: Write the failing test**

Create `tools/test-install.sh`:

```sh
#!/bin/sh
# Tests install.sh asset-selection against a fixture (no network).
set -eu
REM_INSTALL_LIB=1 . "$(dirname "$0")/../install.sh"

FIXTURE='{
  "assets": [
    {"browser_download_url": "https://github.com/shettyh/rem/releases/download/v0.2.0/rem_0.2.0_aarch64.dmg"},
    {"browser_download_url": "https://github.com/shettyh/rem/releases/download/v0.2.0/rem_0.2.0_x64.dmg"},
    {"browser_download_url": "https://github.com/shettyh/rem/releases/download/v0.2.0/rem_0.2.0_amd64.AppImage"},
    {"browser_download_url": "https://github.com/shettyh/rem/releases/download/v0.2.0/rem_0.2.0_x64-setup.exe"}
  ]
}'

fail=0
check() {
  if [ "$2" = "$3" ]; then printf 'ok   - %s\n' "$1"
  else printf 'FAIL - %s\n      got:  %s\n      want: %s\n' "$1" "$2" "$3"; fail=1; fi
}

base="https://github.com/shettyh/rem/releases/download/v0.2.0"
check "macOS arm64 -> aarch64 dmg" "$(select_asset_url "$FIXTURE" '\.dmg' 'aarch64')" "$base/rem_0.2.0_aarch64.dmg"
check "macOS x86_64 -> x64 dmg"    "$(select_asset_url "$FIXTURE" '\.dmg' 'x64')"     "$base/rem_0.2.0_x64.dmg"
check "Linux -> AppImage"          "$(select_asset_url "$FIXTURE" '\.AppImage' '')"   "$base/rem_0.2.0_amd64.AppImage"
check "no match -> empty"          "$(select_asset_url "$FIXTURE" '\.deb' '')"        ""

exit $fail
```

- [ ] **Step 2: Run test to verify it fails**

Run: `sh tools/test-install.sh`
Expected: FAIL — `install.sh` does not exist yet, so sourcing errors (`No such file or directory`).

- [ ] **Step 3: Write minimal implementation (the whole installer)**

Create `install.sh`:

```sh
#!/bin/sh
# rem installer — downloads the latest GitHub release asset and installs it.
#   curl -fsSL https://raw.githubusercontent.com/shettyh/rem/main/install.sh | sh
set -eu

REPO="shettyh/rem"
API="https://api.github.com/repos/${REPO}/releases/latest"

err()  { printf 'error: %s\n' "$1" >&2; exit 1; }
info() { printf '%s\n' "$1" >&2; }

# select_asset_url <json> <ext_regex> <arch_keyword>
# Prints the first browser_download_url whose filename matches ext_regex and
# (if arch_keyword is non-empty) arch_keyword. Prints nothing on no match.
select_asset_url() {
  _json="$1"; _ext="$2"; _arch="$3"
  printf '%s\n' "$_json" \
    | grep -o '"browser_download_url": *"[^"]*"' \
    | sed -E 's/.*"(https[^"]*)".*/\1/' \
    | grep -i "$_ext" \
    | { if [ -n "$_arch" ]; then grep -i "$_arch"; else cat; fi; } \
    | head -n1
}

fetch_release_json() {
  curl -fsSL "$API" || err "could not reach GitHub API. Is there a published release yet? ($API)"
}

install_macos() {
  case "$1" in
    arm64)  key="aarch64" ;;
    x86_64) key="x64" ;;
    *) err "unsupported macOS arch: $1" ;;
  esac
  url=$(select_asset_url "$(fetch_release_json)" '\.dmg' "$key")
  [ -n "$url" ] || err "no .dmg asset for '$key' in the latest release"

  tmp=$(mktemp -d)
  trap 'rm -rf "$tmp"' EXIT
  info "Downloading $url"
  curl -fL -o "$tmp/rem.dmg" "$url" || err "download failed"

  info "Mounting disk image"
  attach_out=$(hdiutil attach "$tmp/rem.dmg" -nobrowse -readonly) || err "could not mount dmg"
  mnt=$(printf '%s\n' "$attach_out" | grep -o '/Volumes/.*' | tail -n1)
  [ -n "$mnt" ] || err "could not find mounted volume"
  trap 'hdiutil detach "$mnt" -quiet 2>/dev/null; rm -rf "$tmp"' EXIT

  app=$(find "$mnt" -maxdepth 1 -name '*.app' | head -n1)
  [ -n "$app" ] || err "no .app found inside the dmg"
  name=$(basename "$app")

  info "Installing $name to /Applications"
  rm -rf "/Applications/$name"
  cp -R "$app" /Applications/
  xattr -dr com.apple.quarantine "/Applications/$name" 2>/dev/null || true
  info "Done. Launch $name from /Applications or Spotlight."
}

install_linux() {
  case "$1" in
    x86_64) : ;;
    *) err "unsupported Linux arch: $1 (only x86_64 is built)" ;;
  esac
  url=$(select_asset_url "$(fetch_release_json)" '\.AppImage' "")
  [ -n "$url" ] || err "no .AppImage asset in the latest release"

  dest="$HOME/.local/bin"
  mkdir -p "$dest"
  info "Downloading $url"
  curl -fL -o "$dest/rem" "$url" || err "download failed"
  chmod +x "$dest/rem"
  info "Installed to $dest/rem"
  case ":$PATH:" in
    *":$dest:"*) : ;;
    *) info "Note: $dest is not on your PATH. Add: export PATH=\"$dest:\$PATH\"" ;;
  esac
  info "AppImage needs FUSE. On Debian/Ubuntu: sudo apt install libfuse2"
}

main() {
  os=$(uname -s); arch=$(uname -m)
  case "$os" in
    Darwin) install_macos "$arch" ;;
    Linux)  install_linux "$arch" ;;
    *) err "unsupported OS: $os (use the GitHub Releases page for Windows)" ;;
  esac
}

# Allow sourcing for tests without executing.
if [ "${REM_INSTALL_LIB:-0}" != "1" ]; then
  main "$@"
fi
```

- [ ] **Step 4: Run test to verify it passes**

Run: `sh tools/test-install.sh`
Expected: all four lines start `ok`, exit status 0.

- [ ] **Step 5: Syntax + lint check**

Run: `sh -n install.sh && sh -n tools/test-install.sh && (command -v shellcheck >/dev/null && shellcheck install.sh tools/test-install.sh || echo "shellcheck not installed, skipped")`
Expected: no syntax errors; shellcheck clean or skipped.

- [ ] **Step 6: Make scripts executable and commit**

```bash
chmod +x install.sh tools/test-install.sh
git -c user.name='shettyh' -c user.email='manjunathshetty@live.com' \
  add install.sh tools/test-install.sh
git -c user.name='shettyh' -c user.email='manjunathshetty@live.com' \
  commit -m "feat: add curl install.sh for macOS and Linux

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

### Task 2: Ad-hoc sign the macOS bundle

**Files:**
- Modify: `src-tauri/tauri.conf.json` (`bundle` object)

**Interfaces:**
- Consumes: nothing.
- Produces: `bundle.macOS.signingIdentity === "-"` for ad-hoc signing.

- [ ] **Step 1: Add the macOS signing block**

In `src-tauri/tauri.conf.json`, the `bundle` object currently ends with the
`android` key. Add a `macOS` key alongside it:

```json
  "bundle": {
    "active": true,
    "targets": "all",
    "icon": [
      "icons/32x32.png",
      "icons/128x128.png",
      "icons/128x128@2x.png",
      "icons/icon.icns",
      "icons/icon.ico"
    ],
    "macOS": {
      "signingIdentity": "-"
    },
    "android": {
      "debugApplicationIdSuffix": ".debug"
    }
  }
```

- [ ] **Step 2: Verify JSON validity and the value**

Run:
```bash
node -e "const c=JSON.parse(require('fs').readFileSync('src-tauri/tauri.conf.json','utf8')); if(c.bundle.macOS.signingIdentity!=='-') throw new Error('signingIdentity not set'); console.log('ok')"
```
Expected: prints `ok`.

- [ ] **Step 3: Commit**

```bash
git -c user.name='shettyh' -c user.email='manjunathshetty@live.com' \
  add src-tauri/tauri.conf.json
git -c user.name='shettyh' -c user.email='manjunathshetty@live.com' \
  commit -m "build: ad-hoc sign the macOS bundle

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

### Task 3: Fix the signing comment in release.yml

**Files:**
- Modify: `.github/workflows/release.yml` (the `env:` comment block under the `tauri-apps/tauri-action@v0` step)

**Interfaces:**
- Consumes: nothing. Produces: nothing (comment-only change, no behavior change).

- [ ] **Step 1: Replace the comment block**

Find this block:

```yaml
        env:
          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
          # --- Code signing is disabled. To enable, add these as repo secrets ---
          # macOS notarization:
          #   APPLE_CERTIFICATE, APPLE_CERTIFICATE_PASSWORD, APPLE_SIGNING_IDENTITY,
          #   APPLE_ID, APPLE_PASSWORD, APPLE_TEAM_ID
          # Windows code signing:
          #   TAURI_SIGNING_PRIVATE_KEY, TAURI_SIGNING_PRIVATE_KEY_PASSWORD
```

Replace with:

```yaml
        env:
          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
          # --- Code signing is intentionally OFF (no Apple Developer ID). ---
          # macOS ships ad-hoc signed; the curl installer avoids Gatekeeper
          # quarantine, and the README documents the workaround for manual
          # DMG downloads. To enable real Developer ID signing + notarization
          # later, add these repo secrets and they are picked up automatically:
          #   APPLE_CERTIFICATE, APPLE_CERTIFICATE_PASSWORD, APPLE_SIGNING_IDENTITY,
          #   APPLE_ID, APPLE_PASSWORD, APPLE_TEAM_ID
          # NOTE: TAURI_SIGNING_PRIVATE_KEY is for the Tauri auto-updater, not
          # Windows Authenticode — no updater is configured, so it is unused.
```

- [ ] **Step 2: Verify YAML still parses**

Run:
```bash
ruby -ryaml -e "YAML.load_file('.github/workflows/release.yml'); puts 'ok'"
```
Expected: prints `ok`.

- [ ] **Step 3: Commit**

```bash
git -c user.name='shettyh' -c user.email='manjunathshetty@live.com' \
  add .github/workflows/release.yml
git -c user.name='shettyh' -c user.email='manjunathshetty@live.com' \
  commit -m "ci: clarify code-signing comment in release workflow

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

### Task 4: README Install + Cutting-a-release docs

**Files:**
- Modify: `README.md` (add two sections after the `## Run & build` section)

**Interfaces:**
- Consumes: the `install.sh` raw URL from Task 1. Produces: nothing.

- [ ] **Step 1: Add the Install and Cutting-a-release sections**

Insert the following after the `## Run & build` section (before `## Architecture`):

````markdown
## Install

**macOS / Linux** — one-liner (no Apple Developer warning; `curl` downloads skip
macOS quarantine):

```bash
curl -fsSL https://raw.githubusercontent.com/shettyh/rem/main/install.sh | sh
```

macOS copies `rem.app` to `/Applications`; Linux installs the AppImage to
`~/.local/bin/rem` (needs FUSE; on Debian/Ubuntu `sudo apt install libfuse2`).

**Manual download** — grab a build from the
[Releases page](https://github.com/shettyh/rem/releases). Because rem isn't
notarized (no paid Apple Developer ID), a **browser-downloaded** DMG triggers a
Gatekeeper warning. Either right-click the app → **Open** → **Open**, or clear
quarantine after copying it to Applications:

```bash
xattr -dr com.apple.quarantine /Applications/rem.app
```

**Windows** — download the `.msi`/`.exe` from the Releases page.

## Cutting a release

The [`release.yml`](.github/workflows/release.yml) workflow builds installers for
macOS (Apple Silicon + Intel), Linux, and Windows on every `v*` tag.

1. Bump the version to match in all three files: `package.json`,
   `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json`.
2. Commit, then tag and push:
   ```bash
   git tag v0.1.1 && git push origin v0.1.1
   ```
3. The workflow opens a **draft** GitHub Release with the installers attached.
4. **Publish the draft.** Required — the curl installer reads `releases/latest`,
   which ignores drafts and prereleases, so it only finds a published release.
````

- [ ] **Step 2: Verify the install command is present**

Run: `grep -q 'install.sh | sh' README.md && grep -q 'com.apple.quarantine' README.md && echo ok`
Expected: prints `ok`.

- [ ] **Step 3: Commit**

```bash
git -c user.name='shettyh' -c user.email='manjunathshetty@live.com' \
  add README.md
git -c user.name='shettyh' -c user.email='manjunathshetty@live.com' \
  commit -m "docs: add install one-liner and release instructions

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

## Manual verification (after first published release)

These can't be automated until a release exists on GitHub:

- [ ] On macOS: `curl -fsSL https://raw.githubusercontent.com/shettyh/rem/main/install.sh | sh`, confirm `rem.app` lands in `/Applications` and launches with no Gatekeeper prompt.
- [ ] On Linux x86_64: same command, confirm `~/.local/bin/rem` runs.
- [ ] Confirm the published release page lists the DMGs, AppImage, and Windows installer as assets.

## Notes / out of scope

- Homebrew cask, Windows curl install, Tauri auto-updater, and version-bump automation are deliberately excluded (YAGNI).
- Real notarization / Windows Authenticode require paid accounts — not done now.
