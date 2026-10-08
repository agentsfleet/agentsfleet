#!/usr/bin/env bash
# The seeder's failure messages carry psql's own output and never its command
# line. Node's execFileSync puts the full argument list in its error message,
# and the seeder hands psql the database URL as an argument, password and all,
# so a seeder that printed that message would leak the password to whatever
# reads the playbook's output.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
readonly REPO_ROOT
readonly SEEDER="$REPO_ROOT/scripts/seed-models.mjs"
# A password no real output contains, so finding it anywhere is the leak.
readonly SENTINEL="sentinel-password-$$"
# What the stubbed psql says before it fails, which the seeder must keep.
readonly PSQL_SAYS="psql: error: connection refused"

passed=0
failed=0
WORK="$(mktemp -d)"
readonly WORK
trap 'rm -rf "$WORK"' EXIT

ok() {
  passed=$((passed + 1))
  echo "  ✓ $1"
}

bad() {
  failed=$((failed + 1))
  echo "  ✗ $1"
}

# A psql that refuses every connection, first on PATH.
mkdir "$WORK/bin"
printf '#!/bin/sh\necho "%s" >&2\nexit 2\n' "$PSQL_SAYS" >"$WORK/bin/psql"
chmod +x "$WORK/bin/psql"

test_seeder_failure_hides_the_database_url() {
  local name="seeder failure hides the database URL"
  local out status=0
  out="$(env -u SEED_PSQL -u APPLY \
    DATABASE_URL="postgres://seeder:${SENTINEL}@db.invalid:5432/agentsfleet" \
    PATH="$WORK/bin:$PATH" node "$SEEDER" 2>&1 </dev/null)" || status=$?
  if [ "$status" -eq 0 ]; then
    bad "$name: the seeder succeeded against a psql that refuses"
  elif [[ "$out" == *"$SENTINEL"* ]]; then
    bad "$name: the output repeats the database password"
  elif [[ "$out" != *"$PSQL_SAYS"* ]]; then
    bad "$name: the output drops psql's own message"
  else
    ok "$name"
  fi
}

command -v node >/dev/null 2>&1 || {
  echo "✗ node is required to run the seeder" >&2
  exit 1
}

test_seeder_failure_hides_the_database_url

printf '\n%d passed, %d failed\n' "$passed" "$failed"
[ "$failed" -eq 0 ]
