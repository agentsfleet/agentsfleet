#!/usr/bin/env bash
# gh-actions-runtime.sh — keep every action pin on a ref that cannot move
# under us.
#
# THE FAILURE THIS EXISTS TO PREVENT.
#
# `@master` resolves to whatever that branch holds at the moment a job starts,
# which means a third party can change what runs in the production deploy path
# without a commit here. A tag or a commit SHA cannot move that way.
#
# Exit: 0 clean · 1 a mutable ref · 2 usage error.
set -uo pipefail

readonly WORKFLOW_GLOB=".github/workflows"
readonly COMPOSITE_GLOB=".github/actions"

fail_count=0
sha_pinned=0
tag_pinned=0

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

# A pin must be a version tag (`@v4`, `@v2.3.9`) or a full commit SHA. Anything
# else — a branch, no ref at all — is a third party's pointer that moves with
# no commit here, and `@master` in a deploy path is the shape this catches.
#
# A VERSION TAG IS NOT IMMUTABLE, and this check does not pretend otherwise.
# A publisher can retarget `v4` at any commit, so a tag-pinned action can change
# what it runs without a commit in this repository. Only a full SHA is fixed.
# The gate accepts tags because seventeen of nineteen pins are tags and moving
# them all is a policy decision with real upkeep, not a script change — so the
# script REPORTS the split instead of claiming a property it has not verified.
# A summary that overstates is worse than none: it retires the question.
check_mutable() {
  local pin="$1" ref="${1##*@}"
  if [ "$pin" = "$ref" ]; then
    note_failure "unpinned action (no ref): $pin"
  elif printf '%s' "$ref" | grep -qE '^[0-9a-f]{40}$'; then
    sha_pinned=$((sha_pinned + 1))
  elif printf '%s' "$ref" | grep -qE '^v[0-9]+([.][0-9]+)*$'; then
    tag_pinned=$((tag_pinned + 1))
  else
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
    check_mutable "$pin"
  done <<<"$pins"

  if [ "$fail_count" -ne 0 ]; then
    printf '✗ [gh-actions] %d pin problem(s) across %d distinct pins\n' "$fail_count" "$count" >&2
    exit 1
  fi
  printf '✓ [gh-actions] %d distinct pins (%d by SHA, %d by version tag) — none unpinned\n' \
    "$count" "$sha_pinned" "$tag_pinned"
}

main "$@"
