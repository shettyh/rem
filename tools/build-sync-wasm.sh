#!/usr/bin/env bash
set -euo pipefail

CHECK_ONLY=false
while [[ $# -gt 0 ]]; do
  case "$1" in
    --check)
      CHECK_ONLY=true
      shift
      ;;
    *)
      echo "Usage: $0 [--check]" >&2
      exit 2
      ;;
  esac
done

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TAURI_DIR="$ROOT_DIR/src-tauri"
OUT_DIR="$ROOT_DIR/sync-server/src/wasm"

sha256_stdin() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum | awk '{print $1}'
  else
    shasum -a 256 | awk '{print $1}'
  fi
}

sha256_file() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | awk '{print $1}'
  else
    shasum -a 256 "$1" | awk '{print $1}'
  fi
}

if ! rustup target list --installed | grep -qx 'wasm32-unknown-unknown'; then
  echo "Installing rustup target wasm32-unknown-unknown..."
  rustup target add wasm32-unknown-unknown
fi

EXPECTED_WBG_VERSION="$(awk '
  /^\[\[package\]\]/ { in_pkg = 0 }
  /^name = "wasm-bindgen"$/ { in_pkg = 1; next }
  in_pkg && /^version = "/ {
    gsub(/^version = "|"/, "", $0)
    print $0
    exit
  }
' "$TAURI_DIR/Cargo.lock")"

if [[ -z "$EXPECTED_WBG_VERSION" ]]; then
  echo "Could not find wasm-bindgen version in $TAURI_DIR/Cargo.lock" >&2
  exit 1
fi

CURRENT_WBG_VERSION=""
if command -v wasm-bindgen >/dev/null 2>&1; then
  CURRENT_WBG_VERSION="$(wasm-bindgen --version | awk '{print $2}')"
fi

if [[ "$CURRENT_WBG_VERSION" != "$EXPECTED_WBG_VERSION" ]]; then
  echo "Installing wasm-bindgen-cli $EXPECTED_WBG_VERSION (found: '${CURRENT_WBG_VERSION:-none}')..."
  cargo install --locked wasm-bindgen-cli --version "$EXPECTED_WBG_VERSION"
fi

compute_source_hash() {
  local dep_tree
  dep_tree="$(
    cargo tree \
      --locked \
      --manifest-path "$TAURI_DIR/Cargo.toml" \
      -p rem-sync-reduce \
      --features wasm \
      --target wasm32-unknown-unknown \
      --prefix none \
      --edges normal,build \
      --format '{p} [{f}]' \
      | sed -E 's/ \([^)]*\)//g' \
      | LC_ALL=C sort -u
  )"

  (
    cd "$ROOT_DIR"
    printf 'wasm-bindgen=%s\n' "$EXPECTED_WBG_VERSION"
    printf 'script=%s\n' "$(sha256_file "tools/build-sync-wasm.sh")"
    find src-tauri/crates/rem-sync-reduce -type f \
      ! -path '*/target/*' \
      ! -name '.DS_Store' \
      | LC_ALL=C sort \
      | while IFS= read -r rel_path; do
          printf 'file=%s  %s\n' "$(sha256_file "$rel_path")" "$rel_path"
        done
    printf '%s\n' "$dep_tree" \
      | while IFS= read -r dep_line; do
          printf 'dep=%s\n' "$dep_line"
        done
  ) | sha256_stdin
}

SOURCE_HASH="$(compute_source_hash)"

if [[ "$CHECK_ONLY" == "true" ]]; then
  if [[ ! -s "$OUT_DIR/rem_sync_reduce_bg.wasm" ]]; then
    echo "Missing or empty $OUT_DIR/rem_sync_reduce_bg.wasm. Run: bash tools/build-sync-wasm.sh" >&2
    exit 1
  fi

  WASM_MAGIC="$(od -An -tx1 -N4 "$OUT_DIR/rem_sync_reduce_bg.wasm" | tr -d ' \n')"
  if [[ "$WASM_MAGIC" != "0061736d" ]]; then
    echo "Committed $OUT_DIR/rem_sync_reduce_bg.wasm is not a valid WASM binary (magic: $WASM_MAGIC)." >&2
    exit 1
  fi

  COMMITTED_HASH=""
  if [[ -f "$OUT_DIR/source.sha256" ]]; then
    COMMITTED_HASH="$(tr -d '[:space:]' < "$OUT_DIR/source.sha256")"
  fi

  if [[ "$COMMITTED_HASH" != "$SOURCE_HASH" ]]; then
    echo "sync-server/src/wasm is out of date with rem-sync-reduce sources/dependencies." >&2
    echo "  committed source hash: ${COMMITTED_HASH:-<missing>}" >&2
    echo "  current   source hash: $SOURCE_HASH" >&2
    echo "Run 'bash tools/build-sync-wasm.sh' and commit sync-server/src/wasm." >&2
    exit 1
  fi
fi

SYSROOT="$(rustc --print sysroot)"
CARGO_HOME_DIR="${CARGO_HOME:-$HOME/.cargo}"
WASM_RUSTFLAGS="${RUSTFLAGS:+$RUSTFLAGS }--remap-path-prefix=${ROOT_DIR}=/rem --remap-path-prefix=${CARGO_HOME_DIR}=/cargo --remap-path-prefix=${SYSROOT}=/rustc-sysroot"

echo "Building rem-sync-reduce for wasm32-unknown-unknown..."
RUSTFLAGS="$WASM_RUSTFLAGS" cargo build \
  --manifest-path "$TAURI_DIR/Cargo.toml" \
  -p rem-sync-reduce \
  --target wasm32-unknown-unknown \
  --release \
  --features wasm

WASM_INPUT="$TAURI_DIR/target/wasm32-unknown-unknown/release/rem_sync_reduce.wasm"

if [[ "$CHECK_ONLY" == "true" ]]; then
  TMP_OUT_DIR="$(mktemp -d)"
  trap 'rm -rf "$TMP_OUT_DIR"' EXIT

  echo "Running wasm-bindgen -> temp dir for binding verification..."
  wasm-bindgen \
    "$WASM_INPUT" \
    --target web \
    --out-dir "$TMP_OUT_DIR" \
    --out-name rem_sync_reduce
  rm -f "$TMP_OUT_DIR/.gitignore" "$TMP_OUT_DIR/package.json"

  # Compare generated JS/TS bindings byte-for-byte. Raw .wasm binary bytes vary
  # across rustc/LLVM versions and host platforms, so binary freshness is gated
  # by source.sha256 above plus sync-server's vitest-pool-workers test suite.
  for binding_file in rem_sync_reduce.js rem_sync_reduce.d.ts rem_sync_reduce_bg.wasm.d.ts; do
    if [[ ! -f "$OUT_DIR/$binding_file" ]]; then
      echo "Missing committed binding file: $OUT_DIR/$binding_file. Run: bash tools/build-sync-wasm.sh" >&2
      exit 1
    fi
    if ! diff -u "$OUT_DIR/$binding_file" "$TMP_OUT_DIR/$binding_file"; then
      echo "Committed $OUT_DIR/$binding_file differs from freshly generated output. Run: bash tools/build-sync-wasm.sh" >&2
      exit 1
    fi
  done

  EXPECTED_FILES="$(printf '%s\n' \
    rem_sync_reduce.d.ts \
    rem_sync_reduce.js \
    rem_sync_reduce_bg.wasm \
    rem_sync_reduce_bg.wasm.d.ts \
    source.sha256 | LC_ALL=C sort)"
  ACTUAL_FILES="$(cd "$OUT_DIR" && ls -1A | grep -vx '\.DS_Store' | LC_ALL=C sort)"
  if [[ "$ACTUAL_FILES" != "$EXPECTED_FILES" ]]; then
    echo "Unexpected file list in $OUT_DIR:" >&2
    diff -u <(printf '%s\n' "$EXPECTED_FILES") <(printf '%s\n' "$ACTUAL_FILES") || true
    exit 1
  fi

  echo "Sync WASM bundle is fresh (source hash: $SOURCE_HASH)."
  exit 0
fi

mkdir -p "$OUT_DIR"

echo "Running wasm-bindgen -> $OUT_DIR..."
wasm-bindgen \
  "$WASM_INPUT" \
  --target web \
  --out-dir "$OUT_DIR" \
  --out-name rem_sync_reduce

# Remove files generated by wasm-bindgen that are not needed for Workers bundling
# (especially .gitignore containing '*' which would hide the generated bundle from git).
rm -f "$OUT_DIR/.gitignore" "$OUT_DIR/package.json"
printf '%s\n' "$SOURCE_HASH" > "$OUT_DIR/source.sha256"

echo "Generated WASM bundle in $OUT_DIR (source hash: $SOURCE_HASH):"
ls -lh "$OUT_DIR"
