#!/usr/bin/env bash
# gh-actions-runtime.sh — keep every action pin on a runtime GitHub still runs,
# and on a ref that cannot change under us.
#
# THE FAILURE THIS EXISTS TO PREVENT.
#
# GitHub retires Node runtimes from its hosted runners on a published date.
# When that date passes, an action pinned to a major whose `action.yml` says
# `using: node20` stops working — and it stops working everywhere at once,
# without a commit to blame. The two pins that mattered here were the secret
# scanner, which runs on every pull request, and the release-notes publisher,
# which fires at tag push AFTER the binaries have already built. The first
# would have broken every pull request; the second would have broken a release
# halfway through one.
#
# WHY THIS IS A DENYLIST AND NOT A LOOKUP.
#
# The honest check would read each action's `action.yml` and assert its `using:`
# — but that is a network call per pin, in a gate that runs on every commit, and
# a gate that needs the network is a gate that fails on a plane. So this records
# the answer instead: a pin we have retired is named here with the reason, and
# the script fails if it comes back. Bumping past a runtime deadline is a
# deliberate act, and so is reverting one.
#
# The mutable-ref half needs no lookup at all. `@master` resolves to whatever
# that branch holds at the moment a job starts, which means a third party can
# change what runs in the production deploy path without a commit here. A tag
# or a commit SHA cannot.
#
# Exit: 0 clean · 1 a retired runtime or a mutable ref · 2 usage error.
set -uo pipefail

readonly WORKFLOW_GLOB=".github/workflows"
readonly COMPOSITE_GLOB=".github/actions"

# Pins retired because their runtime is gone or going. Format: <pin>|<reason>.
# A row leaves this list only when the pin itself is gone from the repository
# for good — the list is the record of WHY a bump happened, and deleting a row
# deletes the reason someone will look for the next time it matters.
readonly RETIRED_PINS=(
  "gitleaks/gitleaks-action@v2|node20 runtime; runs on every pull request, so its removal breaks all of them"
  "softprops/action-gh-release@v2|node20 runtime; fires at tag push after binaries build, so its removal breaks a release mid-flight"
  "actions/checkout@v3|node16 runtime, long retired"
  "actions/checkout@v4|node20 runtime"
  "actions/cache@v3|node16 runtime, long retired"
  "actions/setup-node@v3|node16 runtime, long retired"
)

fail_count=0

note_failure() {
  printf '✗ %s\n' "$1" >&2
  fail_count=$((fail_count + 1))
}

# Every `uses:` value in workflows and in this repository's own composite
# actions. Local references (`./…`) are this repository's own files and carry no
# third-party runtime, so they are not pins and are skipped.
collect_pins() {
  grep -rhoE '^[[:space:]]*-?[[:space:]]*uses:[[:space:]]*[^[:space:]#]+' \
    "$WORKFLOW_GLOB" "$COMPOSITE_GLOB" 2>/dev/null \
    | sed -E 's/^[[:space:]]*-?[[:space:]]*uses:[[:space:]]*//' \
    | grep -v '^\./' \
    | sort -u
}

check_retired() {
  local pin="$1" row retired reason
  for row in "${RETIRED_PINS[@]}"; do
    retired="${row%%|*}"
    reason="${row#*|}"
    if [ "$pin" = "$retired" ]; then
      note_failure "retired pin in use: $pin — $reason"
      return
    fi
  done
}

# A pin is immutable when its ref is a version tag (`@v4`, `@v2.3.9`) or a full
# commit SHA. Anything else — a branch, a moving tag, no ref at all — is a
# third party's mutable pointer, and `@master` in a deploy path is the shape
# this catches.
check_mutable() {
  local pin="$1" ref="${1##*@}"
  if [ "$pin" = "$ref" ]; then
    note_failure "unpinned action (no ref): $pin"
  elif ! printf '%s' "$ref" | grep -qE '^(v[0-9]+([.][0-9]+)*|[0-9a-f]{40})$'; then
    note_failure "mutable ref: $pin — pin a version tag or a commit SHA"
  fi
}

main() {
  [ $# -eq 0 ] || { printf 'usage: %s\n' "$0" >&2; exit 2; }

  local pins pin count=0
  pins="$(collect_pins)"
  [ -n "$pins" ] || { printf '✗ no action pins found — is this the repository root?\n' >&2; exit 1; }

  while IFS= read -r pin; do
    [ -n "$pin" ] || continue
    count=$((count + 1))
    check_retired "$pin"
    check_mutable "$pin"
  done <<<"$pins"

  if [ "$fail_count" -ne 0 ]; then
    printf '✗ [gh-actions] %d pin problem(s) across %d distinct pins\n' "$fail_count" "$count" >&2
    exit 1
  fi
  printf '✓ [gh-actions] %d distinct pins — all immutable, none on a retired runtime\n' "$count"
}

main "$@"
