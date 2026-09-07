#!/usr/bin/env bash
# Checks the acceptance visual verdict recorded for one build.
#
# Usage: 01_verdict_check.sh <BUILD_SHA>
#
# Exit 0 when a verdict file exists for the build, names a reviewer and a date,
# and records `pass`. Exit 1 otherwise, saying which of those is missing. The
# file is the whole evidence: a verdict that lives in a chat message is not one
# this script can read, which is the point of the file.
set -euo pipefail

readonly VERDICT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/verdicts"
readonly PASS="pass"

usage() {
  echo "usage: $(basename "$0") <BUILD_SHA>" >&2
  exit 2
}

# One field of the verdict, by its bullet label.
field() {
  local file="$1" label="$2"
  sed -n "s/^- ${label}: *//p" "$file" | head -n 1
}

main() {
  [ "$#" -eq 1 ] || usage
  local sha="$1"
  local file="${VERDICT_DIR}/${sha}.md"
  if [ ! -f "$file" ]; then
    echo "✗ no verdict file for ${sha} at ${file}" >&2
    exit 1
  fi
  local reviewer date verdict
  reviewer="$(field "$file" "Reviewer")"
  date="$(field "$file" "Date")"
  verdict="$(field "$file" "Verdict")"
  if [ -z "$reviewer" ] || [[ "$reviewer" == \<* ]]; then
    echo "✗ verdict for ${sha} names no reviewer" >&2
    exit 1
  fi
  if [ -z "$date" ] || [[ "$date" == \<* ]]; then
    echo "✗ verdict for ${sha} carries no date" >&2
    exit 1
  fi
  if [ "$verdict" != "$PASS" ]; then
    echo "✗ verdict for ${sha} is '${verdict:-empty}', not ${PASS}" >&2
    exit 1
  fi
  echo "✓ verdict for ${sha}: ${PASS} — ${reviewer} on ${date}"
}

main "$@"
