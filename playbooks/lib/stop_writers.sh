#!/usr/bin/env bash
# stop_writers.sh - Scale the environment's agentsfleetd to zero machines.
#
# Shared implementation for BOTH teardown playbooks (database and redis). It
# lives in playbooks/lib/ — the established home for shared playbook code —
# and each teardown directory carries a thin `stop_writers.sh` that sources it.
#
# That split is not incidental: `operations/explicit_dispatch_test.sh` copies a
# gate into a temp directory and stubs each step BESIDE it, so a gate that
# reaches outside its own directory for a step cannot be dispatch-tested at all.
# One implementation, two thin call sites, no duplication and no cross-directory
# dispatch.
#
# Why this is an executable step and not a sentence in a runbook:
# a live agentsfleetd machine that Fly.io restarts against a just-emptied
# database re-applies its OWN older migration list. The next deployment then
# reads applied versions that the new binary's canonical list does not contain,
# `ensureCanonical` refuses with error.MigrationSchemaAhead, and the teardown
# has to be run a second time. Both playbooks named the precondition and gave
# no command for it, so it was reliably skipped.
#
# The command is not trusted to have worked. `flyctl scale count 0` can report
# success while a machine lingers, so the machine count is READ BACK and a
# non-zero count fails the step, halting the gate before anything destructive.
#
# Idempotent by design — an already-stopped app and an app that does not exist
# are both passes, so a first-time teardown is not blocked by a missing app.
#
# Required environment:
#   ENV=dev|prod   Target environment (inherited from the calling gate)
#
# Exit: 0 zero machines running (including already-stopped / absent)
#       1 could not reach zero

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=./common.sh
source "${SCRIPT_DIR}/common.sh"

# Reads the vault for the Fly token, so it carries the same approval + auth
# gates as every other script under playbooks/operations/.
playbooks_require_vault_read_approval
playbooks_require_op_auth

readonly FLY_TOKEN_FIELD="fly-api-token/credential"
readonly PROCESS_GROUP="app"
# Overridable only so the regression test can exercise the give-up path without
# spending a minute asleep inside the lint gate. Operators never set these.
readonly VERIFY_ATTEMPTS="${VERIFY_ATTEMPTS:-12}"
readonly VERIFY_SLEEP_SECONDS="${VERIFY_SLEEP_SECONDS:-5}"

echo ""
echo "== teardown precondition: stop every writer =="
echo ""

env_mode="${ENV:-}"
case "$env_mode" in
  dev)
    vault="${VAULT_DEV:-ZMB_CD_DEV}"
    app="${FLY_APP_DEV:-agentsfleetd-dev}"
    ;;
  prod)
    vault="${VAULT_PROD:-ZMB_CD_PROD}"
    app="${FLY_APP_PROD:-agentsfleetd-prod}"
    ;;
  *)
    echo "ERROR: ENV must be 'dev' or 'prod' (destructive operations require explicit targeting)" >&2
    exit 1
    ;;
esac

playbooks_require_tool flyctl

FLY_API_TOKEN="$(playbooks_read_ref_or_empty "op://${vault}/${FLY_TOKEN_FIELD}")"
if [ -z "$FLY_API_TOKEN" ]; then
  echo "ERROR: missing 1Password field: op://${vault}/${FLY_TOKEN_FIELD}" >&2
  exit 1
fi
export FLY_API_TOKEN

# An app that was never deployed has no writers by definition. Checked before
# scaling so a first-time teardown is not blocked on creating one.
if ! flyctl status --app "$app" >/dev/null 2>&1; then
  echo "✅ $app does not exist — no writers to stop"
  exit 0
fi

echo "Scaling $app to zero machines (env: $env_mode)..."
flyctl scale count 0 \
  --app "$app" \
  --process-group "$PROCESS_GROUP" \
  --yes

# Read the count back rather than trusting the scale command's exit status.
# `flyctl machine list --json` returns `[]` for an app with no machines.
running_machines() {
  local machines
  machines="$(flyctl machine list --app "$app" --json 2>/dev/null || echo '[]')"
  printf '%s' "$machines" | grep -c '"id"' || true
}

# Counter loop rather than `seq`: BSD seq (macOS, where these playbooks are
# run by hand) counts DOWN when the upper bound is below the lower one, so a
# `seq`-driven loop does not degrade safely at the edges.
attempt=0
while [ "$attempt" -lt "$VERIFY_ATTEMPTS" ]; do
  count="$(running_machines)"
  if [ "$count" -eq 0 ]; then
    echo "✅ $app is at zero machines — no writer can re-migrate the datastore"
    exit 0
  fi
  echo "  $count machine(s) still present; re-checking in ${VERIFY_SLEEP_SECONDS}s..."
  sleep "$VERIFY_SLEEP_SECONDS"
  attempt=$((attempt + 1))
done

echo "❌ ERROR: $app still reports $(running_machines) machine(s) after scaling to zero." >&2
echo "The teardown is BLOCKED: a live writer would re-apply its own migrations" >&2
echo "against the emptied datastore, and the next deploy would then fail" >&2
echo "ensureCanonical with error.MigrationSchemaAhead." >&2
exit 1
