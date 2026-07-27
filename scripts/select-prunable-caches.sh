#!/usr/bin/env bash
# select-prunable-caches.sh — decide which Actions cache entries are reclaimable.
#
#     select-prunable-caches.sh <pr-state-file> <retain-per-family> < caches.tsv
#
# stdin  TSV: id <TAB> ref <TAB> key <TAB> created_at <TAB> size_in_bytes
# arg 1  TSV: pull-request-number <TAB> state   (CLOSED / MERGED / OPEN / UNKNOWN)
# arg 2  how many generations to keep per (ref, family) group
# stdout TSV: id <TAB> ref <TAB> key <TAB> size_in_bytes <TAB> reason
#
# The selection is pure — no network, no deletion — so the workflow that owns
# the GitHub input/output stays thin and this file carries the judgement that
# is worth testing. Two rules, both conservative:
#
#   closed-pr    A cache created on a Pull Request (PR) ref is restorable only
#                from that PR's own runs. Once the PR closes it is unreachable
#                by construction, whatever its age.
#
#   superseded   `restore-keys` prefix fallback resolves to the most recently
#                created entry sharing the prefix, so older generations of the
#                same family are dead weight. A few are retained anyway, because
#                a rerun of a slightly older commit still restores from one.
#
# A state this script cannot resolve (UNKNOWN) is left alone. Guessing would
# trade a full cold rebuild for a few reclaimed megabytes.
set -euo pipefail

if (( $# < 2 )); then
  echo "usage: select-prunable-caches.sh <pr-state-file> <retain-per-family> < caches.tsv" >&2
  exit 2
fi

pr_state_file=$1
retain=$2

case "$retain" in
  ''|*[!0-9]*) echo "✗ retain-per-family must be a non-negative integer, got '$retain'" >&2; exit 2 ;;
esac

if [[ ! -f "$pr_state_file" ]]; then
  echo "✗ pull-request state file not found: $pr_state_file" >&2
  exit 2
fi

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

cat > "$work/all.tsv"

# ── closed-pr ────────────────────────────────────────────────────────────────
while IFS=$'\t' read -r id ref key _created size; do
  [[ -n "${id:-}" ]] || continue
  pr=$(printf '%s' "$ref" | sed -n 's#^refs/pull/\([0-9]\{1,\}\)/.*#\1#p')
  [[ -n "$pr" ]] || continue
  state=$(awk -F'\t' -v p="$pr" '$1 == p { print $2; exit }' "$pr_state_file")
  case "$state" in
    CLOSED|MERGED)
      printf '%s\t%s\t%s\t%s\tclosed-pr\n' "$id" "$ref" "$key" "$size" >> "$work/doomed.tsv"
      ;;
  esac
done < "$work/all.tsv"

# ── superseded ───────────────────────────────────────────────────────────────
# The family is the key with its content-hash runs collapsed, so consecutive
# generations of one job's cache group together. Sorted newest-first within
# (ref, family); everything past the retain count is emitted.
while IFS=$'\t' read -r id ref key created size; do
  [[ -n "${id:-}" ]] || continue
  family=$(printf '%s' "$key" | sed -E 's/[0-9a-f]{32,}/H/g')
  printf '%s\t%s\t%s\t%s\t%s\t%s\n' "$ref" "$family" "$created" "$id" "$key" "$size"
done < "$work/all.tsv" | LC_ALL=C sort -t$'\t' -k1,1 -k2,2 -k3,3r > "$work/by-family.tsv"

awk -F'\t' -v retain="$retain" '
  { group = $1 "\x1f" $2 }
  group != prev { seen = 0; prev = group }
  { seen++ }
  seen > retain { printf "%s\t%s\t%s\t%s\tsuperseded\n", $4, $1, $5, $6 }
' "$work/by-family.tsv" >> "$work/doomed.tsv"

# One entry can satisfy both rules; emit it once, keeping the first reason.
touch "$work/doomed.tsv"
LC_ALL=C sort -t$'\t' -k1,1 -u "$work/doomed.tsv"
