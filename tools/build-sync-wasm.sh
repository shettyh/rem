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

# Generate a deterministic, OS- and toolchain-independent manifest of all inputs
# that affect rem-sync-reduce's WASM output:
#   - wasm-bindgen version from Cargo.lock
#   - tools/build-sync-wasm.sh
#   - all source files under src-tauri/crates/rem-sync-reduce/ (normalizing only
#     rem-sync-reduce's own [package] version line in Cargo.toml so Release Please
#     version bumps do not spuriously invalidate the committed WASM bundle)
#   - transitive package closure of rem-sync-reduce from src-tauri/Cargo.lock
#     (normalizing only rem-sync-reduce's own local version)
generate_source_manifest() {
  python3 - "$ROOT_DIR" "$EXPECTED_WBG_VERSION" <<'PY'
import hashlib
import os
import re
import sys

root_dir = sys.argv[1]
wbg_version = sys.argv[2]

def sha256_file(path: str) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(65536), b""):
            h.update(chunk)
    return h.hexdigest()

def sha256_normalized_crate_manifest(path: str) -> str:
    """Hash rem-sync-reduce/Cargo.toml while normalizing only [package].version."""
    h = hashlib.sha256()
    section = None
    with open(path, "r", encoding="utf-8", newline="") as f:
        for line in f:
            stripped = line.strip()
            header_match = re.match(r"^\[([^\]]+)\]\s*(?:#.*)?$", stripped)
            if header_match:
                section = header_match.group(1).strip()
            elif section == "package" and re.match(r'^version\s*=\s*"[^"]*"\s*(?:#.*)?$', stripped):
                line = re.sub(
                    r'^(\s*version\s*=\s*)"[^"]*"',
                    r'\1"<normalized>"',
                    line,
                    count=1,
                )
            h.update(line.encode("utf-8"))
    return h.hexdigest()

# Parse [[package]] blocks from src-tauri/Cargo.lock (pure stdlib, Python 3.8+)
lock_path = os.path.join(root_dir, "src-tauri", "Cargo.lock")
packages = []
current = None
in_deps = False

with open(lock_path, "r", encoding="utf-8") as f:
    for raw_line in f:
        line = raw_line.strip()
        if line == "[[package]]":
            if current is not None:
                packages.append(current)
            current = {"name": "", "version": "", "checksum": "local", "dependencies": []}
            in_deps = False
            continue
        if current is None:
            continue
        if in_deps:
            if line == "]":
                in_deps = False
                continue
            m = re.match(r'^"([^"]+)",?$', line)
            if m:
                current["dependencies"].append(m.group(1))
            continue
        if line == "dependencies = [":
            in_deps = True
            continue
        for key in ("name", "version", "checksum"):
            m = re.match(rf'^{key} = "([^"]+)"$', line)
            if m:
                current[key] = m.group(1)

if current is not None:
    packages.append(current)

by_name = {}
by_name_ver = {}
for pkg in packages:
    by_name.setdefault(pkg["name"], []).append(pkg)
    by_name_ver[(pkg["name"], pkg["version"])] = pkg

roots = by_name.get("rem-sync-reduce", [])
if len(roots) != 1:
    sys.stderr.write(f"Expected 1 rem-sync-reduce package in Cargo.lock, found {len(roots)}\n")
    sys.exit(1)

visited = {}
queue = [roots[0]]
while queue:
    pkg = queue.pop(0)
    key = (pkg["name"], pkg["version"])
    if key in visited:
        continue
    visited[key] = pkg
    for dep_spec in pkg["dependencies"]:
        parts = dep_spec.split()
        dep_name = parts[0]
        dep_ver = parts[1] if len(parts) >= 2 else None
        if dep_ver is not None:
            target = by_name_ver.get((dep_name, dep_ver))
            if target is not None:
                queue.append(target)
        else:
            for target in by_name.get(dep_name, []):
                queue.append(target)

lines = [f"wasm-bindgen={wbg_version}"]
script_rel = "tools/build-sync-wasm.sh"
lines.append(f"script={sha256_file(os.path.join(root_dir, script_rel))}  {script_rel}")

crate_root = os.path.join(root_dir, "src-tauri", "crates", "rem-sync-reduce")
source_files = []
for dirpath, dirnames, filenames in os.walk(crate_root):
    dirnames[:] = sorted(d for d in dirnames if d != "target")
    for fname in sorted(filenames):
        if fname == ".DS_Store":
            continue
        abs_path = os.path.join(dirpath, fname)
        rel_path = os.path.relpath(abs_path, root_dir).replace(os.sep, "/")
        source_files.append((rel_path, abs_path))

crate_manifest_rel = "src-tauri/crates/rem-sync-reduce/Cargo.toml"
for rel_path, abs_path in sorted(source_files):
    digest = (
        sha256_normalized_crate_manifest(abs_path)
        if rel_path == crate_manifest_rel
        else sha256_file(abs_path)
    )
    lines.append(f"file={digest}  {rel_path}")

for (name, version) in sorted(visited.keys()):
    checksum = visited[(name, version)]["checksum"]
    if name == "rem-sync-reduce" and checksum == "local":
        lines.append(f"pkg={name} <normalized> {checksum}")
    else:
        lines.append(f"pkg={name} {version} {checksum}")

sys.stdout.write("\n".join(lines) + "\n")
PY
}

TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

CURRENT_MANIFEST="$TMP_DIR/source-manifest.txt"
generate_source_manifest > "$CURRENT_MANIFEST"

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

  if [[ ! -f "$OUT_DIR/source-manifest.txt" ]]; then
    echo "Missing $OUT_DIR/source-manifest.txt. Run 'bash tools/build-sync-wasm.sh' and commit sync-server/src/wasm." >&2
    exit 1
  fi

  if ! diff -u "$OUT_DIR/source-manifest.txt" "$CURRENT_MANIFEST"; then
    echo "sync-server/src/wasm is out of date with rem-sync-reduce sources/dependencies." >&2
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
  TMP_OUT_DIR="$TMP_DIR/out"
  mkdir -p "$TMP_OUT_DIR"

  echo "Running wasm-bindgen -> temp dir for binding verification..."
  wasm-bindgen \
    "$WASM_INPUT" \
    --target web \
    --out-dir "$TMP_OUT_DIR" \
    --out-name rem_sync_reduce
  rm -f "$TMP_OUT_DIR/.gitignore" "$TMP_OUT_DIR/package.json"

  # Compare generated JS/TS bindings byte-for-byte. Raw .wasm binary bytes vary
  # across rustc/LLVM versions and host platforms, so binary freshness is gated
  # by source-manifest.txt above plus sync-server's @cloudflare/vitest-plugin suite.
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
    source-manifest.txt | LC_ALL=C sort)"
  ACTUAL_FILES="$(cd "$OUT_DIR" && ls -1A | grep -vx '\.DS_Store' | LC_ALL=C sort)"
  if [[ "$ACTUAL_FILES" != "$EXPECTED_FILES" ]]; then
    echo "Unexpected file list in $OUT_DIR:" >&2
    diff -u <(printf '%s\n' "$EXPECTED_FILES") <(printf '%s\n' "$ACTUAL_FILES") || true
    exit 1
  fi

  echo "Sync WASM bundle is fresh."
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
rm -f "$OUT_DIR/.gitignore" "$OUT_DIR/package.json" "$OUT_DIR/source.sha256"
cp "$CURRENT_MANIFEST" "$OUT_DIR/source-manifest.txt"

echo "Generated WASM bundle in $OUT_DIR:"
ls -lh "$OUT_DIR"
