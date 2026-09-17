#!/usr/bin/env bash
# probes.sh — the checks every step of the datastore cutover is tagged with.
#
#     cluster-ready <app>     the cluster is bootstrapped, not merely listening
#     seed-is-clean <path>    a workflow file resolves no retired vault item
#     zig-citations           no Rust comment cites a .zig file that is gone
#     vault-deletion <item>   refuse unless the approval gate is satisfied
#
# A probe answers one question with an exit code and says which question it
# answered. A step in 001_playbook.md with no probe tag is not a step, it is a
# wish, and probes_test.sh fails the rubric row that carries one.
#
# `flyctl` resolves through $FLYCTL so the self-tests can inject a fake. No
# other caller sets it.
set -euo pipefail

FLYCTL="${FLYCTL:-flyctl}"
CLUSTER_SCRIPT="/usr/local/bin/dragonfly-cluster.sh"
# The retired store. Named once, per RULE UFS, because three probes and the
# rubric all ask about the same string.
RETIRED_STORE="upstash"
# The approval gate credential rotation uses. Deleting a vault item is the one
# move in this cutover with no rollback, so it refuses on absence rather than
# defaulting to permitted.
APPROVAL_ENV="DATASTORE_CUTOVER_APPROVED_BY"
readonly FLYCTL CLUSTER_SCRIPT RETIRED_STORE APPROVAL_ENV

pass() { printf '✓ %s\n' "$1"; }
fail() { printf '✗ %s\n' "$1" >&2; return 1; }

usage() {
  printf 'usage: %s cluster-ready <app> | seed-is-clean <path> | zig-citations | vault-deletion <item>\n' \
    "${0##*/}" >&2
}

# Fly's own check is TCP: a node that has never been handed its DFLYCLUSTER
# CONFIG passes it. `healthy` is the script's own verb -- every node answering
# PING, every node online in CLUSTER SHARDS, and the TLS handshake -- which is
# the shape the daemon's preflight demands before it will boot.
cluster_ready() {
  local app="$1"
  if "$FLYCTL" ssh console --app "$app" --command "bash $CLUSTER_SCRIPT healthy" >/dev/null 2>&1; then
    pass "cluster-ready: $app is bootstrapped and every node is online"
  else
    fail "cluster-ready: $app did not answer healthy — a TCP-listening node is not a bootstrapped cluster"
  fi
}

# A repointed workflow that still resolves the retired item is a workflow that
# will keep working right up until the item is deleted, and then will not.
#
# RESOLUTIONS, not mentions. The word appears in this repository's prose on
# purpose -- a comment explaining which store was retired and why is worth
# keeping, and a probe that forbids naming the thing it retired makes the
# history unwritable. What must not survive is a reference that actually
# reaches the old store: one of its vault items, or an environment variable
# named after it.
#
# The RETIRED ITEM NAMES are the pattern, not the `op://` scheme. Matching the
# scheme would make this file read as a vault reader to
# check-vault-gate-parity, which scans for exactly that literal, and this
# probe reads no vault -- it greps files for references to one. The item name
# is also the more direct question: `upstash-dev` is the thing being retired,
# and `op://` is the syntax that happens to carry it.
seed_is_clean() {
  local path="$1" hits
  [ -f "$path" ] || fail "seed-is-clean: $path does not exist"
  hits="$(grep -inE "(${RETIRED_STORE}-(dev|prod)|${RETIRED_STORE}[_-][A-Za-z0-9_-]*(URL|TOKEN|REST|ENDPOINT))" "$path" || true)"
  if [ -n "$hits" ]; then
    printf '%s\n' "$hits" >&2
    fail "seed-is-clean: $path still resolves $RETIRED_STORE"
  fi
  pass "seed-is-clean: $path resolves no $RETIRED_STORE path"
}

# The prose sweep's own check. A citation of a file still in the tree is a live
# cross-language reference and must survive; a citation of one that is gone is
# a dead pointer a reader will chase.
zig_citations() {
  local dead=0 cited
  # Every .zig path any Rust comment names, deduplicated. Paths are repository
  # relative, so they are checked from the repository root.
  #
  # SOURCE only. `target/` holds cargo's build output, and a citation compiled
  # into an .rmeta is the same citation already read from the .rs file it came
  # from -- so scanning it adds nothing and costs the whole tree's read. Worse,
  # `-o` on a binary prints `Binary file <path> matches` instead of the match,
  # and this loop read those status lines as citations: 24 "dead" ones on any
  # machine that had built the workspace, which is every machine, because
  # `lint-all` runs clippy on the lane that then runs this probe. `-I` skips
  # binaries wherever they turn up; --exclude-dir keeps the read off target/.
  while IFS= read -r cited; do
    [ -n "$cited" ] || continue
    if [ ! -f "$cited" ]; then
      printf '  dead citation: %s\n' "$cited" >&2
      dead=$((dead + 1))
    fi
  done < <(grep -rhoEI --exclude-dir=target \
    '\b(src|rustd)/[A-Za-z0-9_/.-]+\.zig\b' rustd/ 2>/dev/null | sort -u)
  [ "$dead" -eq 0 ] || fail "zig-citations: $dead cited .zig file(s) are not in the tree"
  pass "zig-citations: every cited .zig path resolves"
}

# The only step here with no undo. A repointed workflow reverts in a commit; a
# deleted 1Password item does not come back. So this refuses by default and
# needs a named approver in the environment, the same gate
# playbooks/operations/credential_rotation/001_playbook.md uses.
vault_deletion() {
  local item="$1" approver="${!APPROVAL_ENV:-}"
  if [ -z "$approver" ]; then
    fail "vault-deletion: refusing to delete $item — $APPROVAL_ENV is unset and deleting a credential has no rollback"
  fi
  pass "vault-deletion: $item approved for deletion by $approver"
}

main() {
  case "${1:-}" in
    cluster-ready)  [ "$#" -eq 2 ] || { usage; return 2; }; cluster_ready "$2" ;;
    seed-is-clean)  [ "$#" -eq 2 ] || { usage; return 2; }; seed_is_clean "$2" ;;
    zig-citations)  [ "$#" -eq 1 ] || { usage; return 2; }; zig_citations ;;
    vault-deletion) [ "$#" -eq 2 ] || { usage; return 2; }; vault_deletion "$2" ;;
    *) usage; return 2 ;;
  esac
}

# Sourced by probes_test.sh for its individual verbs; executed as a probe
# otherwise.
if [[ "${BASH_SOURCE[0]}" == "$0" ]]; then
  main "$@"
fi
