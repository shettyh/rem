#!/bin/sh
# Tests install.sh asset-selection against a fixture (no network).
set -eu
REM_INSTALL_LIB=1 . "$(dirname "$0")/../install.sh"

FIXTURE='{
  "assets": [
    {"browser_download_url": "https://github.com/shettyh/rem/releases/download/v0.2.0/rem_0.2.0_aarch64.dmg"},
    {"browser_download_url": "https://github.com/shettyh/rem/releases/download/v0.2.0/rem_0.2.0_x64.dmg"},
    {"browser_download_url": "https://github.com/shettyh/rem/releases/download/v0.2.0/rem_0.2.0_amd64.AppImage"},
    {"browser_download_url": "https://github.com/shettyh/rem/releases/download/v0.2.0/rem_0.2.0_x64-setup.exe"},
    {"browser_download_url": "https://github.com/shettyh/rem/releases/download/v0.2.0/rem-cli-aarch64-apple-darwin.tar.gz"},
    {"browser_download_url": "https://github.com/shettyh/rem/releases/download/v0.2.0/rem-cli-x86_64-apple-darwin.tar.gz"},
    {"browser_download_url": "https://github.com/shettyh/rem/releases/download/v0.2.0/rem-cli-x86_64-unknown-linux-gnu.tar.gz"},
    {"browser_download_url": "https://github.com/shettyh/rem/releases/download/v0.2.0/rem-cli-x86_64-pc-windows-msvc.zip"}
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
check "macOS arm64 -> CLI"         "$(select_asset_url "$FIXTURE" 'rem-cli-.*\.tar\.gz' 'aarch64-apple-darwin')" "$base/rem-cli-aarch64-apple-darwin.tar.gz"
check "macOS x86_64 -> CLI"        "$(select_asset_url "$FIXTURE" 'rem-cli-.*\.tar\.gz' 'x86_64-apple-darwin')" "$base/rem-cli-x86_64-apple-darwin.tar.gz"
check "Linux -> CLI"               "$(select_asset_url "$FIXTURE" 'rem-cli-.*\.tar\.gz' 'x86_64-unknown-linux-gnu')" "$base/rem-cli-x86_64-unknown-linux-gnu.tar.gz"
check "Windows -> CLI"             "$(select_asset_url "$FIXTURE" 'rem-cli-.*\.zip' 'x86_64-pc-windows-msvc')" "$base/rem-cli-x86_64-pc-windows-msvc.zip"
check "no match -> empty"          "$(select_asset_url "$FIXTURE" '\.deb' '')"        ""

if grep -q 'curl -fL -o "$dest/rem-app" "$app_url"' "$(dirname "$0")/../install.sh"; then
  printf 'ok   - Linux desktop command is rem-app\n'
else
  printf 'FAIL - Linux desktop command would collide with CLI rem\n'
  fail=1
fi

exit $fail
