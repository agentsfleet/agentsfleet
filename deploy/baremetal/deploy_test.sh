#!/usr/bin/env bash
# Self-tests for deploy.sh's version-skip equality and its deploy mutex.
#
#     bash deploy/baremetal/deploy_test.sh
#
# Sourcing deploy.sh runs no deploy (its `main` call is guarded), so every
# function is reachable here directly. Each case sources it in a fresh subshell:
# deploy.sh's `readonly` constants can only be assigned once per shell, and its
# `set -e` would abort on the non-zero returns these tests assert on.
#
# The two lock cases need flock, which ships with util-linux and is absent on
# macOS. They SKIP on a machine without it and hard-fail when CI is set, so the
# mutex is always proven on the ubuntu-latest runners that gate a merge.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
readonly DEPLOY_SH="$SCRIPT_DIR/deploy.sh"

# The install + restart calls deploy.sh would make on a real host. Stubbed onto
# PATH so a test can assert a deploy never reached them.
readonly SENTINEL_INSTALL="install-ran"
readonly SENTINEL_SYSTEMCTL="systemctl-ran"

passed=0
failed=0
skipped=0

ok()      { printf 'ok   %s\n' "$1"; passed=$((passed + 1)); }
bad()     { printf 'FAIL %s\n       %s\n' "$1" "$2" >&2; failed=$((failed + 1)); }
skip()    { printf 'SKIP %s\n       %s\n' "$1" "$2"; skipped=$((skipped + 1)); }

WORK_DIR="$(mktemp -d)"
readonly WORK_DIR
readonly STUB_DIR="$WORK_DIR/bin"
mkdir -p "$STUB_DIR"
cleanup() { rm -rf "$WORK_DIR"; }
trap cleanup EXIT

write_stub() {
  local name="$1" sentinel="$2"
  cat >"$STUB_DIR/$name" <<STUB
#!/usr/bin/env bash
touch "\$SENTINEL_DIR/$sentinel"
exit 0
STUB
  chmod +x "$STUB_DIR/$name"
}

# systemctl needs per-subcommand control: `is-active` gates whether the version
# path starts the service and whether verify_healthy sees it come up, so a test
# drives it via SYSTEMCTL_IS_ACTIVE_RC (default 0 = active, so the common
# exact-match skip works). Every call still drops the sentinel for reachability.
cat >"$STUB_DIR/systemctl" <<STUB
#!/usr/bin/env bash
touch "\$SENTINEL_DIR/$SENTINEL_SYSTEMCTL"
for arg in "\$@"; do
  case "\$arg" in
    is-active) exit "\${SYSTEMCTL_IS_ACTIVE_RC:-0}" ;;
  esac
done
exit 0
STUB
chmod +x "$STUB_DIR/systemctl"

write_stub install "$SENTINEL_INSTALL"

# A fake agentsfleet-runner whose --version output is whatever the case needs.
make_stub_runner() {
  local name="$1" version_output="$2"
  local path="$WORK_DIR/$name"
  {
    printf '#!/usr/bin/env bash\n'
    printf 'printf "%%s\\n" %q\n' "$version_output"
  } >"$path"
  chmod +x "$path"
  printf '%s' "$path"
}

# Sources deploy.sh, relaxes its `set -e`, then reports is_already_installed's
# status for `target_version` against a stub binary printing `version_output`.
version_check_status() {
  local target_version="$1" version_output="$2"
  local stub
  stub="$(make_stub_runner "runner-$RANDOM" "$version_output")"
  (
    export SENTINEL_DIR="$WORK_DIR"
    export PATH="$STUB_DIR:$PATH"
    # shellcheck source=./deploy.sh
    source "$DEPLOY_SH" >/dev/null 2>&1
    set +e
    VERSION="$target_version"
    is_already_installed "$stub" >/dev/null 2>&1
  )
}

# The version negatives assert is_already_installed returns non-zero. That is
# also what a broken harness yields — a missing awk, or deploy.sh failing to
# source — so those tests could pass for the wrong reason. Fail loud up front if
# the harness itself is not sound, converting a vacuous green into a red.
preflight() {
  command -v awk >/dev/null 2>&1 \
    || { printf 'FATAL preflight: awk not found — version parsing untestable\n' >&2; exit 2; }
  (
    # shellcheck source=./deploy.sh
    source "$DEPLOY_SH" >/dev/null 2>&1
    declare -F is_already_installed >/dev/null && declare -F version_token_matches >/dev/null
  ) || { printf 'FATAL preflight: sourcing deploy.sh did not define the functions under test\n' >&2; exit 2; }
}

preflight

# ── §1 — version equality ────────────────────────────────────────────────────

test_deploy_version_substring_not_equal_reinstalls() {
  local name="test_deploy_version_substring_not_equal_reinstalls"

  if version_check_status "v0.1.0" "agentsfleet-runner 0.1.0-rc1 (git abc1234)"; then
    bad "$name" "installed 0.1.0-rc1 reported as v0.1.0 — the substring skip is still live"
    return
  fi
  if version_check_status "v0.1" "agentsfleet-runner 0.10.2 (git abc1234)"; then
    bad "$name" "installed 0.10.2 reported as v0.1 — two-part tag collides on substring"
    return
  fi
  ok "$name"
}

test_deploy_version_exact_match_skips() {
  local name="test_deploy_version_exact_match_skips"

  if version_check_status "v0.1.0" "agentsfleet-runner 0.1.0 (git deadbee)"; then
    ok "$name"
  else
    bad "$name" "exact version token 0.1.0 did not match target v0.1.0 — deploy would reinstall needlessly"
  fi
}

test_deploy_malformed_version_reinstalls() {
  local name="test_deploy_malformed_version_reinstalls"

  if version_check_status "v0.1.0" ""; then
    bad "$name" "empty --version output reported as installed"
    return
  fi
  if version_check_status "v0.1.0" "agentsfleet-runner"; then
    bad "$name" "--version output with no field 2 reported as installed"
    return
  fi
  ok "$name"
}

# `systemctl start` exits zero once systemd accepts the job, so a version match
# whose service starts then immediately dies must NOT report installed — it has
# to run verify_healthy and fall through to a full reinstall on failure.
test_deploy_version_match_unhealthy_service_reinstalls() {
  local name="test_deploy_version_match_unhealthy_service_reinstalls"
  local stub
  stub="$(make_stub_runner "runner-unhealthy" "agentsfleet-runner 0.1.0 (git abc1234)")"

  local status=0
  (
    export SENTINEL_DIR="$WORK_DIR" PATH="$STUB_DIR:$PATH"
    # Service never active, and verify_healthy's single fast attempt also finds it
    # inactive — i.e. it started but did not stay up.
    export SYSTEMCTL_IS_ACTIVE_RC=1 VERIFY_HEALTH_ATTEMPTS=1 VERIFY_HEALTH_DELAY=0
    # shellcheck source=./deploy.sh
    source "$DEPLOY_SH" >/dev/null 2>&1
    set +e
    VERSION="v0.1.0"
    is_already_installed "$stub" >/dev/null 2>&1
  ) || status=$?

  if [[ "$status" -ne 0 ]]; then
    ok "$name"
  else
    bad "$name" "version matched but the service never came up healthy, yet is_already_installed reported installed"
  fi
}

# ── §2 — deploy mutex ────────────────────────────────────────────────────────

# The holder signals readiness with a marker file rather than the parent probing
# the lock: a probe is itself an acquisition, so it races the holder and — with a
# non-blocking holder — wins, leaving the lock free and the test vacuously green.
wait_for_marker() {
  local marker="$1" attempt
  for attempt in $(seq 1 100); do
    [[ -e "$marker" ]] && return 0
    sleep 0.05
  done
  return 1
}

test_deploy_second_invocation_blocked_when_locked() {
  local name="test_deploy_second_invocation_blocked_when_locked"
  local lock="$WORK_DIR/held.lock"
  local marker="$WORK_DIR/held.marker"
  local sentinels="$WORK_DIR/blocked"
  local binary="$WORK_DIR/staged-binary"
  mkdir -p "$sentinels"

  # The staged binary must exist. If it does not, an unlocked deploy dies on the
  # missing file before ever calling install, and this test would pass whether or
  # not the lock is taken — green for the wrong reason.
  : >"$binary"

  ( flock -w 5 9 || exit 1; touch "$marker"; sleep 30 ) 9>"$lock" &
  local holder=$!
  if ! wait_for_marker "$marker"; then
    kill "$holder" 2>/dev/null
    bad "$name" "background holder never took $lock — test harness fault, not a deploy fault"
    return
  fi

  local status=0
  DEPLOY_LOCK_PATH="$lock" SENTINEL_DIR="$sentinels" PATH="$STUB_DIR:$PATH" \
    bash "$DEPLOY_SH" runner v9.9.9 "$binary" >/dev/null 2>&1 || status=$?
  kill "$holder" 2>/dev/null
  wait "$holder" 2>/dev/null

  if [[ "$status" -eq 0 ]]; then
    bad "$name" "second deploy exited 0 while the lock was held"
  elif [[ -e "$sentinels/$SENTINEL_INSTALL" || -e "$sentinels/$SENTINEL_SYSTEMCTL" ]]; then
    bad "$name" "blocked deploy still reached install/systemctl — the lock is taken too late"
  else
    ok "$name"
  fi
}

test_deploy_acquires_lock_when_free() {
  local name="test_deploy_acquires_lock_when_free"
  local lock="$WORK_DIR/free.lock"

  # Proves acquisition rather than mere exit 0: once acquire_deploy_lock returns,
  # a separate process must no longer be able to take the same lock.
  if (
    export DEPLOY_LOCK_PATH="$lock"
    # shellcheck source=./deploy.sh
    source "$DEPLOY_SH" >/dev/null 2>&1
    set +e
    acquire_deploy_lock >/dev/null 2>&1 || exit 1
    flock -n "$lock" true 2>/dev/null && exit 1
    exit 0
  ); then
    ok "$name"
  else
    bad "$name" "acquire_deploy_lock did not take $lock when it was free"
  fi
}

# ── Runner ───────────────────────────────────────────────────────────────────

test_deploy_version_substring_not_equal_reinstalls
test_deploy_version_exact_match_skips
test_deploy_malformed_version_reinstalls
test_deploy_version_match_unhealthy_service_reinstalls

if command -v flock >/dev/null 2>&1; then
  test_deploy_second_invocation_blocked_when_locked
  test_deploy_acquires_lock_when_free
elif [[ -n "${CI:-}" ]]; then
  bad "deploy mutex tests" "flock not found on a CI runner — the deploy lock must be proven here"
else
  skip "test_deploy_second_invocation_blocked_when_locked" "flock not installed (macOS: brew install flock; Linux: apt-get install util-linux)"
  skip "test_deploy_acquires_lock_when_free" "flock not installed (macOS: brew install flock; Linux: apt-get install util-linux)"
fi

printf '\n%d passed, %d failed, %d skipped\n' "$passed" "$failed" "$skipped"
[[ "$failed" -eq 0 ]]
