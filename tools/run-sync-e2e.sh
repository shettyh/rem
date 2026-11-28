#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SYNC_SERVER_DIR="$REPO_ROOT/sync-server"
WRANGLER_BIN="$SYNC_SERVER_DIR/node_modules/.bin/wrangler"

DEV_AUTH_TOKEN="${REM_SYNC_E2E_DEV_TOKEN:-local-dev-secret}"
HEALTH_TIMEOUT_SECS="${REM_SYNC_E2E_TIMEOUT_SECS:-60}"

pick_free_port() {
  node -e '
    const net = require("node:net");
    const server = net.createServer();
    server.listen(0, "127.0.0.1", () => {
      const addr = server.address();
      if (!addr || typeof addr === "string") process.exit(1);
      process.stdout.write(String(addr.port));
      server.close();
    });
  '
}

if [[ ! -x "$WRANGLER_BIN" ]]; then
  echo "Installing sync-server dependencies..."
  npm --prefix "$SYNC_SERVER_DIR" ci
fi

PORT="${REM_SYNC_E2E_PORT:-$(pick_free_port)}"
INSPECTOR_PORT="${REM_SYNC_E2E_INSPECTOR_PORT:-$(pick_free_port)}"
while [[ "$INSPECTOR_PORT" == "$PORT" ]]; do
  INSPECTOR_PORT="$(pick_free_port)"
done

PERSIST_DIR="$(mktemp -d "${TMPDIR:-/tmp}/rem-sync-e2e.XXXXXX")"
WRANGLER_LOG="$PERSIST_DIR/wrangler.log"
WRANGLER_PID=""

terminate_process_tree() {
  local pid="$1"
  local signal="$2"
  if command -v pkill >/dev/null 2>&1; then
    pkill "-$signal" -P "$pid" 2>/dev/null || true
  fi
  kill "-$signal" "$pid" 2>/dev/null || true
}

cleanup() {
  local exit_code=$?
  trap - EXIT INT TERM

  if [[ -n "$WRANGLER_PID" ]] && kill -0 "$WRANGLER_PID" 2>/dev/null; then
    terminate_process_tree "$WRANGLER_PID" TERM
    local waited=0
    while kill -0 "$WRANGLER_PID" 2>/dev/null && (( waited < 20 )); do
      sleep 0.25
      waited=$((waited + 1))
    done
    if kill -0 "$WRANGLER_PID" 2>/dev/null; then
      terminate_process_tree "$WRANGLER_PID" KILL
    fi
    wait "$WRANGLER_PID" 2>/dev/null || true
  fi

  if [[ $exit_code -ne 0 && -f "$WRANGLER_LOG" ]]; then
    echo "--- wrangler dev log ($WRANGLER_LOG) ---" >&2
    cat "$WRANGLER_LOG" >&2
    echo "--- end wrangler dev log ---" >&2
  fi

  rm -rf "$PERSIST_DIR"
  exit "$exit_code"
}
trap cleanup EXIT INT TERM

export CI=true
export WRANGLER_SEND_METRICS=false

echo "Applying local D1 migrations (persist: $PERSIST_DIR)..."
(
  cd "$SYNC_SERVER_DIR"
  "$WRANGLER_BIN" d1 migrations apply rem-sync-auth \
    --local \
    --force \
    --persist-to "$PERSIST_DIR"
)

echo "Starting local sync-server on http://127.0.0.1:$PORT ..."
(
  cd "$SYNC_SERVER_DIR"
  exec "$WRANGLER_BIN" dev \
    --local \
    --ip 127.0.0.1 \
    --port "$PORT" \
    --inspector-port "$INSPECTOR_PORT" \
    --persist-to "$PERSIST_DIR" \
    --var "DEV_AUTH_TOKEN:$DEV_AUTH_TOKEN" \
    --show-interactive-dev-session=false
) < /dev/null > "$WRANGLER_LOG" 2>&1 &
WRANGLER_PID=$!

HEALTH_URL="http://127.0.0.1:$PORT/health"
echo "Waiting for $HEALTH_URL (timeout: ${HEALTH_TIMEOUT_SECS}s)..."
deadline=$((SECONDS + HEALTH_TIMEOUT_SECS))
until curl -fsS "$HEALTH_URL" >/dev/null 2>&1; do
  if ! kill -0 "$WRANGLER_PID" 2>/dev/null; then
    echo "ERROR: wrangler dev exited prematurely before /health became ready." >&2
    exit 1
  fi
  if (( SECONDS >= deadline )); then
    echo "ERROR: timed out after ${HEALTH_TIMEOUT_SECS}s waiting for $HEALTH_URL." >&2
    exit 1
  fi
  sleep 0.5
done
echo "sync-server is healthy at $HEALTH_URL"

REM_SYNC_E2E_REQUIRE_LIVE=1 \
REM_SYNC_TEST_URL="http://127.0.0.1:$PORT" \
REM_SYNC_TEST_TOKEN="${DEV_AUTH_TOKEN}:e2e-user" \
cargo test \
  --manifest-path "$REPO_ROOT/src-tauri/Cargo.toml" \
  -p rem-sync \
  --test live_server_e2e \
  -- --nocapture
