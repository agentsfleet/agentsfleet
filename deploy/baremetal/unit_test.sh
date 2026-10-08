#!/usr/bin/env bash
# Self-tests for the runner's systemd unit, agentsfleet-runner.service.
#
#     bash deploy/baremetal/unit_test.sh
#
# The unit is read as text, and every value a case holds it to is read from the
# Rust constant that owns it, so a renamed knob or a new required controller
# fails here instead of on a host. `systemd-analyze verify` needs systemd: it
# SKIPS on a machine without it and hard-fails when CI is set, so the unit is
# always proven on the ubuntu-latest runners that gate a merge.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
readonly REPO_ROOT="$SCRIPT_DIR/../.."
readonly RUNNER_UNIT="$SCRIPT_DIR/agentsfleet-runner.service"
readonly HOME_ENV_PREFIX="Environment=HOME="
readonly RUNTIME_DIR_PREFIX="RuntimeDirectory="
# systemd creates RuntimeDirectory=<name> at /run/<name>.
readonly RUNTIME_DIR_ROOT="/run"
# The entry that supervises leases (`rustd/crates/agentsfleet_runner/src/main.rs`).
readonly RUN_ENTRY="run"
# What %H expands to: the host's name, one series identity per host.
readonly HOST_SPECIFIER="%H"

# The Rust owners of the values below.
readonly PROBE_RS="$REPO_ROOT/rustd/crates/afr_sandbox/src/probe.rs"
readonly CONFIG_RS="$REPO_ROOT/rustd/crates/afr_supervisor/src/config.rs"
readonly RESOURCE_RS="$REPO_ROOT/rustd/crates/afd_otlp/src/resource.rs"

passed=0
failed=0
skipped=0

ok()   { printf 'ok   %s\n' "$1"; passed=$((passed + 1)); }
bad()  { printf 'FAIL %s\n       %s\n' "$1" "$2" >&2; failed=$((failed + 1)); }
skip() { printf 'SKIP %s\n       %s\n' "$1" "$2"; skipped=$((skipped + 1)); }

WORK_DIR="$(mktemp -d)"
readonly WORK_DIR
cleanup() { rm -rf "$WORK_DIR"; }
trap cleanup EXIT

# The string value of `pub const <name>: &str = "<value>";` in `file`.
rust_str_const() {
  local file="$1" name="$2"
  sed -n "s/^pub const ${name}: &str = \"\\(.*\\)\";$/\\1/p" "$file"
}

# The space-separated members of `pub const <name>: [&str; N] = [...];`.
rust_str_array() {
  local file="$1" name="$2"
  sed -n "s/^pub const ${name}: \\[&str; [0-9]*\\] = \\[\\(.*\\)\\];$/\\1/p" "$file" \
    | tr -d '",'
}

# The value of the unit's first `<key>=` line, or nothing.
unit_value() {
  local key="$1"
  sed -n "s/^${key}=//p" "$RUNNER_UNIT" | head -1
}

# A missing owner reads as an empty value, which every case below would then
# check vacuously. Fail loud up front instead.
preflight() {
  local owner
  for owner in "$PROBE_RS" "$CONFIG_RS" "$RESOURCE_RS" "$RUNNER_UNIT"; do
    [[ -f "$owner" ]] \
      || { printf 'FATAL preflight: %s is missing\n' "$owner" >&2; exit 2; }
  done
  [[ -n "$(rust_str_array "$PROBE_RS" REQUIRED_CONTROLLERS)" ]] \
    || { printf 'FATAL preflight: REQUIRED_CONTROLLERS unreadable in %s\n' "$PROBE_RS" >&2; exit 2; }
  [[ -n "$(rust_str_const "$CONFIG_RS" DEFAULT_STORAGE_HOME)" ]] \
    || { printf 'FATAL preflight: DEFAULT_STORAGE_HOME unreadable in %s\n' "$CONFIG_RS" >&2; exit 2; }
  [[ -n "$(rust_str_const "$RESOURCE_RS" INSTANCE_ID_KNOB)" ]] \
    || { printf 'FATAL preflight: INSTANCE_ID_KNOB unreadable in %s\n' "$RESOURCE_RS" >&2; exit 2; }
}

preflight

# The binary takes an entry; started bare it prints its usage and exits, and
# systemd restarts it every RestartSec for as long as the host is up.
test_unit_starts_the_run_entry() {
  local name="test_unit_starts_the_run_entry"
  local exec_start
  exec_start="$(unit_value ExecStart)"
  if [[ "${exec_start##* }" != "$RUN_ENTRY" ]]; then
    bad "$name" "ExecStart=$exec_start does not start the '$RUN_ENTRY' entry"
  else
    ok "$name"
  fi
}

# systemd gives a User=-less service no HOME, so the unit supplies one inside
# its runtime directory: ProtectHome=yes makes /root and /home unreadable to the
# service, and ProtectSystem=strict leaves only ReadWritePaths writable.
test_unit_defines_home() {
  local name="test_unit_defines_home"
  local home_value runtime_name
  home_value="$(unit_value "${HOME_ENV_PREFIX%=}")"
  runtime_name="$(unit_value "${RUNTIME_DIR_PREFIX%=}")"
  if [[ -z "$home_value" ]]; then
    bad "$name" "$RUNNER_UNIT sets no ${HOME_ENV_PREFIX}<path>"
    return
  fi
  if [[ -z "$runtime_name" ]]; then
    bad "$name" "$RUNNER_UNIT declares no ${RUNTIME_DIR_PREFIX}<name> to back HOME"
    return
  fi
  local runtime_dir="${RUNTIME_DIR_ROOT}/${runtime_name}"
  if [[ "$home_value" != "$runtime_dir"* ]]; then
    bad "$name" "HOME=$home_value is outside $runtime_dir — ProtectHome/ProtectSystem leave it unwritable"
  elif ! grep -qE "^ProtectHome=yes" "$RUNNER_UNIT"; then
    bad "$name" "$RUNNER_UNIT no longer sets ProtectHome=yes — re-check why HOME points at $runtime_dir"
  elif ! grep -qE "^ReadWritePaths=.*${runtime_dir}( |$)" "$RUNNER_UNIT"; then
    bad "$name" "$runtime_dir is not in ReadWritePaths — ProtectSystem=strict leaves it read-only"
  else
    ok "$name"
  fi
}

# The storage home holds every lease's sandbox, the spool, the bundle cache and
# the toolbox, so it must be writable under ProtectSystem=strict; and the host
# probe refuses every sandbox unless each controller it requires is delegated.
test_baremetal_unit_hosts_the_rust_runner() {
  local name="test_baremetal_unit_hosts_the_rust_runner"
  local storage_home writable delegated controller
  storage_home="$(rust_str_const "$CONFIG_RS" DEFAULT_STORAGE_HOME)"
  writable=" $(unit_value ReadWritePaths) "
  delegated=" $(unit_value Delegate) "
  if [[ "$writable" != *" $storage_home "* ]]; then
    bad "$name" "ReadWritePaths does not name the storage home $storage_home"
    return
  fi
  for controller in $(rust_str_array "$PROBE_RS" REQUIRED_CONTROLLERS); do
    if [[ "$delegated" != *" $controller "* ]]; then
      bad "$name" "Delegate= omits the $controller controller the host probe requires"
      return
    fi
  done
  ok "$name"
}

# Each host publishes its own metric series: the knob the runner's resource
# reads carries the host's name.
test_unit_names_each_host_instance() {
  local name="test_unit_names_each_host_instance"
  local knob
  knob="$(rust_str_const "$RESOURCE_RS" INSTANCE_ID_KNOB)"
  if ! grep -qxF "Environment=${knob}=${HOST_SPECIFIER}" "$RUNNER_UNIT"; then
    bad "$name" "$RUNNER_UNIT does not set ${knob}=${HOST_SPECIFIER}"
  else
    ok "$name"
  fi
}

# systemd's own reading of the unit. ExecStart's binary is not installed on a
# test machine, so the copy verified points it at one that is; every other
# directive is read as written.
test_unit_passes_systemd_analyze() {
  local name="test_unit_passes_systemd_analyze"
  local copy="$WORK_DIR/agentsfleet-runner.service" output
  sed "s|^ExecStart=[^ ]*|ExecStart=$(command -v true)|" "$RUNNER_UNIT" >"$copy"
  if output="$(systemd-analyze verify "$copy" 2>&1)"; then
    ok "$name"
  else
    bad "$name" "$output"
  fi
}

test_unit_starts_the_run_entry
test_unit_defines_home
test_baremetal_unit_hosts_the_rust_runner
test_unit_names_each_host_instance

if command -v systemd-analyze >/dev/null 2>&1; then
  test_unit_passes_systemd_analyze
elif [[ -n "${CI:-}" ]]; then
  bad "test_unit_passes_systemd_analyze" "systemd-analyze not found on a CI runner — the unit must be proven here"
else
  skip "test_unit_passes_systemd_analyze" "systemd-analyze not installed (it ships with systemd)"
fi

printf '\n%d passed, %d failed, %d skipped\n' "$passed" "$failed" "$skipped"
[[ "$failed" -eq 0 ]]
