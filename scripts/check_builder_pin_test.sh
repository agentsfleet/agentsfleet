#!/usr/bin/env bash
# check_builder_pin_test.sh — the guard's own tests.
#
# A gate nobody tests is a gate that can pass vacuously: a broken grep reports
# "clean" on a tree full of violations and reads exactly like success. These
# fixtures are hermetic — a temp directory, no network, no repository state —
# so the guard is proven to FIRE, not merely to exit 0 on a tree that happens
# to be clean.

set -uo pipefail

GUARD="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/check_builder_pin.sh"
readonly GUARD
FAILURES=0

ok()  { printf 'ok   %s\n' "$1"; }
bad() { printf 'FAIL %s\n     %s\n' "$1" "$2"; FAILURES=$((FAILURES + 1)); }

workdir() { mktemp -d "${TMPDIR:-/tmp}/builder-pin-test.XXXXXX"; }

test_should_pass_when_the_tag_is_derived() {
  local name="test_should_pass_when_the_tag_is_derived" dir
  dir="$(workdir)"; mkdir -p "$dir/wf"
  cat > "$dir/wf/deploy.yml" <<'YAML'
        run: |
          RUST_VERSION="$(sed -n 's/^RUST_VERSION=//p' versions.env)"
          BUILDER="ghcr.io/agentsfleet/ci-rust-alpine:$RUST_VERSION-alpine$ALPINE_SERIES"
          docker run --rm "$BUILDER" sh -c 'cargo build'
YAML
  if bash "$GUARD" "$dir/wf" >/dev/null 2>&1; then ok "$name"
  else bad "$name" "the derived form was rejected"; fi
  rm -rf "$dir"
}

test_should_fail_when_a_literal_tag_is_pasted() {
  local name="test_should_fail_when_a_literal_tag_is_pasted" dir out
  dir="$(workdir)"; mkdir -p "$dir/wf"
  cat > "$dir/wf/deploy.yml" <<'YAML'
        run: |
          docker run --rm ghcr.io/agentsfleet/ci-rust-alpine:1.98.0-alpine3.24 sh -c 'cargo build'
YAML
  local status=0
  out="$(bash "$GUARD" "$dir/wf" 2>&1)" || status=$?
  if [ "$status" -eq 0 ]; then
    bad "$name" "a pasted literal passed the guard"
  elif ! printf '%s' "$out" | grep -q "deploy.yml"; then
    bad "$name" "the guard failed without naming the offending file"
  else
    ok "$name"
  fi
  rm -rf "$dir"
}

# The bug that motivates asserting ABSENCE rather than equality: a literal that
# still matches versions.env today is the one that silently rots at the bump.
test_should_fail_even_when_the_literal_is_currently_correct() {
  local name="test_should_fail_even_when_the_literal_is_currently_correct" dir
  dir="$(workdir)"; mkdir -p "$dir/wf"
  printf 'image: ghcr.io/agentsfleet/ci-rust-alpine:1.98.0-alpine3.24\n' > "$dir/wf/a.yml"
  if bash "$GUARD" "$dir/wf" >/dev/null 2>&1; then
    bad "$name" "a correct-today literal passed — it is the one that rots at the bump"
  else ok "$name"; fi
  rm -rf "$dir"
}

test_should_report_the_line_number_so_the_fix_is_findable() {
  local name="test_should_report_the_line_number_so_the_fix_is_findable" dir out
  dir="$(workdir)"; mkdir -p "$dir/wf"
  printf 'one\ntwo\nghcr.io/agentsfleet/ci-rust-alpine:2.0.0-alpine3.30\n' > "$dir/wf/b.yml"
  out="$(bash "$GUARD" "$dir/wf" 2>&1)"
  if printf '%s' "$out" | grep -q "b.yml:3"; then ok "$name"
  else bad "$name" "expected file:line in the output, got: $out"; fi
  rm -rf "$dir"
}

test_should_refuse_a_missing_directory_rather_than_pass_vacuously() {
  local name="test_should_refuse_a_missing_directory_rather_than_pass_vacuously"
  local status=0
  bash "$GUARD" "/nonexistent/workflows" >/dev/null 2>&1 || status=$?
  if [ "$status" -eq 2 ]; then ok "$name"
  else bad "$name" "a missing directory must exit 2, not report the tree clean (got $status)"; fi
}

test_should_pass_when_the_tag_is_derived
test_should_fail_when_a_literal_tag_is_pasted
test_should_fail_even_when_the_literal_is_currently_correct
test_should_report_the_line_number_so_the_fix_is_findable
test_should_refuse_a_missing_directory_rather_than_pass_vacuously

echo
if [ "$FAILURES" -eq 0 ]; then echo "5 passed, 0 failed"; else echo "$FAILURES failed"; exit 1; fi
