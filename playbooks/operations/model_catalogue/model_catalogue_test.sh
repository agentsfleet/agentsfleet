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

# The allowlist is the sole source of every core.model_library row, and those
# rows are billing rates: a malformed entry becomes a wrong charge rather than a
# crash. Nothing else validates the file — seed-models.mjs maps model_id and
# context_cap_tokens straight through with no shape check, so a typo reaches the
# catalogue unchallenged.
#
# Two row shapes are legal. Manual providers carry full rate objects. Providers
# with source: api (endpoint + field_map) list bare model-id strings and fetch
# rates live, so only the manual rows can be rate-checked here.
test_allowlist_rate_rows_are_well_formed() {
  local name="allowlist_rate_rows_are_well_formed"
  local root allowlist violations status=0
  root="$(cd "$SCRIPT_DIR/../../.." && pwd)"
  allowlist="$root/scripts/model-library-allowlist.json"

  if ! command -v python3 >/dev/null 2>&1; then
    bad "$name" "python3 not found; cannot validate $allowlist"
    return
  fi

  # stderr is folded in and the exit status is captured: a validator that dies
  # writes its traceback to stderr and prints no violations, so status-blind
  # capture would read a crash as a clean bill of health.
  violations="$(python3 - "$allowlist" 2>&1 <<'PY'
import json
import sys

REQUIRED = ("model_id", "context_cap_tokens", "input", "cached_input", "output")
OPTIONAL = ("note", "tier")
MANUAL_SOURCE = "manual"
API_SOURCE = "api"

try:
    with open(sys.argv[1], encoding="utf-8") as handle:
        document = json.load(handle)
except (OSError, ValueError) as error:
    print(f"unreadable allowlist: {error}")
    sys.exit(0)


def is_number(value):
    return isinstance(value, (int, float)) and not isinstance(value, bool)


checked = 0
providers = document.get("providers")
if not isinstance(providers, dict):
    print(f"providers must be an object, got {type(providers).__name__}")
    providers = {}

for provider, config in providers.items():
    seen = set()
    if not isinstance(config, dict):
        print(f"{provider}: provider entry must be an object, got {type(config).__name__}")
        continue

    source = config.get("source")
    if source not in (MANUAL_SOURCE, API_SOURCE):
        print(f"{provider}: source must be {MANUAL_SOURCE!r} or {API_SOURCE!r}, got {source!r}")

    # Every container is type-checked before it is walked. A string or object
    # here iterates characters or keys, each of which is a str and would sail
    # through the bare-id branch below as though it were a model name.
    models = config.get("models")
    if not isinstance(models, list):
        print(f"{provider}: models must be an array, got {type(models).__name__}")
        continue

    for model in models:
        # Row shape is bound to the declared source rather than inferred from
        # the JSON type. An api provider lists bare ids and fetches rates live;
        # a manual provider carries them inline. A row in the other shape would
        # satisfy a type-only check and then leave the seeder reading rate
        # fields that are not there, or skipping a lookup it needed to make.
        if isinstance(model, str):
            if source != API_SOURCE:
                print(f"{provider}/{model}: bare model id under source {source!r} — only {API_SOURCE} lists ids")
            if model in seen:
                print(f"{provider}: duplicate model id {model}")
            seen.add(model)
            continue

        # Anything that is neither a bare id nor a rate object is malformed, and
        # naming it is the check's job. Falling through would raise on .get and
        # abandon the scan mid-provider.
        if not isinstance(model, dict):
            print(f"{provider}: model entry must be a bare id or a rate object, got {type(model).__name__}")
            continue

        checked += 1
        identifier = model.get("model_id")
        label = f"{provider}/{identifier}"
        if source != MANUAL_SOURCE:
            print(f"{label}: inline rate row under source {source!r} — only {MANUAL_SOURCE} carries rates")

        for key in REQUIRED:
            if key not in model:
                print(f"{label}: missing required key {key}")
        for key in model:
            if key not in REQUIRED and key not in OPTIONAL:
                print(f"{label}: unexpected key {key}")

        if isinstance(identifier, str) and identifier:
            if identifier in seen:
                print(f"{provider}: duplicate model id {identifier}")
            seen.add(identifier)
        else:
            print(f"{label}: model_id must be a non-empty string")

        context = model.get("context_cap_tokens")
        if not isinstance(context, int) or isinstance(context, bool) or context <= 0:
            print(f"{label}: context_cap_tokens must be a positive integer, got {context!r}")

        for key in ("input", "cached_input", "output"):
            rate = model.get(key)
            if not is_number(rate) or rate < 0:
                print(f"{label}: {key} must be a non-negative number, got {rate!r}")

        cached = model.get("cached_input")
        fresh = model.get("input")
        if is_number(cached) and is_number(fresh) and cached > fresh:
            print(f"{label}: cached_input {cached} exceeds input {fresh}")

# An empty scan proves nothing — mirror check-playbooks' reference-scan guard.
if checked == 0:
    print("scan matched no rate rows — the check is broken, not the tree")
PY
  )" || status=$?

  if [ "$status" -ne 0 ]; then
    bad "$name" "validator exited $status without reaching a verdict — a crash is not a pass: $violations"
  elif [ -n "$violations" ]; then
    bad "$name" "$violations"
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
test_allowlist_rate_rows_are_well_formed

printf '\n%d passed, %d failed\n' "$passed" "$failed"
[ "$failed" -eq 0 ]
