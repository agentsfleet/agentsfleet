#!/usr/bin/env bash
# check_orly_pin.sh — the orly on this machine's PATH must be the version
# .oracle/orly.json pins.
#
# THE FAILURE THIS EXISTS TO PREVENT.
#
# `.oracle/orly.json` records `orly_version`, and Continuous Integration (CI)
# installs exactly that version before running the gates — the two lines in
# .github/workflows/governance.yml that read the field with jq and install it.
# Locally nothing did. A stale global orly READS the pin, proceeds anyway, and
# grades a narrower criteria set than the repository declares — one of the five
# declared commands instead of five — while printing a list that looks
# complete. On a stale engine `orly doctor` printed thirty red "managed file
# was edited" lines and still exited 0. So the pin was recorded and unenforced
# on the only path a developer actually uses, which is the same shape as a test
# that cannot fail.
#
# WHY THIS ASSERTS RATHER THAN INSTALLS.
#
# Installing on every commit costs network per commit and silently mutates a
# developer's global tooling. The commit tier is declared to cost seconds, so
# this compares two strings and NAMES the install command instead of running
# it. The developer stays in charge of their own machine; the gate still
# refuses to run on the wrong engine.
#
# WHY THE PINNED VERSION IS NEVER SPELLED HERE.
#
# A number pasted into this file would be a second place for the pin to live,
# and the wrong one within an hour of the next engine bump. `.oracle/orly.json`
# is the single source; jq reads it with the same expression governance.yml
# uses, so the local path and CI cannot disagree about what "pinned" means.
#
# Exit: 0 the installed version matches the pin · 1 drift, or orly absent, or
#       a version that cannot be read from the binary · 2 the pin itself could
#       not be read (missing config, missing jq, absent field).

set -euo pipefail

CONFIG="${1:-.oracle/orly.json}"
ORLY_BIN="${ORLY_BIN:-orly}"
readonly CONFIG ORLY_BIN
readonly LABEL="[orly-pin]"
readonly PACKAGE="@agentsfleet/orly"
readonly SEMVER='[0-9]+\.[0-9]+\.[0-9]+([-+][0-9A-Za-z.-]+)?'

fail() { printf '✗ %s %s\n' "$LABEL" "$1" >&2; }

# Derived from where the binary actually resolves rather than assumed: a
# bun-installed orly is not replaced by `npm install --global`, and an
# npm-installed one is not replaced by bun — PATH order decides which wins, so
# naming the wrong manager prints a command that leaves the drift in place.
install_command() { # $1 = resolved binary path ("" when absent), $2 = version
  case "$1" in
    */.bun/*) printf 'bun install -g %s@%s' "$PACKAGE" "$2" ;;
    *)        printf 'npm install --global %s@%s' "$PACKAGE" "$2" ;;
  esac
}

remedy() { # $1 = resolved binary path, $2 = pinned version
  printf '  Fix:\n    %s\n' "$(install_command "$1" "$2")" >&2
}

if [ ! -f "$CONFIG" ]; then
  fail "no orly config at $CONFIG"
  printf '  usage: check_orly_pin.sh [path/to/orly.json]\n' >&2
  exit 2
fi

if ! command -v jq >/dev/null 2>&1; then
  fail "jq not found, so the pinned version cannot be read from $CONFIG"
  printf '  Install via: mise install jq\n' >&2
  exit 2
fi

pinned="$(jq -r '.orly_version // empty' "$CONFIG" 2>/dev/null || true)"
if [ -z "$pinned" ]; then
  fail "$CONFIG declares no orly_version — the pin CI installs from is missing"
  exit 2
fi

resolved="$(command -v "$ORLY_BIN" 2>/dev/null || true)"
if [ -z "$resolved" ]; then
  fail "orly is not installed, and the rule gate cannot be skipped"
  printf '    pinned (%s): %s\n' "$CONFIG" "$pinned" >&2
  remedy "" "$pinned"
  exit 1
fi

installed="$("$ORLY_BIN" --version 2>/dev/null | grep -oE "$SEMVER" | head -1 || true)"
if [ -z "$installed" ]; then
  fail "'$ORLY_BIN --version' printed no version, so the pin cannot be proved"
  printf '    pinned  (%s): %s\n' "$CONFIG" "$pinned" >&2
  printf '    binary  (%s): no version in its output\n' "$resolved" >&2
  remedy "$resolved" "$pinned"
  exit 1
fi

if [ "$installed" != "$pinned" ]; then
  fail "installed orly is not the version this repository pins"
  printf '    pinned    (%s): %s\n' "$CONFIG" "$pinned" >&2
  printf '    installed (%s): %s\n' "$resolved" "$installed" >&2
  printf '  A mismatched engine reads the pin, proceeds, and grades a narrower\n' >&2
  printf '  criteria set than this repository declares — green while proving less.\n' >&2
  remedy "$resolved" "$pinned"
  exit 1
fi

printf '✓ %s orly %s matches the pin in %s\n' "$LABEL" "$installed" "$CONFIG"
