#!/usr/bin/env bash
# Self-tests for what deploy.sh checks and unpacks before it writes to the host.
#
#     bash deploy/baremetal/deploy_inputs_test.sh
#
# Each case sources deploy.sh in a fresh subshell, as deploy_test.sh does: its
# `readonly` constants can be assigned once per shell, and its `set -e` would
# abort on the non-zero returns these cases assert on. `install` is stubbed on
# PATH and drops a sentinel, so a case can assert a refused deploy never
# reached it: every write the deploy makes to the host goes through it first.
#
# The bundle case needs GNU tar, the tar every runner host ships. It SKIPS on
# a machine without it and hard-fails when CI is set, so the extraction is
# always proven on the ubuntu-latest runners that gate a merge.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
readonly DEPLOY_SH="$SCRIPT_DIR/deploy.sh"
# Where deploy.sh reads the runner's env file; a host that has one would
# answer the env case with its own file instead of the refusal it expects.
readonly HOST_ENV_FILE="/opt/agentsfleet/.env"
readonly SENTINEL_INSTALL="install-ran"
readonly FIXTURE_DIGEST="0000000000000000000000000000000000000000000000000000000000000001"
readonly TOOLBOX_PARTS=(erofs json json.sig)
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

# A directory holding the toolbox set, without `skip` if given.
toolbox_set() {
  local dir="$1" skip="${2:-}" part
  mkdir -p "$dir"
  for part in "${TOOLBOX_PARTS[@]}"; do
    [[ "$part" == "$skip" ]] || printf '%s\n' "$part" >"$dir/toolbox-$FIXTURE_DIGEST.$part"
  done
}

# Runs deploy.sh's main in local mode with the lock taken as given, and leaves
# any sentinel under `sentinels`.
local_deploy_status() {
  local sentinels="$1" binary="$2" toolbox="$3"
  mkdir -p "$sentinels"
  (
    export SENTINEL_DIR="$sentinels" PATH="$STUB_DIR:$PATH"
    # shellcheck source=./deploy.sh
    source "$DEPLOY_SH" >/dev/null 2>&1
    set +e
    acquire_deploy_lock() { :; }
    main runner v9.9.9 "$binary" "$toolbox" >/dev/null 2>&1
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
  local binary="$WORK_DIR/binary" toolbox="$WORK_DIR/short-toolbox" sentinels="$WORK_DIR/toolbox-refused"
  : >"$binary"
  toolbox_set "$toolbox" json.sig
  if local_deploy_status "$sentinels" "$binary" "$toolbox"; then
    bad "$name" "a deploy whose toolbox lacks its signature exited 0"
  elif [[ -e "$sentinels/$SENTINEL_INSTALL" ]]; then
    bad "$name" "a deploy refused for its toolbox wrote to the host first"
  else
    ok "$name"
  fi
}

test_deploy_refused_env_leaves_the_host_untouched() {
  local name="test_deploy_refused_env_leaves_the_host_untouched"
  local binary="$WORK_DIR/binary" toolbox="$WORK_DIR/full-toolbox" sentinels="$WORK_DIR/env-refused"
  if [[ -e "$HOST_ENV_FILE" ]]; then
    skip "$name" "$HOST_ENV_FILE exists here, so the deploy would not refuse it"
    return
  fi
  : >"$binary"
  toolbox_set "$toolbox"
  if local_deploy_status "$sentinels" "$binary" "$toolbox"; then
    bad "$name" "a deploy with no env file at $HOST_ENV_FILE exited 0"
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

if tar --version 2>/dev/null | grep -q 'GNU tar'; then
  test_deploy_bundle_keeps_its_directory_private
elif [[ -n "${CI:-}" ]]; then
  bad "test_deploy_bundle_keeps_its_directory_private" "GNU tar not found on a CI runner — the bundle extraction must be proven here"
else
  skip "test_deploy_bundle_keeps_its_directory_private" "GNU tar not installed (it is the tar every runner host ships)"
fi

printf '\n%d passed, %d failed, %d skipped\n' "$passed" "$failed" "$skipped"
[[ "$failed" -eq 0 ]]
