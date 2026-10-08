#!/usr/bin/env bash
# Self-tests for what deploy.sh checks and unpacks before it writes to the
# host, and where it stages the toolbox.
#
#     bash deploy/baremetal/deploy_inputs_test.sh
#
# Each case sources deploy.sh in a fresh subshell, as deploy_test.sh does: its
# `readonly` constants can be assigned once per shell, and its `set -e` would
# abort on the non-zero returns these cases assert on. `install` is stubbed on
# PATH and drops a sentinel, so a case can assert a refused deploy never
# reached it: every write the deploy makes to the host goes through it first.
# A deploy reads its inputs from a run directory, built by
# deploy_test_support.sh as the runner playbook fills one.
#
# The bundle case needs GNU tar, the tar every runner host ships. It SKIPS on
# a machine without it and hard-fails when CI is set, so the extraction is
# always proven on the ubuntu-latest runners that gate a merge.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
readonly DEPLOY_SH="$SCRIPT_DIR/deploy.sh"
# shellcheck source=./deploy_test_support.sh
source "$SCRIPT_DIR/deploy_test_support.sh"
readonly SENTINEL_INSTALL="install-ran"
readonly SENTINEL_COPY="cp-ran"
readonly SENTINEL_SYSTEMCTL="systemctl-ran"
readonly PRIVATE_DIR_MODE="drwx------"

passed=0
failed=0
skipped=0

ok()   { printf 'ok   %s\n' "$1"; passed=$((passed + 1)); }
bad()  { printf 'FAIL %s\n       %s\n' "$1" "$2" >&2; failed=$((failed + 1)); }
skip() { printf 'SKIP %s\n       %s\n' "$1" "$2"; skipped=$((skipped + 1)); }

WORK_DIR="$(mktemp -d)"
readonly WORK_DIR
readonly STUB_DIR="$WORK_DIR/bin"
mkdir -p "$STUB_DIR"
cleanup() { rm -rf "$WORK_DIR"; }
trap cleanup EXIT

printf '#!/usr/bin/env bash\ntouch "$SENTINEL_DIR/%s"\n' "$SENTINEL_INSTALL" >"$STUB_DIR/install"
# The bundle case's download hands back a local bundle, and its tar records the
# flags it was given before running the real one.
printf '#!/usr/bin/env bash\nwhile [ $# -gt 1 ] && [ "$1" != -o ]; do shift; done\ncp "$STUB_BUNDLE" "$2"\n' >"$STUB_DIR/curl"
printf '#!/usr/bin/env bash\nprintf "%%s\\n" "$@" >"$TAR_ARGS"\nexec "$REAL_TAR" "$@"\n' >"$STUB_DIR/tar"
chmod +x "$STUB_DIR/install" "$STUB_DIR/curl" "$STUB_DIR/tar"
# The staging case's own stubs: a deploy whose steps run out of order copies
# the unit into place and restarts it, and both stop at a sentinel here
# instead of on the host. Apart from STUB_DIR, whose curl copies with cp.
readonly STAGE_STUB_DIR="$WORK_DIR/stage-bin"
mkdir -p "$STAGE_STUB_DIR"
printf '#!/usr/bin/env bash\ntouch "$SENTINEL_DIR/%s"\n' "$SENTINEL_COPY" >"$STAGE_STUB_DIR/cp"
printf '#!/usr/bin/env bash\ntouch "$SENTINEL_DIR/%s"\n' "$SENTINEL_SYSTEMCTL" >"$STAGE_STUB_DIR/systemctl"
chmod +x "$STAGE_STUB_DIR/cp" "$STAGE_STUB_DIR/systemctl"

# Runs deploy.sh's main in local mode on `run_dir`, with the lock free and the
# run directory taken where it lies, and leaves any sentinel under `sentinels`.
local_deploy_status() {
  local sentinels="$1" run_dir="$2"
  mkdir -p "$sentinels"
  (
    export SENTINEL_DIR="$sentinels" PATH="$STUB_DIR:$PATH"
    # shellcheck source=./deploy.sh
    source "$DEPLOY_SH" >/dev/null 2>&1
    set +e
    acquire_deploy_lock() { :; }
    check_run_dir() { :; }
    main runner v9.9.9 "$run_dir" >/dev/null 2>&1
  )
}

# The refusal cases assert a non-zero status, which a harness that cannot
# source deploy.sh also yields. Fail loud up front instead.
preflight() {
  (
    # shellcheck source=./deploy.sh
    source "$DEPLOY_SH" >/dev/null 2>&1
    declare -F check_deploy_inputs >/dev/null && declare -F fetch_release >/dev/null
  ) || { printf 'FATAL preflight: sourcing deploy.sh did not define the functions under test\n' >&2; exit 2; }
}

preflight

# A binary replaced ahead of a refusal boots at the next restart against the
# toolbox the previous release staged, which the runner refuses.
test_deploy_refused_toolbox_leaves_the_host_untouched() {
  local name="test_deploy_refused_toolbox_leaves_the_host_untouched"
  local run_dir="$WORK_DIR/${RUN_NAME_PREFIX}short-toolbox" sentinels="$WORK_DIR/toolbox-refused"
  run_dir_with "$run_dir" json.sig
  if local_deploy_status "$sentinels" "$run_dir"; then
    bad "$name" "a deploy whose toolbox lacks its signature exited 0"
  elif [[ -e "$sentinels/$SENTINEL_INSTALL" ]]; then
    bad "$name" "a deploy refused for its toolbox wrote to the host first"
  else
    ok "$name"
  fi
}

test_deploy_refused_env_leaves_the_host_untouched() {
  local name="test_deploy_refused_env_leaves_the_host_untouched"
  local run_dir="$WORK_DIR/${RUN_NAME_PREFIX}no-env" sentinels="$WORK_DIR/env-refused"
  run_dir_with "$run_dir" env
  if local_deploy_status "$sentinels" "$run_dir"; then
    bad "$name" "a deploy whose run directory holds no $RUN_ENV_FILE exited 0"
  elif [[ -e "$sentinels/$SENTINEL_INSTALL" ]]; then
    bad "$name" "a deploy refused for its env file wrote to the host first"
  else
    ok "$name"
  fi
}

# The env checks read the file the deploy would install, before installing it.
test_deploy_inputs_refuse_a_placeholder_token() {
  local name="test_deploy_inputs_refuse_a_placeholder_token"
  local binary="$WORK_DIR/binary" toolbox="$WORK_DIR/full-toolbox" real fake status_real status_fake
  real="$WORK_DIR/real.env" fake="$WORK_DIR/fake.env"
  : >"$binary"
  toolbox_set "$toolbox"
  printf 'AGENTSFLEET_API_URL=https://api.example.test\nAGENTSFLEET_RUNNER_TOKEN=agt_rREAL\n' >"$real"
  printf 'AGENTSFLEET_API_URL=https://api.example.test\nAGENTSFLEET_RUNNER_TOKEN=agt_rFAKE_x\n' >"$fake"
  # shellcheck source=./deploy.sh
  (source "$DEPLOY_SH" >/dev/null 2>&1; set +e; check_deploy_inputs "$binary" "$toolbox" "$real" >/dev/null 2>&1)
  status_real=$?
  # shellcheck source=./deploy.sh
  (source "$DEPLOY_SH" >/dev/null 2>&1; set +e; check_deploy_inputs "$binary" "$toolbox" "$fake" >/dev/null 2>&1)
  status_fake=$?
  if [[ "$status_real" -ne 0 ]]; then
    bad "$name" "complete inputs with a real token were refused"
  elif [[ "$status_fake" -eq 0 ]]; then
    bad "$name" "the placeholder token passed the checks"
  else
    ok "$name"
  fi
}

# The deploy stages the toolbox where the runner admits it at boot: under the
# storage home the run's env file names, in toolbox/incoming, as the runner
# lays out its ToolboxHome.
test_deploy_stages_the_toolbox_where_the_runner_admits_it() {
  local name="test_deploy_stages_the_toolbox_where_the_runner_admits_it"
  local run_dir="$WORK_DIR/${RUN_NAME_PREFIX}stage" home="$WORK_DIR/storage-home" part
  run_dir_with "$run_dir" "" "RUNNER_STORAGE_HOME=$home"
  if ! (
    # shellcheck source=./deploy.sh
    source "$DEPLOY_SH" >/dev/null 2>&1
    stage_toolbox "$run_dir" "$run_dir/$RUN_ENV_FILE" >/dev/null 2>&1
  ); then
    bad "$name" "a complete set was refused"
    return
  fi
  for part in "${TOOLBOX_PARTS[@]}"; do
    if [[ ! -f "$home/toolbox/incoming/toolbox-$FIXTURE_DIGEST.$part" ]]; then
      bad "$name" "the set's .$part is not under $home/toolbox/incoming"
      return
    fi
  done
  ok "$name"
}

# A toolbox the deploy cannot stage stops it before the unit restarts, so the
# running runner keeps serving instead of booting against a set that never
# landed. The binary is installed first, which proves the run got that far.
test_deploy_that_cannot_stage_its_toolbox_never_restarts_the_runner() {
  local name="test_deploy_that_cannot_stage_its_toolbox_never_restarts_the_runner"
  local run_dir="$WORK_DIR/${RUN_NAME_PREFIX}no-toolbox" sentinels="$WORK_DIR/stage-refused" status=0
  mkdir -p "$run_dir" "$sentinels"
  : >"$run_dir/$RUN_BINARY_FILE"
  (
    export SENTINEL_DIR="$sentinels" PATH="$STAGE_STUB_DIR:$STUB_DIR:$PATH"
    # shellcheck source=./deploy.sh
    source "$DEPLOY_SH" >/dev/null 2>&1
    acquire_deploy_lock() { :; }
    check_run_dir() { :; }
    check_deploy_inputs() { :; }
    main runner v9.9.9 "$run_dir" >/dev/null 2>&1
  ) || status=$?
  if [[ "$status" -eq 0 ]]; then
    bad "$name" "a deploy whose toolbox never staged exited 0"
  elif [[ ! -e "$sentinels/$SENTINEL_INSTALL" ]]; then
    bad "$name" "the deploy stopped before it installed the binary — test harness fault, not a deploy fault"
  elif [[ -e "$sentinels/$SENTINEL_SYSTEMCTL" ]]; then
    bad "$name" "a deploy whose toolbox never staged still reached systemctl"
  else
    ok "$name"
  fi
}

# The bundle is built on a CI runner, so its `./` entry carries that runner's
# uid and a 0755 mode. Unpacked as root with tar's defaults it would hand the
# private download directory to that uid; the deploy keeps root the owner and
# the directory 0700. Ownership needs root to observe, so the case reads the
# flags tar was handed and the mode it left.
test_deploy_bundle_keeps_its_directory_private() {
  local name="test_deploy_bundle_keeps_its_directory_private"
  local src="$WORK_DIR/bundle-src" bundle="$WORK_DIR/bundle.tar.gz" args="$WORK_DIR/tar-args" mode real_tar
  real_tar="$(command -v tar)"
  mkdir -p "$src"
  printf 'runner\n' >"$src/agentsfleet-runner-linux-amd64"
  chmod 755 "$src"
  tar czf "$bundle" -C "$src" .
  mode="$(
    export PATH="$STUB_DIR:$PATH" STUB_BUNDLE="$bundle" TAR_ARGS="$args" REAL_TAR="$real_tar"
    # shellcheck source=./deploy.sh
    source "$DEPLOY_SH" >/dev/null 2>&1
    set +e
    VERSION=v9.9.9
    fetch_release >/dev/null 2>&1 || exit 1
    ls -ld "$RELEASE_DIR" | cut -c1-10
  )"
  if [[ "$mode" != "$PRIVATE_DIR_MODE" ]]; then
    bad "$name" "the download directory reads '${mode:-unextracted}' after the bundle, want $PRIVATE_DIR_MODE"
  elif ! grep -qx -- '--no-same-owner' "$args"; then
    bad "$name" "tar was not told to keep root the owner: $(tr '\n' ' ' <"$args")"
  else
    ok "$name"
  fi
}

test_deploy_refused_toolbox_leaves_the_host_untouched
test_deploy_refused_env_leaves_the_host_untouched
test_deploy_inputs_refuse_a_placeholder_token
test_deploy_stages_the_toolbox_where_the_runner_admits_it
test_deploy_that_cannot_stage_its_toolbox_never_restarts_the_runner

if tar --version 2>/dev/null | grep -q 'GNU tar'; then
  test_deploy_bundle_keeps_its_directory_private
elif [[ -n "${CI:-}" ]]; then
  bad "test_deploy_bundle_keeps_its_directory_private" "GNU tar not found on a CI runner — the bundle extraction must be proven here"
else
  skip "test_deploy_bundle_keeps_its_directory_private" "GNU tar not installed (it is the tar every runner host ships)"
fi

printf '\n%d passed, %d failed, %d skipped\n' "$passed" "$failed" "$skipped"
[[ "$failed" -eq 0 ]]
