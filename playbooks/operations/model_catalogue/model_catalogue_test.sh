#!/usr/bin/env bash
#
# Guards the model-catalogue gate's refusals.
#
# Every assertion here is about something NOT happening: an invalid environment
# not reaching a step, an unapproved apply not reaching the vault, the diff arm
# not reaching the write. Those are the properties that make it safe to point
# this at production, and none of them is observable from a successful run.
#
# The gate is driven with a stubbed PATH so no step can reach a real vault, a
# real database, or the network.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
GATE="$SCRIPT_DIR/00_gate.sh"

passed=0
failed=0

TMPROOT="$(mktemp -d)"
trap 'rm -rf "$TMPROOT"' EXIT

ok() {
  printf 'ok   %s\n' "$1"
  passed=$((passed + 1))
}

bad() {
  printf 'FAIL %s\n       %s\n' "$1" "$2" >&2
  failed=$((failed + 1))
}

# Stubs that RECORD rather than act, so "did this step run?" is answerable.
# `op` writing to a marker file is how the vault-read assertions are made:
# an approval check that fired too late would leave the marker behind.
# $2 (optional) is the row count the stubbed psql reports for the catalogue
# count query; it defaults to a populated catalogue.
make_sandbox() {
  local dir="$TMPROOT/$1"
  local rows="${2:-7}"
  mkdir -p "$dir/bin"
  cat >"$dir/bin/op" <<STUB
#!/usr/bin/env bash
echo "op \$*" >>"$dir/op.log"
case "\$1" in
  whoami) exit 0 ;;
  read) printf 'postgres://stub/stub\n' ;;
esac
exit 0
STUB
  cat >"$dir/bin/node" <<STUB
#!/usr/bin/env bash
echo "node \$*" >>"$dir/node.log"
exit 0
STUB
  cat >"$dir/bin/psql" <<STUB
#!/usr/bin/env bash
echo "psql \$*" >>"$dir/psql.log"
printf '$rows\n'
exit 0
STUB
  chmod +x "$dir/bin/op" "$dir/bin/node" "$dir/bin/psql"
  printf '%s' "$dir"
}

run_gate() {
  local dir="$1"
  shift
  env -i \
    PATH="$dir/bin:/usr/bin:/bin:/usr/sbin:/sbin" \
    HOME="$HOME" \
    "$@" \
    bash "$GATE" 2>&1
}

run_step() {
  local dir="$1" step="$2"
  shift 2
  env -i \
    PATH="$dir/bin:/usr/bin:/bin:/usr/sbin:/sbin" \
    HOME="$HOME" \
    "$@" \
    bash "$SCRIPT_DIR/$step" 2>&1
}

test_should_reject_unknown_environment_before_dispatch() {
  local name="should_reject_unknown_environment_before_dispatch"
  local dir
  dir="$(make_sandbox "$name")"
  run_gate "$dir" ACTION=diff ENV=staging ALLOW_VAULT_READS=1 >/dev/null 2>&1
  local status=$?
  if [ "$status" -ne 2 ]; then
    bad "$name" "expected exit 2 for ENV=staging, got $status"
  elif [ -f "$dir/op.log" ]; then
    bad "$name" "a step ran before ENV was validated"
  else
    ok "$name"
  fi
}

test_should_reject_all_environments() {
  local name="should_reject_all_environments"
  local dir
  dir="$(make_sandbox "$name")"
  run_gate "$dir" ACTION=diff ENV=all ALLOW_VAULT_READS=1 >/dev/null 2>&1
  local status=$?
  # Rate writes never span environments in one invocation.
  if [ "$status" -ne 2 ]; then
    bad "$name" "expected exit 2 for ENV=all, got $status"
  elif [ -f "$dir/op.log" ]; then
    bad "$name" "a step ran for ENV=all"
  else
    ok "$name"
  fi
}

test_should_reject_unknown_action() {
  local name="should_reject_unknown_action"
  local dir
  dir="$(make_sandbox "$name")"
  run_gate "$dir" ACTION=destroy ENV=dev ALLOW_VAULT_READS=1 >/dev/null 2>&1
  local status=$?
  if [ "$status" -ne 2 ]; then
    bad "$name" "expected exit 2 for ACTION=destroy, got $status"
  else
    ok "$name"
  fi
}

test_should_require_apply_approval() {
  local name="should_require_apply_approval"
  local dir
  dir="$(make_sandbox "$name")"
  run_gate "$dir" ACTION=apply ENV=dev ALLOW_VAULT_READS=1 >/dev/null 2>&1
  local status=$?
  # The approval check must precede the vault read, so the refusal path never
  # touches a credential.
  if [ "$status" -ne 2 ]; then
    bad "$name" "expected exit 2 without ALLOW_MODEL_CATALOGUE_WRITES, got $status"
  elif [ -f "$dir/op.log" ]; then
    bad "$name" "the vault was read before the approval check"
  else
    ok "$name"
  fi
}

test_diff_arm_writes_nothing() {
  local name="diff_arm_writes_nothing"
  local dir
  dir="$(make_sandbox "$name")"
  run_gate "$dir" ACTION=diff ENV=dev ALLOW_VAULT_READS=1 >/dev/null 2>&1
  # The seed script must be invoked WITHOUT --apply, and the apply step's
  # confirmation prompt must never appear.
  if [ ! -f "$dir/node.log" ]; then
    bad "$name" "the diff step did not run the seed script"
  elif grep -q -- "--apply" "$dir/node.log"; then
    bad "$name" "the diff arm invoked the seed script with --apply"
  else
    ok "$name"
  fi
}

test_scripts_print_no_credentials() {
  local name="scripts_print_no_credentials"
  local dir output
  dir="$(make_sandbox "$name")"
  output="$(run_gate "$dir" ACTION=diff ENV=dev ALLOW_VAULT_READS=1 || true)"
  # The stubbed vault returns a connection string; it must never be echoed.
  if printf '%s' "$output" | grep -q 'postgres://stub'; then
    bad "$name" "a step printed the connection string"
  else
    ok "$name"
  fi
}

test_deploy_steps_reference_catalogue_priming() {
  local name="deploy_steps_reference_catalogue_priming"
  local root missing
  root="$(cd "$SCRIPT_DIR/../../.." && pwd)"
  missing=""
  for playbook in \
    "$root/playbooks/founding/04_deploy_dev/001_playbook.md" \
    "$root/playbooks/founding/07_deploy_prod/001_playbook.md"; do
    grep -q "operations/model_catalogue" "$playbook" || missing="$missing $playbook"
  done
  if [ -n "$missing" ]; then
    bad "$name" "deploy steps do not cite the catalogue playbook:$missing"
  else
    ok "$name"
  fi
}

test_verify_fails_on_empty_catalogue() {
  local name="verify_fails_on_empty_catalogue"
  local empty populated out
  empty="$(make_sandbox "${name}_empty" 0)"
  populated="$(make_sandbox "${name}_populated" 7)"

  # An empty catalogue means every fleet has no model to run on. Verify must
  # refuse, and NAME the count, so a skipped priming cannot be recorded green.
  out="$(run_step "$empty" 03_verify.sh ENV=dev ALLOW_VAULT_READS=1 || true)"
  if run_step "$empty" 03_verify.sh ENV=dev ALLOW_VAULT_READS=1 >/dev/null 2>&1; then
    bad "$name" "verify passed against a catalogue reporting 0 rows"
  elif ! printf '%s' "$out" | grep -q '0 rows'; then
    bad "$name" "verify failed but did not name the row count it found"
  elif run_step "$populated" 03_verify.sh ENV=dev ALLOW_VAULT_READS=1 >/dev/null 2>&1; then
    ok "$name"
  else
    bad "$name" "verify failed against a populated catalogue"
  fi
}

test_apply_aborts_on_confirmation_mismatch() {
  local name="apply_aborts_on_confirmation_mismatch"
  local mismatch match
  mismatch="$(make_sandbox "${name}_mismatch")"
  match="$(make_sandbox "${name}_match")"

  # Operator means dev, is pointed at prod, types "dev". The write must not
  # happen — this is the guard against writing billing rates to the wrong
  # environment, and the only thing standing between the two.
  run_step "$mismatch" 02_apply.sh ENV=prod ALLOW_VAULT_READS=1 \
    ALLOW_MODEL_CATALOGUE_WRITES=1 <<<"dev" >/dev/null 2>&1

  # Positive control, and the reason this test is worth having: without it the
  # assertion above passes whenever the step dies for ANY reason, including a
  # broken stub. Proving the SAME invocation writes when the confirmation
  # matches is what makes the refusal attributable to the guard.
  run_step "$match" 02_apply.sh ENV=prod ALLOW_VAULT_READS=1 \
    ALLOW_MODEL_CATALOGUE_WRITES=1 <<<"prod" >/dev/null 2>&1

  if [ -f "$mismatch/node.log" ] && grep -q -- "--apply" "$mismatch/node.log"; then
    bad "$name" "the catalogue was written despite a confirmation mismatch"
  elif [ ! -f "$match/node.log" ] || ! grep -q -- "--apply" "$match/node.log"; then
    bad "$name" "a MATCHING confirmation did not write either — the refusal proves nothing"
  else
    ok "$name"
  fi
}

test_existing_teardown_gates_still_reject_unknown_env() {
  local name="existing_teardown_gates_still_reject_unknown_env"
  local root failed=""
  root="$(cd "$SCRIPT_DIR/../../.." && pwd)"
  # Regression guard: both teardown gates gained a dispatched step in this
  # milestone. Their pre-existing ENV validation must be untouched.
  for gate in \
    "$root/playbooks/operations/teardown/database/00_gate.sh" \
    "$root/playbooks/operations/teardown/redis/00_gate.sh"; do
    if ENV=staging ALLOW_VAULT_READS=1 bash "$gate" >/dev/null 2>&1; then
      failed="$failed $gate"
    fi
  done
  if [ -n "$failed" ]; then
    bad "$name" "gate accepted ENV=staging:$failed"
  else
    ok "$name"
  fi
}

test_should_reject_unknown_environment_before_dispatch
test_should_reject_all_environments
test_should_reject_unknown_action
test_should_require_apply_approval
test_diff_arm_writes_nothing
test_scripts_print_no_credentials
test_deploy_steps_reference_catalogue_priming
test_verify_fails_on_empty_catalogue
test_apply_aborts_on_confirmation_mismatch
test_existing_teardown_gates_still_reject_unknown_env

printf '\n%d passed, %d failed\n' "$passed" "$failed"
[ "$failed" -eq 0 ]
