#!/usr/bin/env bash
# Self-tests for the vault approval + auth gates on the credential-rotation scripts.
#
#     bash playbooks/operations/credential_rotation/vault_gate_test.sh
#
# The filename deliberately avoids the 0[1-9]_*.sh / [1-9][0-9]_*.sh shape that
# 00_gate.sh globs (RULE GLS): a test that the gate dispatcher executed as a
# rotation step would run the tests against production during a real rotation.
#
# `op` and `curl` are stubbed onto PATH. Every stub invocation is appended to a
# log, so a test can assert the script exited before it ever reached the vault.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Both scripts read the vault and so must carry both gates.
GATED_SCRIPTS=("$SCRIPT_DIR/01_vault_sync.sh" "$SCRIPT_DIR/02_service_health.sh")
readonly GATED_SCRIPTS

readonly APPROVAL_HINT="ALLOW_VAULT_READS"
readonly OP_AUTH_HINT="not authenticated"

passed=0
failed=0

ok()  { printf 'ok   %s\n' "$1"; passed=$((passed + 1)); }
bad() { printf 'FAIL %s\n       %s\n' "$1" "$2" >&2; failed=$((failed + 1)); }

WORK_DIR="$(mktemp -d)"
readonly WORK_DIR
readonly STUB_DIR="$WORK_DIR/bin"
readonly OP_CALLS="$WORK_DIR/op-calls.log"
mkdir -p "$STUB_DIR"
cleanup() { rm -rf "$WORK_DIR"; }
trap cleanup EXIT

# Records the full argv of every call so vault-read reachability is assertable
# even when `read` is not $1 (`op --account X read …`). `whoami` honours
# OP_WHOAMI_STATUS so a test can simulate an expired session. The subcommand is
# whichever argument is not a global flag or its value.
cat >"$STUB_DIR/op" <<'STUB'
#!/usr/bin/env bash
printf '%s\n' "$*" >>"$OP_CALLS"
for arg in "$@"; do
  case "$arg" in
    whoami) exit "${OP_WHOAMI_STATUS:-0}" ;;
    read)   printf 'stub-secret\n'; exit 0 ;;
  esac
done
exit 0
STUB

# Fails fast: no test should ever reach a network call, and a real request from a
# CI runner would be a silent dependency on api-dev being up.
cat >"$STUB_DIR/curl" <<'STUB'
#!/usr/bin/env bash
printf 'curl\n' >>"$OP_CALLS"
exit 1
STUB

chmod +x "$STUB_DIR/op" "$STUB_DIR/curl"

# Runs `script` with the stubs in front of PATH. Echoes combined output; returns
# the script's exit status. `extra_env` entries are passed through to `env`.
run_gated_script() {
  local script="$1"; shift
  : >"$OP_CALLS"
  env -u ALLOW_VAULT_READS \
    PATH="$STUB_DIR:$PATH" \
    OP_CALLS="$OP_CALLS" \
    "$@" \
    bash "$script" 2>&1
}

# A vault read reached the stub iff any logged argv contains `read` as a word —
# `op read …` and `op --account X read …` both match; `op whoami` does not.
op_read_was_called() { grep -qE '(^| )read( |$)' "$OP_CALLS" 2>/dev/null; }

# ── Dimension 3.1 — no approval, no vault ────────────────────────────────────

test_credential_rotation_blocks_without_approval() {
  local name="test_credential_rotation_blocks_without_approval"
  local script output status

  for script in "${GATED_SCRIPTS[@]}"; do
    status=0
    output="$(run_gated_script "$script")" || status=$?

    if [[ "$status" -eq 0 ]]; then
      bad "$name" "$(basename "$script") exited 0 with ALLOW_VAULT_READS unset"
      return
    fi
    if [[ "$output" != *"$APPROVAL_HINT"* ]]; then
      bad "$name" "$(basename "$script") failed without naming $APPROVAL_HINT: $output"
      return
    fi
    if op_read_was_called; then
      bad "$name" "$(basename "$script") reached the vault read before the approval gate"
      return
    fi
  done
  ok "$name"
}

# ── Dimension 3.2 — approval without an authenticated op ─────────────────────

test_credential_rotation_requires_op_auth() {
  local name="test_credential_rotation_requires_op_auth"
  local script output status

  for script in "${GATED_SCRIPTS[@]}"; do
    status=0
    output="$(run_gated_script "$script" ALLOW_VAULT_READS=1 OP_WHOAMI_STATUS=1)" || status=$?

    if [[ "$status" -eq 0 ]]; then
      bad "$name" "$(basename "$script") exited 0 while op was unauthenticated"
      return
    fi
    if [[ "$output" != *"$OP_AUTH_HINT"* ]]; then
      bad "$name" "$(basename "$script") failed without the sign-in hint: $output"
      return
    fi
    if op_read_was_called; then
      bad "$name" "$(basename "$script") reached the vault read with op unauthenticated"
      return
    fi
  done
  ok "$name"
}

# ── Positive path — the gates permit, not just deny ──────────────────────────

# The two deny tests both pass against a script that ALWAYS blocks (e.g. an
# inverted gate, or an unconditional exit after the preamble). This proves the
# gates let an approved + authenticated run through to the vault read.
test_credential_rotation_allows_when_approved_and_authed() {
  local name="test_credential_rotation_allows_when_approved_and_authed"
  local script

  for script in "${GATED_SCRIPTS[@]}"; do
    run_gated_script "$script" ALLOW_VAULT_READS=1 OP_WHOAMI_STATUS=0 >/dev/null 2>&1 || true
    if ! op_read_was_called; then
      bad "$name" "$(basename "$script") never reached the vault read with approval + auth present — a gate blocks the happy path"
      return
    fi
  done
  ok "$name"
}

test_credential_rotation_blocks_without_approval
test_credential_rotation_requires_op_auth
test_credential_rotation_allows_when_approved_and_authed

printf '\n%d passed, %d failed\n' "$passed" "$failed"
[[ "$failed" -eq 0 ]]
