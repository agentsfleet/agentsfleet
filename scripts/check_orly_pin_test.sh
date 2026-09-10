#!/usr/bin/env bash
# check_orly_pin_test.sh — the guard's own tests.
#
# A gate nobody tests is a gate that can pass vacuously, and this guard's whole
# subject is a check that was recorded but never ran. Every fixture here is
# hermetic — a temp directory, a stub `orly` written on the spot, no network and
# no repository state — so the guard is proven to FIRE on drift rather than
# merely to exit 0 on a machine that happens to be pinned correctly today.
#
# The stub is reached through ORLY_BIN, which is why the guard takes the binary
# from the environment: without it these tests could only grade whatever orly
# this developer has installed, and the drift case would be untestable.

set -uo pipefail

GUARD="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/check_orly_pin.sh"
readonly GUARD
FAILURES=0

ok()  { printf 'ok   %s\n' "$1"; }
bad() { printf 'FAIL %s\n     %s\n' "$1" "$2"; FAILURES=$((FAILURES + 1)); }

workdir() { mktemp -d "${TMPDIR:-/tmp}/orly-pin-test.XXXXXX"; }

# $1 = dir, $2 = pinned version. Only the field the guard reads, so a fixture
# cannot pass by accident on shape the guard ignores.
write_config() {
  mkdir -p "$1/.oracle"
  printf '{"schema_version":1,"orly_version":"%s"}\n' "$2" > "$1/.oracle/orly.json"
}

# $1 = path to create, $2 = exactly what `--version` should print.
write_stub_orly() {
  mkdir -p "$(dirname "$1")"
  { printf '#!/usr/bin/env bash\n'; printf 'printf "%%s\\n" %q\n' "$2"; } > "$1"
  chmod +x "$1"
}

test_should_pass_when_the_installed_version_matches_the_pin() {
  local name="test_should_pass_when_the_installed_version_matches_the_pin" dir
  dir="$(workdir)"
  write_config "$dir" "0.10.7"
  write_stub_orly "$dir/bin/orly" "0.10.7"
  if ORLY_BIN="$dir/bin/orly" bash "$GUARD" "$dir/.oracle/orly.json" >/dev/null 2>&1; then
    ok "$name"
  else
    bad "$name" "a matching version was rejected"
  fi
  rm -rf "$dir"
}

test_should_fail_when_the_installed_version_drifts_from_the_pin() {
  local name="test_should_fail_when_the_installed_version_drifts_from_the_pin" dir out status=0
  dir="$(workdir)"
  write_config "$dir" "0.10.8"
  write_stub_orly "$dir/bin/orly" "0.10.7"
  out="$(ORLY_BIN="$dir/bin/orly" bash "$GUARD" "$dir/.oracle/orly.json" 2>&1)" || status=$?
  if [ "$status" -eq 0 ]; then
    bad "$name" "a stale engine passed the guard"
  elif ! printf '%s' "$out" | grep -q "0.10.8"; then
    bad "$name" "the failure did not print the PINNED version: $out"
  elif ! printf '%s' "$out" | grep -q "0.10.7"; then
    bad "$name" "the failure did not print the INSTALLED version: $out"
  else
    ok "$name"
  fi
  rm -rf "$dir"
}

# The remediation carries the pinned version, never the installed one — a fix
# command naming the version already on the machine is a no-op that reads
# like a fix.
test_should_name_an_install_command_carrying_the_pinned_version() {
  local name="test_should_name_an_install_command_carrying_the_pinned_version" dir out
  dir="$(workdir)"
  write_config "$dir" "0.11.0"
  write_stub_orly "$dir/bin/orly" "0.10.7"
  out="$(ORLY_BIN="$dir/bin/orly" bash "$GUARD" "$dir/.oracle/orly.json" 2>&1)" || true
  if printf '%s' "$out" | grep -q "@agentsfleet/orly@0.11.0"; then ok "$name"
  else bad "$name" "expected an install command pinned to 0.11.0, got: $out"; fi
  rm -rf "$dir"
}

# A bun-installed orly is not replaced by `npm install --global`: PATH order
# decides, so the manager named must match where the binary resolves.
test_should_name_the_bun_installer_when_the_binary_resolves_under_bun() {
  local name="test_should_name_the_bun_installer_when_the_binary_resolves_under_bun" dir out
  dir="$(workdir)"
  write_config "$dir" "0.11.0"
  write_stub_orly "$dir/.bun/bin/orly" "0.10.7"
  out="$(ORLY_BIN="$dir/.bun/bin/orly" bash "$GUARD" "$dir/.oracle/orly.json" 2>&1)" || true
  if printf '%s' "$out" | grep -q "bun install -g @agentsfleet/orly@0.11.0"; then ok "$name"
  else bad "$name" "expected the bun spelling for a bun-resolved binary, got: $out"; fi
  rm -rf "$dir"
}

test_should_report_absence_rather_than_crash_confusingly() {
  local name="test_should_report_absence_rather_than_crash_confusingly" dir out status=0
  dir="$(workdir)"
  write_config "$dir" "0.10.7"
  out="$(ORLY_BIN="$dir/bin/orly-does-not-exist" bash "$GUARD" "$dir/.oracle/orly.json" 2>&1)" || status=$?
  if [ "$status" -ne 1 ]; then
    bad "$name" "an absent orly must exit 1, got $status"
  elif ! printf '%s' "$out" | grep -q "not installed"; then
    bad "$name" "the failure did not say orly is absent: $out"
  elif ! printf '%s' "$out" | grep -q "@agentsfleet/orly@0.10.7"; then
    bad "$name" "an absent orly must still name the install command: $out"
  else
    ok "$name"
  fi
  rm -rf "$dir"
}

# A binary whose banner carries a prefix still pins: the guard reads a version
# out of the output rather than demanding the whole line be one.
test_should_accept_a_version_banner_with_a_prefix() {
  local name="test_should_accept_a_version_banner_with_a_prefix" dir
  dir="$(workdir)"
  write_config "$dir" "1.2.3"
  write_stub_orly "$dir/bin/orly" "orly 1.2.3 (darwin-arm64)"
  if ORLY_BIN="$dir/bin/orly" bash "$GUARD" "$dir/.oracle/orly.json" >/dev/null 2>&1; then
    ok "$name"
  else
    bad "$name" "a prefixed version banner was rejected"
  fi
  rm -rf "$dir"
}

test_should_refuse_a_missing_config_rather_than_pass_vacuously() {
  local name="test_should_refuse_a_missing_config_rather_than_pass_vacuously" status=0
  bash "$GUARD" "/nonexistent/.oracle/orly.json" >/dev/null 2>&1 || status=$?
  if [ "$status" -eq 2 ]; then ok "$name"
  else bad "$name" "a missing config must exit 2, not report the machine pinned (got $status)"; fi
}

test_should_refuse_a_config_that_declares_no_orly_version() {
  local name="test_should_refuse_a_config_that_declares_no_orly_version" dir status=0
  dir="$(workdir)"
  mkdir -p "$dir/.oracle"
  printf '{"schema_version":1}\n' > "$dir/.oracle/orly.json"
  bash "$GUARD" "$dir/.oracle/orly.json" >/dev/null 2>&1 || status=$?
  if [ "$status" -eq 2 ]; then ok "$name"
  else bad "$name" "an absent orly_version must exit 2, not pass (got $status)"; fi
  rm -rf "$dir"
}

test_should_pass_when_the_installed_version_matches_the_pin
test_should_fail_when_the_installed_version_drifts_from_the_pin
test_should_name_an_install_command_carrying_the_pinned_version
test_should_name_the_bun_installer_when_the_binary_resolves_under_bun
test_should_report_absence_rather_than_crash_confusingly
test_should_accept_a_version_banner_with_a_prefix
test_should_refuse_a_missing_config_rather_than_pass_vacuously
test_should_refuse_a_config_that_declares_no_orly_version

echo
if [ "$FAILURES" -eq 0 ]; then echo "8 passed, 0 failed"; else echo "$FAILURES failed"; exit 1; fi
