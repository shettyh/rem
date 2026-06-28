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
