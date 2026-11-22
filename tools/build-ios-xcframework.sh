#!/usr/bin/env bash
# Build RemCore.xcframework (iOS device and/or simulator) and generate UniFFI Swift bindings.
#
# Usage:
#   bash tools/build-ios-xcframework.sh [--debug] [--sim-only | --device-only]
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MANIFEST="$ROOT_DIR/src-tauri/Cargo.toml"
OUT_SWIFT_DIR="$ROOT_DIR/ios/RemApp/Generated"
OUT_XCFRAMEWORK="$ROOT_DIR/ios/Frameworks/RemCore.xcframework"
TMP_BINDINGS="$ROOT_DIR/src-tauri/target/uniffi-ios"

PROFILE="release"
CARGO_PROFILE_FLAG="--release"
SLICE_MODE="all"

while [ "$#" -gt 0 ]; do
  case "$1" in
    --debug)
      PROFILE="debug"
      CARGO_PROFILE_FLAG=""
      ;;
    --sim-only)
      SLICE_MODE="sim"
      ;;
    --device-only)
      SLICE_MODE="device"
      ;;
    *)
      echo "Unknown option: $1" >&2
      echo "Usage: $0 [--debug] [--sim-only | --device-only]" >&2
      exit 1
      ;;
  esac
  shift
done

export IPHONEOS_DEPLOYMENT_TARGET="${IPHONEOS_DEPLOYMENT_TARGET:-17.0}"

IOS_LIB="$ROOT_DIR/src-tauri/target/aarch64-apple-ios/$PROFILE/librem_ffi.a"
SIM_LIB="$ROOT_DIR/src-tauri/target/aarch64-apple-ios-sim/$PROFILE/librem_ffi.a"

case "$SLICE_MODE" in
  all)
    echo "==> Building rem-ffi ($PROFILE, iOS $IPHONEOS_DEPLOYMENT_TARGET+) for aarch64-apple-ios and aarch64-apple-ios-sim"
    cargo build --manifest-path "$MANIFEST" -p rem-ffi --lib --target aarch64-apple-ios $CARGO_PROFILE_FLAG
    cargo build --manifest-path "$MANIFEST" -p rem-ffi --lib --target aarch64-apple-ios-sim $CARGO_PROFILE_FLAG
    BINDGEN_LIB="$SIM_LIB"
    ;;
  sim)
    echo "==> Building rem-ffi ($PROFILE, iOS $IPHONEOS_DEPLOYMENT_TARGET+) for aarch64-apple-ios-sim only"
    cargo build --manifest-path "$MANIFEST" -p rem-ffi --lib --target aarch64-apple-ios-sim $CARGO_PROFILE_FLAG
    BINDGEN_LIB="$SIM_LIB"
    ;;
  device)
    echo "==> Building rem-ffi ($PROFILE, iOS $IPHONEOS_DEPLOYMENT_TARGET+) for aarch64-apple-ios only"
    cargo build --manifest-path "$MANIFEST" -p rem-ffi --lib --target aarch64-apple-ios $CARGO_PROFILE_FLAG
    BINDGEN_LIB="$IOS_LIB"
    ;;
esac

echo "==> Generating UniFFI Swift bindings"
rm -rf "$TMP_BINDINGS"
mkdir -p "$TMP_BINDINGS" "$OUT_SWIFT_DIR" "$(dirname "$OUT_XCFRAMEWORK")"

(
  cd "$ROOT_DIR/src-tauri"
  cargo run -p rem-ffi --bin uniffi-bindgen -- \
    generate --library "$BINDGEN_LIB" --language swift --out-dir "$TMP_BINDINGS"
)

cp "$TMP_BINDINGS/rem_ffi.swift" "$OUT_SWIFT_DIR/rem_ffi.swift"

HEADERS_DIR="$TMP_BINDINGS/headers"
mkdir -p "$HEADERS_DIR"
cp "$TMP_BINDINGS/rem_ffiFFI.h" "$HEADERS_DIR/"
cp "$TMP_BINDINGS/rem_ffiFFI.modulemap" "$HEADERS_DIR/module.modulemap"

echo "==> Creating RemCore.xcframework ($SLICE_MODE)"
rm -rf "$OUT_XCFRAMEWORK"
XCFRAMEWORK_ARGS=()
if [ "$SLICE_MODE" = "all" ] || [ "$SLICE_MODE" = "device" ]; then
  XCFRAMEWORK_ARGS+=(-library "$IOS_LIB" -headers "$HEADERS_DIR")
fi
if [ "$SLICE_MODE" = "all" ] || [ "$SLICE_MODE" = "sim" ]; then
  XCFRAMEWORK_ARGS+=(-library "$SIM_LIB" -headers "$HEADERS_DIR")
fi

xcodebuild -create-xcframework "${XCFRAMEWORK_ARGS[@]}" -output "$OUT_XCFRAMEWORK"

echo "==> Done: $OUT_XCFRAMEWORK and $OUT_SWIFT_DIR/rem_ffi.swift"
