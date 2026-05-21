#!/usr/bin/env bash
# Black-box smoke tests for install.sh.
#
# Each test builds a hermetic sandbox: PATH points at a fake-bin dir holding only
# the real coreutils install.sh needs plus per-scenario fakes (npm/npx/node/hosts).
# No real npm/node is ever invoked. Run: bash ui/usezombie.sh/install_test.sh
set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
INSTALL_SH="$SCRIPT_DIR/dist/install.sh"
REAL_TOOLS="mkdir grep tail cat uname basename dirname id rm chmod sed head wc bash sh mktemp"
PASS=0 FAIL=0

# ─── Result helpers ──────────────────────────────────────────────────────────

pass() { PASS=$((PASS + 1)); printf '  ok   %s\n' "$1"; }
fail() { FAIL=$((FAIL + 1)); printf '  FAIL %s — %s\n' "$1" "$2" >&2; }

assert_rc()        { if [[ "$RC" == "$2" ]]; then pass "$1"; else fail "$1" "expected rc $2, got $RC"; fi; }
assert_out()       { if printf '%s' "$OUT" | grep -qF -- "$2"; then pass "$1"; else fail "$1" "stdout missing '$2'"; fi; }
assert_log()       { if grep -qF -- "$2" "$3"; then pass "$1"; else fail "$1" "log missing '$2'"; fi; }
assert_log_empty() { if [[ ! -s "$1" ]]; then pass "$2"; else fail "$2" "expected no command invocation"; fi; }

# ─── Sandbox + fakes ─────────────────────────────────────────────────────────

new_sandbox() {
  SANDBOX="$(mktemp -d)"
  FAKE_BIN="$SANDBOX/bin"; HOME_DIR="$SANDBOX/home"; PREFIX_DIR="$SANDBOX/prefix"
  NPM_LOG="$SANDBOX/npm.argv"; NPX_LOG="$SANDBOX/npx.argv"
  mkdir -p "$FAKE_BIN" "$HOME_DIR"
  : >"$NPM_LOG"; : >"$NPX_LOG"
  UZ_INSTALL_ENV="$PREFIX_DIR"; UZ_HOST_ENV=""
  local tool path
  for tool in $REAL_TOOLS; do
    path="$(command -v "$tool" 2>/dev/null)" && ln -sf "$path" "$FAKE_BIN/$tool"
  done
}

cleanup() { [[ -n "${SANDBOX:-}" && -d "$SANDBOX" ]] && rm -rf "$SANDBOX"; }

# fake_logged <name> <log> <exit> [stderr-msg] — logs argv, optional stderr, exits code.
fake_logged() {
  local name="$1" log="$2" code="$3" msg="${4:-}" path
  path="$FAKE_BIN/$name"
  printf '#!/bin/sh\nprintf "%%s\\n" "$*" >> "%s"\n' "$log" >"$path"
  [[ -n "$msg" ]] && printf 'printf "%%s\\n" "%s" >&2\n' "$msg" >>"$path"
  printf 'exit %s\n' "$code" >>"$path"
  chmod +x "$path"
}

# fake_present <name> — exists on PATH, exits 0 (presence-only, e.g. node, hosts).
fake_present() {
  printf '#!/bin/sh\nexit 0\n' >"$FAKE_BIN/$1"; chmod +x "$FAKE_BIN/$1"
}

run_install() {
  OUT="$(
    env "PATH=$FAKE_BIN" "HOME=$HOME_DIR" "SHELL=/bin/bash" \
        "USEZOMBIE_INSTALL=$UZ_INSTALL_ENV" "USEZOMBIE_HOST=$UZ_HOST_ENV" \
        bash "$INSTALL_SH" "$@" </dev/null 2>&1
  )"
  RC=$?
}

# ─── Tests ───────────────────────────────────────────────────────────────────

test_happy_path_posix() {
  new_sandbox; fake_present node; fake_logged npm "$NPM_LOG" 0; fake_logged npx "$NPX_LOG" 0; fake_present claude
  run_install
  assert_rc "happy_path: exit 0" 0
  assert_log "happy_path: npm install argv" "install -g" "$NPM_LOG"
  assert_log "happy_path: npm package" "@usezombie/zombiectl" "$NPM_LOG"
  assert_log "happy_path: npx skills --host" "skills add usezombie/usezombie --host=claude" "$NPX_LOG"
  assert_out "happy_path: next-command hint" "/usezombie-install-platform-ops"
  cleanup
}

test_host_detection_claude() {
  new_sandbox; fake_present node; fake_logged npm "$NPM_LOG" 0; fake_logged npx "$NPX_LOG" 0; fake_present claude
  run_install
  assert_log "host_detection_claude: --host=claude" "--host=claude" "$NPX_LOG"
  cleanup
}

test_host_detection_ambiguous() {
  new_sandbox; fake_present node; fake_logged npm "$NPM_LOG" 0; fake_logged npx "$NPX_LOG" 0
  fake_present claude; fake_present amp
  run_install
  assert_rc "host_ambiguous: exit 4" 4
  assert_out "host_ambiguous: diagnostic" "multiple agent hosts found"
  assert_log_empty "$NPM_LOG" "host_ambiguous: npm not called"
  cleanup
}

test_host_override() {
  new_sandbox; fake_present node; fake_logged npm "$NPM_LOG" 0; fake_logged npx "$NPX_LOG" 0
  fake_present claude; fake_present amp; UZ_HOST_ENV="codex"
  run_install
  assert_rc "host_override: exit 0" 0
  assert_log "host_override: --host=codex wins" "--host=codex" "$NPX_LOG"
  cleanup
}

test_node_missing() {
  new_sandbox; fake_present claude
  run_install
  assert_rc "node_missing: exit 5" 5
  assert_out "node_missing: Node.js message" "Node.js"
  assert_out "node_missing: nodejs.org link" "nodejs.org"
  cleanup
}

test_network_failure() {
  new_sandbox; fake_present node; fake_logged npm "$NPM_LOG" 1 "npm error code ENOTFOUND getaddrinfo registry.npmjs.org"
  fake_logged npx "$NPX_LOG" 0; fake_present claude
  run_install
  assert_rc "network_failure: exit 1" 1
  assert_out "network_failure: actionable" "network error"
  cleanup
}

test_npm_install_failed() {
  new_sandbox; fake_present node; fake_logged npm "$NPM_LOG" 1 "npm error code E404 Not Found"
  fake_logged npx "$NPX_LOG" 0; fake_present claude
  run_install
  assert_rc "npm_install_failed: exit 2" 2
  assert_out "npm_install_failed: npm failed message" "npm failed to install"
  cleanup
}

test_install_dir_not_writable() {
  new_sandbox; fake_present node; fake_logged npm "$NPM_LOG" 0; fake_logged npx "$NPX_LOG" 0; fake_present claude
  UZ_INSTALL_ENV="/proc/forbidden-usezombie"
  run_install
  assert_rc "install_dir_not_writable: exit 3" 3
  assert_log_empty "$NPM_LOG" "install_dir_not_writable: npm not called"
  cleanup
}

test_version_pin() {
  new_sandbox; fake_present node; fake_logged npm "$NPM_LOG" 0; fake_logged npx "$NPX_LOG" 0; fake_present claude
  run_install v0.42.0
  assert_rc "version_pin: exit 0" 0
  assert_log "version_pin: npm targets pinned version" "@usezombie/zombiectl@0.42.0" "$NPM_LOG"
  cleanup
}

test_partial_download_safety() {
  new_sandbox; fake_present node; fake_logged npm "$NPM_LOG" 0; fake_logged npx "$NPX_LOG" 0; fake_present claude
  local n; n="$(wc -l <"$INSTALL_SH")"
  OUT="$(head -n "$((n - 2))" "$INSTALL_SH" | PATH="$FAKE_BIN" HOME="$HOME_DIR" bash 2>&1)"; RC=$?
  if printf '%s' "$OUT" | grep -qF "Installing"; then
    fail "partial_download: main not invoked" "ran despite truncation"
  else
    pass "partial_download: main not invoked"
  fi
  assert_log_empty "$NPM_LOG" "partial_download: npm not called"
  cleanup
}

test_reinstall_idempotent() {
  new_sandbox; fake_present node; fake_logged npm "$NPM_LOG" 0; fake_logged npx "$NPX_LOG" 0; fake_present claude
  mkdir -p "$PREFIX_DIR/bin"; printf '#!/bin/sh\nexit 0\n' >"$PREFIX_DIR/bin/zombiectl"; chmod +x "$PREFIX_DIR/bin/zombiectl"
  run_install; assert_rc "reinstall_idempotent: first run exit 0" 0
  assert_out "reinstall_idempotent: first run detects existing" "Existing install detected"
  run_install; assert_rc "reinstall_idempotent: second run exit 0" 0
  assert_out "reinstall_idempotent: second run detects existing" "Existing install detected"
  cleanup
}

test_invalid_version_rejected() {
  new_sandbox; fake_present node; fake_logged npm "$NPM_LOG" 0; fake_logged npx "$NPX_LOG" 0; fake_present claude
  run_install vNOPE
  assert_rc "invalid_version: exit 2" 2
  assert_out "invalid_version: diagnostic" "invalid version"
  assert_log_empty "$NPM_LOG" "invalid_version: npm not called"
  cleanup
}

test_unknown_flag_rejected() {
  new_sandbox; fake_present node; fake_logged npm "$NPM_LOG" 0; fake_logged npx "$NPX_LOG" 0; fake_present claude
  run_install --bogus
  assert_rc "unknown_flag: exit 2" 2
  assert_out "unknown_flag: diagnostic" "unknown flag"
  cleanup
}

test_npm_missing_node_present() {
  new_sandbox; fake_present node; fake_present claude   # node present, npm + npx absent
  run_install
  assert_rc "npm_missing: exit 5" 5
  assert_out "npm_missing: Node.js message" "Node.js"
  cleanup
}

test_generic_host_no_flag() {
  new_sandbox; fake_present node; fake_logged npm "$NPM_LOG" 0; fake_logged npx "$NPX_LOG" 0   # no host binaries
  run_install
  assert_rc "generic_host: exit 0" 0
  assert_log "generic_host: npx skills add" "skills add usezombie/usezombie" "$NPX_LOG"
  if grep -qF -- "--host" "$NPX_LOG"; then
    fail "generic_host: no --host flag" "unexpected --host passed for generic host"
  else
    pass "generic_host: no --host flag"
  fi
  assert_out "generic_host: generic hint" "Add the platform-ops skill"
  cleanup
}

test_skill_add_failure_nonfatal() {
  new_sandbox; fake_present node; fake_logged npm "$NPM_LOG" 0; fake_logged npx "$NPX_LOG" 1; fake_present claude   # npx fails
  run_install
  assert_rc "skill_add_failure: still exit 0 (CLI installed)" 0
  assert_out "skill_add_failure: warns with manual command" "skill install did not complete"
  cleanup
}

test_shellcheck_clean() {
  if ! command -v shellcheck >/dev/null 2>&1; then
    printf '  skip shellcheck_clean — shellcheck not installed\n'; return 0
  fi
  if shellcheck -s bash "$INSTALL_SH"; then pass "shellcheck_clean: zero warnings"; else fail "shellcheck_clean" "shellcheck reported issues"; fi
}

# ─── Runner ──────────────────────────────────────────────────────────────────

main() {
  printf 'install.sh smoke tests\n'
  test_happy_path_posix
  test_host_detection_claude
  test_host_detection_ambiguous
  test_host_override
  test_node_missing
  test_network_failure
  test_npm_install_failed
  test_install_dir_not_writable
  test_version_pin
  test_partial_download_safety
  test_reinstall_idempotent
  test_invalid_version_rejected
  test_unknown_flag_rejected
  test_npm_missing_node_present
  test_generic_host_no_flag
  test_skill_add_failure_nonfatal
  test_shellcheck_clean
  printf '\n%d passed, %d failed\n' "$PASS" "$FAIL"
  [[ "$FAIL" -eq 0 ]]
}

main "$@"
