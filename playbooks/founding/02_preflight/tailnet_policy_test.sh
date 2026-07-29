#!/usr/bin/env bash
# Regression tests for the canonical tailnet policy.
#
#     bash playbooks/founding/02_preflight/tailnet_policy_test.sh
#
# The live policy carries an "sshTests" block that Tailscale evaluates when the
# policy is saved, so a console edit that breaks CI is rejected at save time.
# These tests are the repo-side half of the same guarantee: they stop the
# canonical copy from drifting away from the grant CI depends on, and they run
# in `make check-playbooks` without needing tailnet credentials.
#
# Guards the Jul 28, 2026 outage: the workers and the ephemeral GitHub Actions
# node shared tag:ci, the ssh block granted only autogroup:member, and a tagged
# node carries no user identity — so every CI deploy was refused.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
readonly POLICY_FILE="$SCRIPT_DIR/tailnet-policy.hujson"
readonly REPO_ROOT="$SCRIPT_DIR/../../.."

passed=0
failed=0

ok()  { printf 'ok   %s\n' "$1"; passed=$((passed + 1)); }
bad() { printf 'FAIL %s\n       %s\n' "$1" "$2" >&2; failed=$((failed + 1)); }

# HuJSON is JSON with comments and trailing commas. Strip both so jq can read
# the policy structurally instead of the tests grepping for formatting.
policy_json() {
  python3 - "$POLICY_FILE" <<'PY'
import json, re, sys

raw = open(sys.argv[1], encoding="utf-8").read()
raw = re.sub(r"^\s*//.*$", "", raw, flags=re.M)
raw = re.sub(r",(\s*[}\]])", r"\1", raw)
json.dump(json.loads(raw), sys.stdout)
PY
}

assert_jq() {
  local name="$1" filter="$2" explanation="$3"
  local json
  json="$(policy_json)" || {
    bad "$name" "policy file is not parseable as HuJSON"
    return 1
  }
  if printf '%s' "$json" | jq -e "$filter" >/dev/null 2>&1; then
    return 0
  fi
  bad "$name" "$explanation"
  return 1
}

test_policy_declares_both_tag_owners() {
  local name="test_policy_declares_both_tag_owners"
  assert_jq "$name" \
    '(.tagOwners["tag:ci"] | index("autogroup:admin")) and
     (.tagOwners["tag:worker"] | index("autogroup:admin"))' \
    'tagOwners must declare tag:ci and tag:worker, each owned by autogroup:admin so an admin can retag a machine from the console' || return
  ok "$name"
}

test_policy_grants_ci_tag_to_worker_tag() {
  local name="test_policy_grants_ci_tag_to_worker_tag"
  # "check" is impossible for a tagged source (nobody to re-authenticate), and
  # root is excluded because every deploy step escalates via NOPASSWD sudo.
  assert_jq "$name" \
    '[.ssh[]
      | select(.action == "accept")
      | select(.src | index("tag:ci"))
      | select(.dst | index("tag:worker"))
      | select(.users | index("root") | not)]
     | length > 0' \
    'ssh block must accept src tag:ci -> dst tag:worker for non-root users; without it every CI deploy fails with "tailnet policy does not permit you to SSH to this node"' || return
  ok "$name"
}

test_member_rule_covers_both_tags_during_retag() {
  local name="test_member_rule_covers_both_tags_during_retag"
  assert_jq "$name" \
    '[.ssh[]
      | select(.action == "accept")
      | select(.src | index("autogroup:member"))
      | select((.dst | index("tag:worker")) and (.dst | index("tag:ci")))]
     | length > 0' \
    'the member ssh rule must list both tag:worker and tag:ci, so retagging a worker cannot strand human access mid-flight' || return
  ok "$name"
}

test_policy_asserts_ci_access_in_sshtests() {
  local name="test_policy_asserts_ci_access_in_sshtests"
  assert_jq "$name" \
    '[.sshTests[]
      | select(.src == "tag:ci")
      | select((.dst | index("zombie-dev-worker-ant")) and
               (.dst | index("zombie-prod-worker-ant")))
      | select(.accept | length > 0)
      | select(.deny | index("root"))]
     | length > 0' \
    'sshTests must assert tag:ci reaches both workers as a non-root user and is denied root, so Tailscale rejects a policy save that breaks CI' || return
  ok "$name"
}

test_bootstrap_playbooks_advertise_worker_tag() {
  local name="test_bootstrap_playbooks_advertise_worker_tag"
  # Scoped to the playbook prose: those `tailscale up` lines are what an operator
  # copies onto a host. Restricting to *.md also keeps this test from matching
  # the very grep pattern written below.
  local stale
  stale="$(grep -rn --include='*.md' -- '--advertise-tags=tag:ci' "$REPO_ROOT/playbooks" 2>/dev/null || true)"
  if [ -n "$stale" ]; then
    bad "$name" "a worker bootstrap still advertises tag:ci, which would silently undo the tag split on the next re-bootstrap: ${stale}"
    return
  fi
  local advertised
  advertised="$(grep -rl --include='*.md' -- '--advertise-tags=tag:worker' "$REPO_ROOT/playbooks" 2>/dev/null | wc -l | tr -d ' ')"
  if [ "$advertised" -lt 2 ]; then
    bad "$name" "expected both the dev and prod bootstrap playbooks to advertise tag:worker, found ${advertised}"
    return
  fi
  ok "$name"
}

test_policy_declares_both_tag_owners
test_policy_grants_ci_tag_to_worker_tag
test_member_rule_covers_both_tags_during_retag
test_policy_asserts_ci_access_in_sshtests
test_bootstrap_playbooks_advertise_worker_tag

printf '\n%d passed, %d failed\n' "$passed" "$failed"
[ "$failed" -eq 0 ]
