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
