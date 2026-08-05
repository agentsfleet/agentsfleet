#!/usr/bin/env bash
#
# Guards the stop-the-writer precondition shared by both teardown playbooks.
#
# The failure it prevents is specific and expensive: a live agentsfleetd that
# Fly.io restarts against a just-emptied datastore re-applies its OWN older
# migration list, the next deploy then fails `ensureCanonical` with
# `error.MigrationSchemaAhead`, and the whole teardown has to be run again.
#
# Both gates are checked for DISPATCH ORDER rather than mere presence: a
# stop-writers step that runs after the destructive step prevents nothing, and
# "the file mentions it" is not the claim.
#
# flyctl is stubbed on PATH so the verification logic is exercised without a
# real Fly account and without any chance of scaling something real.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
STEP="$SCRIPT_DIR/stop_writers.sh"
DB_GATE="$SCRIPT_DIR/../operations/teardown/database/00_gate.sh"
REDIS_GATE="$SCRIPT_DIR/../operations/teardown/redis/00_gate.sh"

passed=0
failed=0

# ONE temp root torn down on exit. Deliberately not a per-test `trap RETURN`:
# a RETURN trap fires when any nested function returns, so `make_sandbox`
# returning deleted the sandbox before the step under test could run — and the
# assertions then passed for the wrong reason (a missing flyctl stub also makes
# the step exit non-zero).
TMPROOT="$(mktemp -d)"
trap 'rm -rf "$TMPROOT"' EXIT

sandbox_dir() {
  local dir="$TMPROOT/$1"
  mkdir -p "$dir"
  printf '%s' "$dir"
}

ok() {
  printf 'ok   %s\n' "$1"
  passed=$((passed + 1))
}

bad() {
  printf 'FAIL %s\n       %s\n' "$1" "$2" >&2
  failed=$((failed + 1))
}

# A sandbox whose PATH shadows flyctl and op. MACHINE_COUNT drives how many
# machines the stubbed `machine list` reports back.
make_sandbox() {
  local dir="$1"
  local machine_count="$2"
  local status_exit="${3:-0}"
  mkdir -p "$dir/bin"

  cat >"$dir/bin/flyctl" <<STUB
#!/usr/bin/env bash
case "\$1" in
  status) exit $status_exit ;;
  scale) exit 0 ;;
  machine)
    # Counter loop, not \`seq\`: BSD seq (macOS) counts DOWN for \`seq 1 0\` and
    # emits "1 0", so the zero-machine stub reported two machines and the
    # zero-machine assertion failed against a correct script.
    payload=""
    _i=0
    while [ "\$_i" -lt $machine_count ]; do
      payload="\${payload}{\"id\":\"m\"}"
      _i=\$((_i + 1))
    done
    printf '[%s]\n' "\$payload"
    ;;
esac
exit 0
STUB

  cat >"$dir/bin/op" <<'STUB'
#!/usr/bin/env bash
case "$1" in
  whoami) exit 0 ;;
  read) printf 'stub-token\n' ;;
esac
exit 0
STUB

  chmod +x "$dir/bin/flyctl" "$dir/bin/op"
}

run_step() {
  local dir="$1"
  local confirm="${2:-DEV}"
  PATH="$dir/bin:$PATH" \
    ENV=dev \
    ALLOW_VAULT_READS=1 \
    VERIFY_SLEEP_SECONDS=0 \
    "$STEP" <<<"$confirm" 2>&1
}

test_stop_writers_verifies_zero_machines() {
  local name="stop_writers_verifies_zero_machines"
  local dir
  dir="$(sandbox_dir "$name")"
  # Scale reports success, but a machine lingers — the step must NOT trust the
  # command's exit status.
  make_sandbox "$dir" 1
  if run_step "$dir" >/dev/null 2>&1; then
    bad "$name" "step passed while a machine was still running"
  else
    ok "$name"
  fi
}

test_stop_writers_reaches_zero() {
  local name="stop_writers_reaches_zero"
  local dir
  dir="$(sandbox_dir "$name")"
  make_sandbox "$dir" 0
  if run_step "$dir" >/dev/null 2>&1; then
    ok "$name"
  else
    bad "$name" "step failed even though zero machines were reported"
  fi
}

test_stop_writers_is_idempotent() {
  local name="stop_writers_is_idempotent"
  local dir
  dir="$(sandbox_dir "$name")"
  # `flyctl status` non-zero == the app does not exist. A first-time teardown
  # must not be blocked by a missing app.
  make_sandbox "$dir" 0 1
  if run_step "$dir" >/dev/null 2>&1; then
    ok "$name"
  else
    bad "$name" "step failed for an app that does not exist"
  fi
}

test_stop_writers_refuses_without_confirmation() {
  local name="stop_writers_refuses_without_confirmation"
  local dir out
  dir="$(sandbox_dir "$name")"
  make_sandbox "$dir" 0
  # Scaling to zero is an outage and runs BEFORE 02_teardown's own prompt, so a
  # wrong answer must scale nothing at all — otherwise invoking the gate to read
  # that later prompt would already have taken the environment down.
  out="$(run_step "$dir" "nope" || true)"
  if printf '%s' "$out" | grep -q 'Scaling'; then
    bad "$name" "the app was scaled despite a failed confirmation"
  elif ! printf '%s' "$out" | grep -q 'Confirmation failed'; then
    bad "$name" "no confirmation was requested before scaling"
  else
    ok "$name"
  fi
}

test_stop_writers_rejects_unknown_env() {
  local name="stop_writers_rejects_unknown_env"
  local dir
  dir="$(sandbox_dir "$name")"
  make_sandbox "$dir" 0
  if PATH="$dir/bin:$PATH" ENV=staging ALLOW_VAULT_READS=1 "$STEP" >/dev/null 2>&1; then
    bad "$name" "step accepted ENV=staging"
  else
    ok "$name"
  fi
}

# The ordering claim: stop-writers must be dispatched BEFORE the credential
# check in both gates. A step that runs after the destructive one prevents
# nothing, so presence alone is not the assertion.
assert_dispatch_order() {
  local name="$1"
  local gate="$2"
  local stop_line credential_line
  stop_line="$(grep -n 'stop_writers.sh' "$gate" | grep 'run_step' | head -1 | cut -d: -f1)"
  credential_line="$(grep -n '01_credential_check.sh' "$gate" | grep 'run_step' | head -1 | cut -d: -f1)"
  if [ -z "$stop_line" ]; then
    bad "$name" "$gate does not dispatch stop_writers.sh"
  elif [ -z "$credential_line" ]; then
    bad "$name" "$gate does not dispatch 01_credential_check.sh"
  elif [ "$stop_line" -lt "$credential_line" ]; then
    ok "$name"
  else
    bad "$name" "stop_writers.sh is dispatched after the credential check in $gate"
  fi
}

test_teardown_gates_dispatch_stop_writers_first() {
  assert_dispatch_order "database_gate_dispatches_stop_writers_first" "$DB_GATE"
  assert_dispatch_order "redis_gate_dispatches_stop_writers_first" "$REDIS_GATE"
}

test_scripts_print_no_credentials() {
  local name="scripts_print_no_credentials"
  local dir output
  dir="$(sandbox_dir "$name")"
  make_sandbox "$dir" 0
  output="$(run_step "$dir" || true)"
  # The stubbed vault returns `stub-token`; it must never be echoed.
  if printf '%s' "$output" | grep -q 'stub-token'; then
    bad "$name" "the step printed a vault value"
  else
    ok "$name"
  fi
}

test_stop_writers_verifies_zero_machines
test_stop_writers_reaches_zero
test_stop_writers_is_idempotent
test_stop_writers_refuses_without_confirmation() {
  local name="stop_writers_refuses_without_confirmation"
  local dir out
  dir="$(sandbox_dir "$name")"
  make_sandbox "$dir" 0
  # Scaling to zero is an outage and runs BEFORE 02_teardown's own prompt, so a
  # wrong answer must scale nothing at all — otherwise invoking the gate to read
  # that later prompt would already have taken the environment down.
  out="$(run_step "$dir" "nope" || true)"
  if printf '%s' "$out" | grep -q 'Scaling'; then
    bad "$name" "the app was scaled despite a failed confirmation"
  elif ! printf '%s' "$out" | grep -q 'Confirmation failed'; then
    bad "$name" "no confirmation was requested before scaling"
  else
    ok "$name"
  fi
}

test_stop_writers_rejects_unknown_env
test_stop_writers_refuses_without_confirmation
test_teardown_gates_dispatch_stop_writers_first
test_scripts_print_no_credentials

printf '\n%d passed, %d failed\n' "$passed" "$failed"
[ "$failed" -eq 0 ]
