#!/usr/bin/env bash
# Select the highest-versioned stable Xcode installed under /Applications
# (skipping symlinks and beta/RC pre-releases) so CI and TestFlight always
# build against the latest GA iOS SDK available on the runner.
#
# Usage:
#   bash tools/select-latest-xcode.sh [--print-only] [--applications-dir <dir>]
set -euo pipefail

PRINT_ONLY=0
APPLICATIONS_DIR="/Applications"

while [ "$#" -gt 0 ]; do
  case "$1" in
    --print-only)
      PRINT_ONLY=1
      ;;
    --applications-dir)
      if [ "$#" -lt 2 ]; then
        echo "Missing argument for --applications-dir" >&2
        exit 2
      fi
      APPLICATIONS_DIR="$2"
      shift
      ;;
    *)
      echo "Unknown option: $1" >&2
      echo "Usage: $0 [--print-only] [--applications-dir <dir>]" >&2
      exit 2
      ;;
  esac
  shift
done

xcode_app=$(python3 - "$APPLICATIONS_DIR" <<'PY'
import glob
import os
import re
import sys

apps_dir = sys.argv[1]
candidates = []

for path in glob.glob(os.path.join(apps_dir, "Xcode*.app")):
    if os.path.islink(path) or not os.path.isdir(path):
        continue
    name = os.path.basename(path)
    match = re.fullmatch(r"Xcode_(\d+(?:\.\d+)*)\.app", name)
    if match:
        version = tuple(int(part) for part in match.group(1).split("."))
        candidates.append((version, path))

if candidates:
    candidates.sort(reverse=True)
    print(candidates[0][1])
else:
    fallback = os.path.join(apps_dir, "Xcode.app")
    if os.path.isdir(fallback):
        print(fallback)
PY
)

if [ -z "$xcode_app" ]; then
  echo "::error::No stable Xcode installation found under $APPLICATIONS_DIR" >&2
  exit 1
fi

if [ "$PRINT_ONLY" -eq 1 ]; then
  echo "$xcode_app"
  exit 0
fi

developer_dir="$xcode_app/Contents/Developer"
if [ ! -d "$developer_dir" ]; then
  echo "::error::Expected Developer directory at $developer_dir" >&2
  exit 1
fi

echo "Selecting Xcode: $xcode_app"
if [ "$(id -u)" -eq 0 ]; then
  xcode-select -s "$developer_dir"
else
  sudo xcode-select -s "$developer_dir"
fi

xcodebuild -version
sdk_path=$(xcrun --sdk iphoneos --show-sdk-path)
sdk_name=$(/usr/libexec/PlistBuddy -c "Print :CanonicalName" "$sdk_path/SDKSettings.plist")
sdk_version=$(xcrun --sdk iphoneos --show-sdk-version)
sdk_build=$(xcrun --sdk iphoneos --show-sdk-build-version)
echo "iOS SDK: ${sdk_name} (${sdk_version}, build ${sdk_build})"
