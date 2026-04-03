#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
REPO_ROOT=$(cd "$SCRIPT_DIR/.." && pwd)

curl() {
  printf '%s\n' "$*"
}

AZURE_PAT="dummy"

# shellcheck source=/dev/null
source "$REPO_ROOT/src/bin/create-test-card"

payload=$(mktemp)
printf '[]' > "$payload"

result=$(azure_patch_json "https://example.invalid" "$payload")
rm -f "$payload"

[[ "$result" == *'-X PATCH'* ]]

message=$(extract_azure_error_message '{"count":1,"value":{"Message":"The requested resource does not support http method POST."}}')
[[ "$message" == *'does not support http method POST'* ]]

echo "ok"
