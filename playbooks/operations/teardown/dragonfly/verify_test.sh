#!/usr/bin/env bash
#
# Runs the Dragonfly teardown's verify step against a stubbed cache.
#
# The teardown is `FLUSHALL` (02_teardown.sh) and the verification is
# `DBSIZE == 0` (03_verify.sh), both blind to key names, so no key name is
# asserted here. What the operator acts on is the verdict and the restart
# instruction: a flush destroys the `fleet_lease` consumer group while running
# agentsfleetd processes still hold their cursors.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
readonly VERIFY="$SCRIPT_DIR/03_verify.sh"
readonly CACHE_URL="redis://default:stub-password@dragonfly.example.test:6379"
readonly RESTART_INSTRUCTION="restart or redeploy every agentsfleetd machine"
readonly EMPTY_VERDICT="SUCCESS: cache is empty"

passed=0
failed=0
work_dir="$(mktemp -d)"
stub_dir="$work_dir/bin"
calls="$work_dir/calls"
mkdir -p "$stub_dir"
trap 'rm -rf -- "$work_dir"' EXIT

ok() {
  printf 'ok   %s\n' "$1"
  passed=$((passed + 1))
}

bad() {
  printf 'FAIL %s\n       %s\n' "$1" "$2" >&2
  failed=$((failed + 1))
}

cat >"$stub_dir/op" <<'STUB'
#!/usr/bin/env bash
case "${1:-}" in
  whoami) printf 'stub-user\n' ;;
  read) printf '%s\n' "$STUB_CACHE_URL" ;;
  *) exit 1 ;;
esac
STUB

# Runs the container's command on this host, so the redis-cli stub below
# answers the script the real image would run.
cat >"$stub_dir/docker" <<'STUB'
#!/usr/bin/env bash
while [ "$#" -gt 0 ] && [ "$1" != "redis:7-alpine" ]; do shift; done
shift
exec "$@"
STUB

cat >"$stub_dir/redis-cli" <<'STUB'
#!/usr/bin/env bash
printf '%s\n' "$*" >>"$CALLS"
printf '%s\n' "$STUB_DBSIZE"
STUB
chmod +x "$stub_dir/op" "$stub_dir/docker" "$stub_dir/redis-cli"

run_verify() {
  local dbsize="$1"
  : >"$calls"
  env PATH="$stub_dir:$PATH" CALLS="$calls" STUB_CACHE_URL="$CACHE_URL" \
    STUB_DBSIZE="$dbsize" ALLOW_VAULT_READS=1 ENV=dev bash "$VERIFY" 2>&1
}

test_verify_passes_an_empty_cache_and_asks_for_a_restart() {
  local name="verify passes an empty cache and asks for a restart"
  local out status=0
  out="$(run_verify 0)" || status=$?
  if [ "$status" -ne 0 ]; then
    bad "$name" "status=$status: $out"
  elif ! grep -Fqx -- "-u $CACHE_URL DBSIZE" "$calls"; then
    bad "$name" "DBSIZE was not asked of the vault's cache: $(cat "$calls")"
  elif [[ "$out" != *"$EMPTY_VERDICT"* ]]; then
    bad "$name" "no empty-cache verdict: $out"
  elif [[ "$out" != *"$RESTART_INSTRUCTION"* ]]; then
    bad "$name" "the operator is not told to restart agentsfleetd: $out"
  else
    ok "$name"
  fi
}

test_verify_fails_when_keys_remain() {
  local name="verify fails when keys remain"
  local out status=0
  out="$(run_verify 7)" || status=$?
  if [ "$status" -eq 0 ]; then
    bad "$name" "a cache holding 7 keys passed: $out"
  elif [[ "$out" == *"$EMPTY_VERDICT"* ]]; then
    bad "$name" "a cache holding 7 keys was reported empty: $out"
  else
    ok "$name"
  fi
}

test_verify_passes_an_empty_cache_and_asks_for_a_restart
test_verify_fails_when_keys_remain

printf '\n%d passed, %d failed\n' "$passed" "$failed"
[ "$failed" -eq 0 ]
