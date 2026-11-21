#!/usr/bin/env bash
# Build RemCore.xcframework (iOS device + iOS simulator) and generate UniFFI Swift bindings.
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MANIFEST="$ROOT_DIR/src-tauri/Cargo.toml"
OUT_SWIFT_DIR="$ROOT_DIR/ios/RemApp/Generated"
OUT_XCFRAMEWORK="$ROOT_DIR/ios/Frameworks/RemCore.xcframework"
TMP_BINDINGS="$ROOT_DIR/src-tauri/target/uniffi-ios"

PROFILE="release"
CARGO_PROFILE_FLAG="--release"
if [ "${1:-}" = "--debug" ]; then
  PROFILE="debug"
  CARGO_PROFILE_FLAG=""
fi

export IPHONEOS_DEPLOYMENT_TARGET="${IPHONEOS_DEPLOYMENT_TARGET:-17.0}"

echo "==> Building rem-ffi ($PROFILE, iOS $IPHONEOS_DEPLOYMENT_TARGET+) for aarch64-apple-ios and aarch64-apple-ios-sim"
cargo build --manifest-path "$MANIFEST" -p rem-ffi --lib --target aarch64-apple-ios $CARGO_PROFILE_FLAG
cargo build --manifest-path "$MANIFEST" -p rem-ffi --lib --target aarch64-apple-ios-sim $CARGO_PROFILE_FLAG

IOS_LIB="$ROOT_DIR/src-tauri/target/aarch64-apple-ios/$PROFILE/librem_ffi.a"
SIM_LIB="$ROOT_DIR/src-tauri/target/aarch64-apple-ios-sim/$PROFILE/librem_ffi.a"

echo "==> Generating UniFFI Swift bindings"
rm -rf "$TMP_BINDINGS"
mkdir -p "$TMP_BINDINGS" "$OUT_SWIFT_DIR" "$(dirname "$OUT_XCFRAMEWORK")"

(
  cd "$ROOT_DIR/src-tauri"
  cargo run -p rem-ffi --bin uniffi-bindgen -- \
    generate --library "$SIM_LIB" --language swift --out-dir "$TMP_BINDINGS"
)

cp "$TMP_BINDINGS/rem_ffi.swift" "$OUT_SWIFT_DIR/rem_ffi.swift"

HEADERS_DIR="$TMP_BINDINGS/headers"
mkdir -p "$HEADERS_DIR"
cp "$TMP_BINDINGS/rem_ffiFFI.h" "$HEADERS_DIR/"
cp "$TMP_BINDINGS/rem_ffiFFI.modulemap" "$HEADERS_DIR/module.modulemap"

echo "==> Creating RemCore.xcframework"
rm -rf "$OUT_XCFRAMEWORK"
xcodebuild -create-xcframework \
  -library "$IOS_LIB" -headers "$HEADERS_DIR" \
  -library "$SIM_LIB" -headers "$HEADERS_DIR" \
  -output "$OUT_XCFRAMEWORK"

echo "==> Done: $OUT_XCFRAMEWORK and $OUT_SWIFT_DIR/rem_ffi.swift"
