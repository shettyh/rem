#!/usr/bin/env bash
# Submit a macOS artifact (.zip or .dmg) to Apple's notarization service and
# fail with Apple's diagnostic log if status is not Accepted.
set -euo pipefail

if [ "$#" -ne 1 ]; then
  echo "usage: $0 <artifact-path>" >&2
  exit 2
fi

artifact="$1"
: "${APPLE_API_KEY_PATH:?APPLE_API_KEY_PATH is required}"
: "${APPLE_API_KEY:?APPLE_API_KEY is required}"
: "${APPLE_API_ISSUER:?APPLE_API_ISSUER is required}"

result=$(xcrun notarytool submit "$artifact" \
  --key "$APPLE_API_KEY_PATH" \
  --key-id "$APPLE_API_KEY" \
  --issuer "$APPLE_API_ISSUER" \
  --output-format json \
  --wait)

status=$(printf '%s' "$result" | jq -r '.status // empty')
submission_id=$(printf '%s' "$result" | jq -r '.id // empty')

echo "Notarization result for $(basename "$artifact"): status=${status:-unknown} id=${submission_id:-unknown}"

if [ "$status" != "Accepted" ]; then
  if [ -n "$submission_id" ]; then
    echo "::group::Apple notarization log ($submission_id)"
    xcrun notarytool log "$submission_id" \
      --key "$APPLE_API_KEY_PATH" \
      --key-id "$APPLE_API_KEY" \
      --issuer "$APPLE_API_ISSUER" || true
    echo "::endgroup::"
  fi
  echo "::error::Apple notarization failed for $artifact (status=${status:-unknown})"
  exit 1
fi
