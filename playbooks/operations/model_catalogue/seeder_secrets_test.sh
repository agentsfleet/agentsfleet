#!/usr/bin/env bash
# A refused native database connection must not print its password or URL.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
readonly REPO_ROOT
readonly SEEDER="$REPO_ROOT/scripts/seed-models.mjs"
# A password no real output contains, so finding it anywhere is the leak.
readonly SENTINEL="sentinel-password-$$"
readonly FAILURE="catalogue refresh failed"

passed=0
failed=0

ok() {
  passed=$((passed + 1))
  echo "  ✓ $1"
}

bad() {
  failed=$((failed + 1))
  echo "  ✗ $1"
}

test_seeder_failure_hides_the_database_url() {
  local name="seeder failure hides the database URL"
  local out status=0
  out="$(DATABASE_URL="postgres://seeder:${SENTINEL}@127.0.0.1:1/agentsfleet" \
    bun "$SEEDER" 2>&1 </dev/null)" || status=$?
  if [ "$status" -eq 0 ]; then
    bad "$name: the seeder succeeded against a refused connection"
  elif [[ "$out" == *"$SENTINEL"* ]] || [[ "$out" == *"postgres://"* ]]; then
    bad "$name: the output repeats database connection credentials"
  elif [[ "$out" != *"$FAILURE"* ]]; then
    bad "$name: the failure has no useful operation status"
  else
    ok "$name"
  fi
}

command -v bun >/dev/null 2>&1 || {
  echo "✗ bun is required to run the seeder" >&2
  exit 1
}

test_seeder_failure_hides_the_database_url

printf '\n%d passed, %d failed\n' "$passed" "$failed"
[ "$failed" -eq 0 ]
