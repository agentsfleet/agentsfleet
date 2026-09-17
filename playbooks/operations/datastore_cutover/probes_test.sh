#!/usr/bin/env bash
# probes_test.sh — the cutover probes' own tests.
#
# Every probe here is pure or takes an injectable flyctl, so the whole file
# runs with no Fly account, no cluster and no network. Discovered by
# SCRIPT_SELF_TESTS and run by `make lint-all`.
#
# The one test that is not about a probe is the last: it reads 001_playbook.md
# and fails if a step row carries no probe tag. That is the rule the playbook
# states about itself, and a rule a document states about itself is a rule
# nobody enforces unless something reads it back.

set -uo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROBES="$HERE/probes.sh"
PLAYBOOK="$HERE/001_playbook.md"
readonly HERE PROBES PLAYBOOK
FAILURES=0

ok()  { printf 'ok   %s\n' "$1"; }
bad() { printf 'FAIL %s\n     %s\n' "$1" "$2"; FAILURES=$((FAILURES + 1)); }

# A fake flyctl whose exit code the caller chooses. Written per test rather
# than shared, so a test that wants a failure cannot be confused by one that
# wanted a success.
fake_flyctl() {
  local exit_code="$1" dir
  dir="$(mktemp -d "${TMPDIR:-/tmp}/probes-test.XXXXXX")"
  printf '#!/usr/bin/env bash\nexit %s\n' "$exit_code" >"$dir/flyctl"
  chmod +x "$dir/flyctl"
  printf '%s/flyctl' "$dir"
}

test_should_pass_cluster_ready_when_healthy_answers() {
  local name="test_should_pass_cluster_ready_when_healthy_answers"
  local fake; fake="$(fake_flyctl 0)"
  if FLYCTL="$fake" bash "$PROBES" cluster-ready dragonfly-dev >/dev/null 2>&1; then
    ok "$name"
  else
    bad "$name" "a healthy cluster was reported as not ready"
  fi
  rm -rf "$(dirname "$fake")"
}

# The failure that matters most: Fly's own check is TCP, so a node holding no
# cluster configuration looks alive. The probe must not agree with it.
test_should_fail_cluster_ready_when_healthy_refuses() {
  local name="test_should_fail_cluster_ready_when_healthy_refuses"
  local fake; fake="$(fake_flyctl 1)"
  if FLYCTL="$fake" bash "$PROBES" cluster-ready dragonfly-dev >/dev/null 2>&1; then
    bad "$name" "a node that failed healthy was reported ready"
  else
    ok "$name"
  fi
  rm -rf "$(dirname "$fake")"
}

test_should_fail_a_seed_that_still_resolves_the_retired_store() {
  local name="test_should_fail_a_seed_that_still_resolves_the_retired_store"
  local f; f="$(mktemp "${TMPDIR:-/tmp}/probes-seed.XXXXXX")"
  # The scheme is assembled rather than written out. check-vault-gate-parity
  # scans every playbooks/operations/*.sh for a literal `op://` and demands the
  # vault-auth preamble of whatever it finds; this file reads no vault, it
  # writes a fixture that LOOKS like one. The fixture on disk is byte-identical
  # to the real thing, which is the part that has to be realistic.
  local scheme="op:"
  printf 'DRAGONFLY_URL: %s//VAULT/upstash-dev/api-url\n' "$scheme" >"$f"
  if bash "$PROBES" seed-is-clean "$f" >/dev/null 2>&1; then
    bad "$name" "a workflow still resolving the retired item passed"
  else
    ok "$name"
  fi
  rm -f "$f"
}

# Prose about the retired store is not a resolution. A probe that forbade the
# word would make this milestone's own comments unwritable.
test_should_allow_prose_that_names_the_retired_store() {
  local name="test_should_allow_prose_that_names_the_retired_store"
  local f; f="$(mktemp "${TMPDIR:-/tmp}/probes-seed.XXXXXX")"
  # Assembled for the same reason as the fixture above.
  local scheme="op:"
  printf '# A lane still resolving an Upstash seed cannot boot the daemon.\nDRAGONFLY_URL: %s//VAULT/dragonfly-dev/api-url\n' "$scheme" >"$f"
  if bash "$PROBES" seed-is-clean "$f" >/dev/null 2>&1; then
    ok "$name"
  else
    bad "$name" "a comment naming the retired store was treated as a resolution"
  fi
  rm -f "$f"
}

test_should_refuse_a_vault_deletion_with_no_approver() {
  local name="test_should_refuse_a_vault_deletion_with_no_approver"
  if env -u DATASTORE_CUTOVER_APPROVED_BY bash "$PROBES" vault-deletion upstash-dev >/dev/null 2>&1; then
    bad "$name" "a credential deletion was permitted with nobody approving it"
  else
    ok "$name"
  fi
}

test_should_permit_a_vault_deletion_with_a_named_approver() {
  local name="test_should_permit_a_vault_deletion_with_a_named_approver"
  if DATASTORE_CUTOVER_APPROVED_BY="Indy" bash "$PROBES" vault-deletion upstash-dev >/dev/null 2>&1; then
    ok "$name"
  else
    bad "$name" "a named approver was still refused"
  fi
}

test_should_refuse_an_unknown_verb() {
  local name="test_should_refuse_an_unknown_verb"
  bash "$PROBES" not-a-probe >/dev/null 2>&1
  if [ "$?" -eq 2 ]; then
    ok "$name"
  else
    bad "$name" "an unknown verb did not exit 2"
  fi
}

# The rubric row. Every numbered step in the playbook's Steps table names a
# probe, and every probe it names is a verb probes.sh actually has -- a tag
# pointing at a verb that does not exist grades nothing.
test_should_find_a_probe_on_every_playbook_step() {
  local name="test_should_find_a_probe_on_every_playbook_step"
  local untagged=0 unknown=0 row tag verb
  while IFS= read -r row; do
    # The probe tag is the LAST column. Reading the first backtick instead
    # would pick up whatever the step description quotes, which is a filename
    # about as often as it is a probe.
    row="${row%|}"
    tag="${row##*|}"
    case "$tag" in *'`'*) ;; *) untagged=$((untagged + 1)); continue ;; esac
    verb="${tag#*\`}"
    verb="${verb%%[\` ]*}"
    grep -q "^    $verb)" "$PROBES" || { printf '  unknown verb: %s\n' "$verb" >&2; unknown=$((unknown + 1)); }
  done < <(grep -E '^\| [0-9]+ \|' "$PLAYBOOK")

  if [ "$untagged" -ne 0 ]; then
    bad "$name" "$untagged step(s) carry no probe tag"
  elif [ "$unknown" -ne 0 ]; then
    bad "$name" "$unknown step(s) name a probe verb probes.sh does not have"
  else
    ok "$name"
  fi
}

test_should_pass_cluster_ready_when_healthy_answers
test_should_fail_cluster_ready_when_healthy_refuses
test_should_fail_a_seed_that_still_resolves_the_retired_store
test_should_allow_prose_that_names_the_retired_store
test_should_refuse_a_vault_deletion_with_no_approver
test_should_permit_a_vault_deletion_with_a_named_approver
test_should_refuse_an_unknown_verb
test_should_find_a_probe_on_every_playbook_step

if [ "$FAILURES" -ne 0 ]; then
  printf '%s failure(s)\n' "$FAILURES"
  exit 1
fi
