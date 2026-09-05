#!/usr/bin/env bash
# build_and_push_test.sh — the image builder's own tests.
#
# What these exist to catch: this script decides the tag every lane compiles
# inside, and `fetch-shas` rewrites the pins in versions.env in place. A silent
# wrong answer from either is a binary built by a compiler nobody chose, or a
# rustup installed against a checksum that proves nothing — neither of which
# announces itself. So the fixtures are hermetic (a temp tree, a stub `docker`,
# a stub `curl`, no network, no repository state) and every one asserts the
# script FIRES rather than merely exits 0.
#
# The stub `curl` serves DIFFERENT bytes per architecture on purpose. An
# earlier version served one blob to both, which let a swapped
# architecture-to-key mapping pass all eleven tests.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
readonly SCRIPT_DIR
FAILURES=0
RUN=0

ok()  { printf 'ok   %s\n' "$1"; RUN=$((RUN + 1)); }
bad() { printf 'FAIL %s\n     %s\n' "$1" "$2"; FAILURES=$((FAILURES + 1)); RUN=$((RUN + 1)); }

# The fixtures and stubs live next door; this file is the assertions.
# `source-path=SCRIPTDIR` resolves the sibling relative to THIS file rather than
# the caller's working directory, so `shellcheck -x` follows it from anywhere.
# shellcheck source-path=SCRIPTDIR
# shellcheck source=build_and_push_fixtures.sh
. "$SCRIPT_DIR/build_and_push_fixtures.sh"

test_should_refuse_when_the_pin_and_versions_env_disagree() {
  local name="${FUNCNAME[0]}" dir out status=0
  dir="$(fixture 1.98.0 1.98.1)"; stub_docker "$dir"
  out="$(run_script "$dir")" || status=$?
  if [ "$status" -eq 0 ]; then bad "$name" "built an image whose rustc differs from the repository's pin"
  elif ! printf '%s' "$out" | grep -q "1.98.0"; then bad "$name" "refused without naming the pin: $out"
  else ok "$name"; fi
}

# The workspace floor is a hard refusal for every developer, so a bump that
# moves it out of step with the pin has to stop the build, not the developer.
test_should_refuse_when_the_workspace_floor_disagrees_with_the_pin() {
  local name="${FUNCNAME[0]}" dir out status=0
  dir="$(fixture 1.98.1 1.98.1 1.98.0)"; stub_docker "$dir"
  out="$(run_script "$dir")" || status=$?
  if [ "$status" -eq 0 ]; then bad "$name" "a Cargo.toml floor out of step with the pin passed"
  elif ! printf '%s' "$out" | grep -q "rust-version"; then bad "$name" "refused without naming the floor: $out"
  else ok "$name"; fi
}

# Without this, every "should refuse" test below passes for the wrong reason:
# a script that exits non-zero unconditionally satisfies all of them at once.
test_should_exit_zero_when_the_build_succeeds() {
  local name="${FUNCNAME[0]}" dir out status=0
  dir="$(fixture)"; stub_docker "$dir"
  out="$(run_script "$dir")" || status=$?
  if [ "$status" -eq 0 ]; then ok "$name"
  else bad "$name" "a successful build reported failure (exit $status): $out"; fi
}

test_should_exit_zero_when_fetch_shas_succeeds() {
  local name="${FUNCNAME[0]}" dir out status=0
  dir="$(fixture)"; stub_curl "$dir"
  out="$(run_script "$dir" fetch-shas)" || status=$?
  if [ "$status" -eq 0 ]; then ok "$name"
  else bad "$name" "a successful refresh reported failure (exit $status): $out"; fi
}

test_should_derive_the_tag_from_versions_env() {
  local name="${FUNCNAME[0]}" dir
  dir="$(fixture)"; stub_docker "$dir"
  run_script "$dir" >/dev/null
  if grep -qx "ghcr.io/agentsfleet/ci-rust-alpine:1.98.1-alpine3.24" "$dir/args"; then ok "$name"
  else bad "$name" "tag was not composed from the pins: $(tr '\n' ' ' < "$dir/args")"; fi
}

# The Dockerfile refuses to build without these three, so a script that forgets
# one turns a clear refusal into an unverified installer or a failed build.
test_should_pass_the_rustup_pin_and_both_checksums() {
  local name="${FUNCNAME[0]}" dir missing="" expected
  dir="$(fixture)"; stub_docker "$dir"
  run_script "$dir" >/dev/null
  for expected in "RUSTUP_VERSION=1.29.0" \
                  "RUSTUP_SHA256_X86_64_MUSL=1111111111111111111111111111111111111111111111111111111111111111" \
                  "RUSTUP_SHA256_AARCH64_MUSL=2222222222222222222222222222222222222222222222222222222222222222"; do
    grep -qx "$expected" "$dir/args" || missing="$missing $expected"
  done
  if [ -z "$missing" ]; then ok "$name"; else bad "$name" "build args missing:$missing"; fi
}

test_should_push_both_architectures_by_default() {
  local name="${FUNCNAME[0]}" dir
  dir="$(fixture)"; stub_docker "$dir"
  run_script "$dir" >/dev/null
  if grep -qx "linux/amd64,linux/arm64" "$dir/args" && grep -qx -- "--push" "$dir/args"; then ok "$name"
  else bad "$name" "a pushed image must carry both arches: $(tr '\n' ' ' < "$dir/args")"; fi
}

test_should_load_a_single_arch_when_not_pushing() {
  local name="${FUNCNAME[0]}" dir
  dir="$(fixture)"; stub_docker "$dir"
  run_script "$dir" --no-push >/dev/null
  if grep -qx -- "--load" "$dir/args" && ! grep -qx -- "--platform" "$dir/args"; then ok "$name"
  else bad "$name" "--load cannot accept a multi-platform manifest: $(tr '\n' ' ' < "$dir/args")"; fi
}

test_should_suffix_the_tag_with_the_revision() {
  local name="${FUNCNAME[0]}" dir
  dir="$(fixture)"; stub_docker "$dir"
  run_script "$dir" --revision r2 >/dev/null
  if grep -qx "ghcr.io/agentsfleet/ci-rust-alpine:1.98.1-alpine3.24-r2" "$dir/args"; then ok "$name"
  else bad "$name" "revision did not reach the tag: $(tr '\n' ' ' < "$dir/args")"; fi
}

test_should_refuse_when_a_required_pin_is_missing() {
  local name="${FUNCNAME[0]}" dir out status=0
  dir="$(fixture)"; stub_docker "$dir"
  grep -v '^RUSTUP_VERSION=' "$(env_file "$dir")" > "$dir/v" && mv "$dir/v" "$(env_file "$dir")"
  out="$(run_script "$dir")" || status=$?
  if [ "$status" -eq 0 ]; then bad "$name" "a missing pin passed silently"
  elif ! printf '%s' "$out" | grep -q "RUSTUP_VERSION"; then bad "$name" "refused without naming it: $out"
  else ok "$name"; fi
}

# `${VAR:?}` fires on unset, not on absent-from-file. Without clearing the pins
# first, an operator who exported one would bake a value the file never carried
# while the error message blamed the file.
test_should_refuse_a_pin_the_file_lacks_even_when_the_environment_exports_it() {
  local name="${FUNCNAME[0]}" dir status=0
  dir="$(fixture)"; stub_docker "$dir"
  grep -v '^RUSTUP_VERSION=' "$(env_file "$dir")" > "$dir/v" && mv "$dir/v" "$(env_file "$dir")"
  ( cd "$dir" && PATH="$dir/bin:$PATH" STUB_ARGS="$dir/args" RUSTUP_VERSION=9.9.9 \
      bash "$dir/playbooks/operations/ci_rust_images/build_and_push.sh" ) >/dev/null 2>&1 || status=$?
  if [ "$status" -ne 0 ]; then ok "$name"
  else bad "$name" "an exported pin stood in for one the file does not contain"; fi
}

# The mutation this kills: swapping the architecture-to-key mapping. With one
# blob served to both arches the two checksums were identical and the swap was
# invisible.
test_fetch_shas_should_record_each_architectures_own_checksum() {
  local name="${FUNCNAME[0]}" dir x86 arm
  dir="$(fixture)"; stub_curl "$dir"
  x86="$(sha_of_string 'rustup-init-x86_64')"
  arm="$(sha_of_string 'rustup-init-aarch64')"
  run_script "$dir" fetch-shas >/dev/null
  if grep -qx "RUSTUP_SHA256_X86_64_MUSL=$x86" "$(env_file "$dir")" \
  && grep -qx "RUSTUP_SHA256_AARCH64_MUSL=$arm" "$(env_file "$dir")"; then ok "$name"
  else bad "$name" "each key must carry its OWN arch's checksum: $(grep RUSTUP_SHA256 "$(env_file "$dir")" | tr '\n' ' ')"; fi
}

# Hashing our own download would bless a tampered body. The publisher's stated
# checksum is the thing worth comparing against.
test_fetch_shas_should_refuse_when_the_published_checksum_disagrees() {
  local name="${FUNCNAME[0]}" dir out status=0 before
  dir="$(fixture)"; stub_curl "$dir"
  before="$(cat "$(env_file "$dir")")"
  out=$( cd "$dir" && PATH="$dir/bin:$PATH" MODE=mismatch \
      bash "$dir/playbooks/operations/ci_rust_images/build_and_push.sh" fetch-shas 2>&1 ) || status=$?
  if [ "$status" -eq 0 ]; then bad "$name" "bytes that do not match the published checksum were accepted"
  elif ! printf '%s' "$out" | grep -q "mismatch"; then bad "$name" "refused without saying why: $out"
  elif [ "$before" != "$(cat "$(env_file "$dir")")" ]; then bad "$name" "a refused fetch still rewrote versions.env"
  else ok "$name"; fi
}

test_fetch_shas_should_write_the_installer_version_it_fetched() {
  local name="${FUNCNAME[0]}" dir
  dir="$(fixture)"; stub_curl "$dir"
  run_script "$dir" fetch-shas --rustup-version 1.30.0 >/dev/null
  if grep -qx "RUSTUP_VERSION=1.30.0" "$(env_file "$dir")"; then ok "$name"
  else bad "$name" "checksums moved but the installer version they belong to did not"; fi
}

# One architecture failing must leave the file as it was, not carrying a new
# checksum beside a stale one.
test_fetch_shas_should_not_half_write_when_one_architecture_fails() {
  local name="${FUNCNAME[0]}" dir before status=0
  dir="$(fixture)"; stub_curl "$dir"
  before="$(cat "$(env_file "$dir")")"
  ( cd "$dir" && PATH="$dir/bin:$PATH" MODE=halfway \
      bash "$dir/playbooks/operations/ci_rust_images/build_and_push.sh" fetch-shas ) >/dev/null 2>&1 || status=$?
  if [ "$status" -eq 0 ]; then bad "$name" "a failed download reported success"
  elif [ "$before" != "$(cat "$(env_file "$dir")")" ]; then bad "$name" "versions.env was left half-rewritten"
  else ok "$name"; fi
}

# The prose beside each pin is why the pin is what it is. A refresh that
# regenerated the file from a template would erase it, and nothing would notice.
test_fetch_shas_should_leave_every_other_line_intact() {
  local name="${FUNCNAME[0]}" dir
  dir="$(fixture)"; stub_curl "$dir"
  run_script "$dir" fetch-shas >/dev/null
  if grep -q "Prose that must survive a rewrite." "$(env_file "$dir")" \
  && grep -qx "RUST_VERSION=1.98.1" "$(env_file "$dir")" \
  && grep -qx "ALPINE_SERIES=3.24" "$(env_file "$dir")"; then ok "$name"
  else bad "$name" "the refresh destroyed content it was not asked to change"; fi
}

# A trap that merely returns lets the shell RESUME after the interrupted
# command, so a cancelled refresh would delete its scratch directory and then
# carry on writing into it. Cancellation has to read as cancellation.
test_should_stop_rather_than_resume_when_cancelled() {
  local name="${FUNCNAME[0]}" dir before status=0 pid marker i=0
  dir="$(fixture)"; stub_curl_slow "$dir"
  before="$(cat "$(env_file "$dir")")"
  marker="$dir/started"
  # `exec` matters: without it the recorded pid is the wrapper subshell, which
  # dies on SIGTERM all by itself and returns 143 no matter what the script does.
  # The signal has to reach the script, so the wrapper becomes the script.
  PATH="$dir/bin:$PATH" MARKER="$marker" bash -c \
    'cd "$1" && exec bash "$1/playbooks/operations/ci_rust_images/build_and_push.sh" fetch-shas' \
    _ "$dir" >/dev/null 2>&1 &
  pid=$!
  while [ ! -f "$marker" ] && [ "$i" -lt 100 ]; do i=$((i + 1)); sleep 0.1; done
  kill -TERM "$pid" 2>/dev/null
  wait "$pid" 2>/dev/null || status=$?
  # 143 exactly, not merely non-zero: a shared returning trap ALSO exits non-zero,
  # because the resumed download writes into the scratch directory cleanup just
  # deleted and dies as a download failure. Only the status distinguishes
  # "cancelled" from "carried on and then broke".
  if [ ! -f "$marker" ]; then bad "$name" "the stub never ran — the test proved nothing"
  elif [ "$status" -ne 143 ]; then bad "$name" "cancellation exited $status, not 143 — the shell resumed instead of stopping"
  elif [ "$before" != "$(cat "$(env_file "$dir")")" ]; then bad "$name" "a cancelled refresh still rewrote versions.env"
  else ok "$name"; fi
}

test_should_refuse_an_unknown_argument() {
  local name="${FUNCNAME[0]}" dir status=0
  dir="$(fixture)"; stub_docker "$dir"
  run_script "$dir" --wat >/dev/null || status=$?
  if [ "$status" -ne 0 ]; then ok "$name"
  else bad "$name" "an unknown flag was accepted and the build ran anyway"; fi
}

test_should_refuse_when_the_versions_file_is_absent() {
  local name="${FUNCNAME[0]}" dir status=0
  dir="$(fixture)"; stub_docker "$dir"
  rm -f "$(env_file "$dir")"
  run_script "$dir" >/dev/null || status=$?
  if [ "$status" -ne 0 ]; then ok "$name"
  else bad "$name" "built with no pins at all"; fi
}

# Called by name rather than discovered by `declare -F`: a discovery loop hides
# every function from shellcheck, which then reports the whole file as dead code.
test_should_refuse_when_the_pin_and_versions_env_disagree
test_should_refuse_when_the_workspace_floor_disagrees_with_the_pin
test_should_exit_zero_when_the_build_succeeds
test_should_exit_zero_when_fetch_shas_succeeds
test_should_derive_the_tag_from_versions_env
test_should_pass_the_rustup_pin_and_both_checksums
test_should_push_both_architectures_by_default
test_should_load_a_single_arch_when_not_pushing
test_should_suffix_the_tag_with_the_revision
test_should_refuse_when_a_required_pin_is_missing
test_should_refuse_a_pin_the_file_lacks_even_when_the_environment_exports_it
test_fetch_shas_should_record_each_architectures_own_checksum
test_fetch_shas_should_refuse_when_the_published_checksum_disagrees
test_fetch_shas_should_write_the_installer_version_it_fetched
test_fetch_shas_should_not_half_write_when_one_architecture_fails
test_fetch_shas_should_leave_every_other_line_intact
test_should_stop_rather_than_resume_when_cancelled
test_should_refuse_an_unknown_argument
test_should_refuse_when_the_versions_file_is_absent

if [ "$FAILURES" -eq 0 ]; then printf '\n%d passed, 0 failed\n' "$RUN"; exit 0; fi
printf '\n%d passed, %d failed\n' "$((RUN - FAILURES))" "$FAILURES"; exit 1
