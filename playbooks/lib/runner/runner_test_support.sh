#!/usr/bin/env bash
# Stub harness shared by the runner playbook suites.
#
#     source "$(dirname "${BASH_SOURCE[0]}")/runner_test_support.sh"
#
# Split out of runner_test.sh when the verify-lane cases pushed that file past
# the 350-line cap. The `op` and `tailscale` stubs plus the hermetic
# `run_script` launcher are the entire world those suites execute in, and
# runner_test.sh and runner_verify_test.sh need them identically — a second
# copy would drift the moment one suite taught its stub a new answer.

set -uo pipefail

RUNNER_TEST_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
readonly PREPARE="$RUNNER_TEST_DIR/prepare.sh"
readonly DEPLOY="$RUNNER_TEST_DIR/deploy.sh"
readonly VERIFY="$RUNNER_TEST_DIR/verify.sh"
# The cgroup controllers the runner lanes require, as common.sh declares them,
# read in a subshell so the library's `set -e` stays out of this harness. The
# stub's cgroup report and the cases hand back exactly these.
# shellcheck source=./common.sh
required_controllers="$(source "$RUNNER_TEST_DIR/common.sh" && printf '%s' "$REQUIRED_CGROUP_CONTROLLERS")"
readonly required_controllers
# Public certificate only, copied from boring 5.4.0 test/root-ca.pem (Apache-2.0).
# Source and license: https://github.com/cloudflare/boring/tree/309bf35b9eb97fc21040915a5f4a886941965c47/boring
readonly stream_ca_fixture="$RUNNER_TEST_DIR/fixtures/stream-ca.pem"
if [ -z "$required_controllers" ]; then
  printf 'FATAL: REQUIRED_CGROUP_CONTROLLERS read empty from %s\n' "$RUNNER_TEST_DIR/common.sh" >&2
  exit 2
fi
passed=0
failed=0
work_dir="$(mktemp -d)"
stub_dir="$work_dir/bin"
calls="$work_dir/calls"
readyz_counter="$work_dir/readyz-attempts"
mkdir -p "$stub_dir"
trap 'rm -rf "$work_dir"' EXIT

ok() { printf 'ok   %s\n' "$1"; passed=$((passed + 1)); }
bad() { printf 'FAIL %s\n       %s\n' "$1" "$2" >&2; failed=$((failed + 1)); }

report_results() {
  printf '\n%d passed, %d failed\n' "$passed" "$failed"
  [ "$failed" -eq 0 ]
}

cat >"$stub_dir/op" <<'STUB'
#!/usr/bin/env bash
printf '%s\n' "$*" >>"$CALLS"
case "${1:-}" in
  whoami) printf 'stub-user\n' ;;
  read)
    case "${2:-}" in
      */tailscale-hostname) printf 'runner-host\n' ;;
      */deploy-user) printf 'runner-user\n' ;;
      */ca-certificate)
        [ "${STUB_MISSING_CA:-0}" != 1 ] || exit 1
        if [ "${STUB_CORRUPT_CA:-0}" = 1 ]; then
          printf '%s\n' '-----BEGIN CERTIFICATE-----' 'not a certificate' '-----END CERTIFICATE-----'
        else
          cat "$STUB_STREAM_CA"
        fi
        ;;
      */wireguard-config)
        [ "${STUB_MISSING_WIREGUARD:-0}" != 1 ] || exit 1
        printf '%s\n' 'fixture-wireguard-config'
        ;;
      */server-name) printf '%s\n' "${STUB_STREAM_SERVER_NAME:-agentsfleetd-$ENV.internal}" ;;
      */runner-token) printf '%s\n' "${STUB_TOKEN:-agt_rREAL_TOKEN}" ;;
      *) exit 1 ;;
    esac
    ;;
  *) exit 1 ;;
esac
STUB

cat >"$stub_dir/tailscale" <<'STUB'
#!/usr/bin/env bash
printf '%s\n' "$*" >>"$CALLS"
if [ "${1:-}" = status ]; then
  printf '{"Self":{"Online":true}}\n'
  exit 0
fi
command="${*: -1}"
if [ -n "${TAILSCALE_FAIL_MATCH:-}" ] && [[ "$command" == *"$TAILSCALE_FAIL_MATCH"* ]]; then
  exit 9
fi
if [ "${STUB_EXECUTE_CGROUP_GATE:-0}" = 1 ] &&
  [[ "$command" == *"/sys/fs/cgroup/cgroup.controllers"* ]]
then
  command="${command//\/sys\/fs\/cgroup/$STUB_CGROUP_ROOT}"
  bash -c "$command"
  exit $?
fi
if [ "${STUB_EXECUTE_WIREGUARD:-0}" = 1 ] && [[ "$command" == *"/etc/wireguard/.fly-dev."* ]]; then
  command="${command//\/etc\/wireguard/$STUB_WIREGUARD_DIR}"
  bash -c "$command"
  exit $?
fi
case "$command" in
  *"http://agentsfleetd-dev.internal:3000/readyz"*) exit 0 ;;
  # What OpenSSL prints for each carrier: over QUIC the version reads QUICv1
  # and no group line appears, which a TLS-shaped fake once hid from the tests.
  *"openssl s_client -brief"*"-quic"*)
    printf '%s\n' 'CONNECTION ESTABLISHED' 'Protocol version: QUICv1' 'Verification: OK'
    ;;
  *"openssl s_client -brief"*)
    printf '%s\n' 'CONNECTION ESTABLISHED' 'Protocol version: TLSv1.3' 'Verification: OK' 'Negotiated TLS1.3 group: X25519MLKEM768'
    ;;
  *"BWRAP_VERSION="*)
    printf '%s\n' \
      'BWRAP_VERSION=bwrap 0.11.0' \
      'BWRAP_INFO_FD=1' \
      'BWRAP_BLOCK_FD=1' \
      'NFT_VERSION=nftables v1.0' \
      'IP_VERSION=iproute2-6.0'
    ;;
  *"cgroup.subtree_control"*)
    printf '%s\n' \
      "root_controllers=$STUB_REQUIRED_CONTROLLERS" \
      "root_subtree=$STUB_REQUIRED_CONTROLLERS" \
      "slice_controllers=$STUB_REQUIRED_CONTROLLERS" \
      "slice_subtree=$STUB_REQUIRED_CONTROLLERS" \
      "service_controllers=$STUB_REQUIRED_CONTROLLERS" \
      "service_subtree=${STUB_CGROUP_CONTROLLERS:-$STUB_REQUIRED_CONTROLLERS}"
    ;;
  "cat '/proc/sys/net/ipv4/ip_forward'")
    printf '%s\n' "${STUB_IP_FORWARD:-1}"
    ;;
  *"/readyz"*)
    # One status per probe, last entry repeating, so a case can hand the
    # readiness loop a Cloudflare 530 that clears (tunnel reconnect) or one
    # that never does (real outage).
    # shellcheck disable=SC2206  # deliberate split of the space-separated list
    statuses=(${STUB_READYZ_STATUSES:-200})
    index=0
    [ -f "$READYZ_COUNTER" ] && index="$(cat "$READYZ_COUNTER")"
    printf '%s\n' "$((index + 1))" >"$READYZ_COUNTER"
    # Unquoted inside the arithmetic: bash 3.2 refuses a quoted operand there
    # ("1" - 1: operand expected), and this stub dying prints no status at all,
    # which the caller then reads as a 000 that looks like a real outage.
    [ "$index" -ge "${#statuses[@]}" ] && index=$((${#statuses[@]} - 1))
    printf '%s\n' "${statuses[index]}"
    ;;
esac
if [[ "$command" == *"/opt/agentsfleet/.env.new"* ]]; then
  cat >"$TRANSFER_ENV"
elif [[ "$command" == *"/opt/agentsfleet/runner-stream-trust.pem.new"* ]]; then
  cat >"$TRANSFER_CA"
else
  cat >/dev/null || true
fi
STUB
# Execute the real remote file-update logic in a fixture directory. Only host
# identity and systemd calls are stubbed; the pipe, compare, file modes and
# replacement order stay real.
cat >"$stub_dir/sudo" <<'STUB'
#!/usr/bin/env bash
exec "$@"
STUB
cat >"$stub_dir/chown" <<'STUB'
#!/usr/bin/env bash
exit 0
STUB
cat >"$stub_dir/systemctl" <<'STUB'
#!/usr/bin/env bash
case "$1" in
  is-active) test -f "$STUB_WIREGUARD_DIR/active" ;;
  stop)
    cp "$STUB_WIREGUARD_DIR/fly-dev.conf" "$STUB_WIREGUARD_DIR/stopped-config"
    rm -f "$STUB_WIREGUARD_DIR/active"
    printf 'stop\n' >>"$STUB_WIREGUARD_DIR/events"
    ;;
  enable)
    touch "$STUB_WIREGUARD_DIR/active"
    printf 'enable\n' >>"$STUB_WIREGUARD_DIR/events"
    ;;
  *) exit 1 ;;
esac
STUB
chmod +x "$stub_dir/op" "$stub_dir/tailscale" "$stub_dir/sudo" "$stub_dir/chown" "$stub_dir/systemctl"
runner_binary="$work_dir/agentsfleet-runner"
printf 'runner\n' >"$runner_binary"
# The toolbox a deploy ships beside the binary, as every workflow lays it out:
# the image named by its digest, its manifest and the manifest's signature.
readonly TOOLBOX_FIXTURE_DIGEST="0000000000000000000000000000000000000000000000000000000000000001"
toolbox_fixture="$work_dir/toolbox"
mkdir -p "$toolbox_fixture"
for part in erofs json json.sig; do
  printf '%s\n' "$part" >"$toolbox_fixture/toolbox-$TOOLBOX_FIXTURE_DIGEST.$part"
done
cgroup_fixture="$work_dir/cgroup"
mkdir -p "$cgroup_fixture"

run_script() {
  : >"$calls"
  rm -f "$readyz_counter" "$work_dir/transferred.env" "$work_dir/transferred-ca.pem"
  # The test must supply its whole world, so the child sees ONLY what this
  # harness assigns plus the VAR=val pairs each case passes. Cases that mean to
  # exercise a variable pass it as an argument below, which still wins.
  #
  # A shell can still export `AGENTSFLEET_API_URL` from the machine-level
  # runner file. `common.sh` reads it as
  # `${AGENTSFLEET_API_URL:-$expected_api_url}` then refuses the deploy when it
  # disagrees with ENV's endpoint. An ambient dev endpoint therefore failed the
  # ENV=prod case on a workstation while Continuous Integration — with a bare
  # environment — passed.
  #
  # `env -i` rather than `env -u` per known name: the enumerated form fixes the
  # variables someone remembered, and this file gains readers faster than it
  # gains maintainers of that list. Wiping closes the whole class, including
  # the next input the runner library learns to read. PATH is composed before
  # the wipe, so the stub dir still shadows the real tools while the system
  # tail keeps bash findable.
  env -i \
    PATH="$stub_dir:$PATH" \
    CALLS="$calls" \
    READYZ_COUNTER="$readyz_counter" \
    TRANSFER_ENV="$work_dir/transferred.env" \
    TRANSFER_CA="$work_dir/transferred-ca.pem" \
    STUB_STREAM_CA="$stream_ca_fixture" \
    STUB_WIREGUARD_DIR="$work_dir/wireguard" \
    STUB_REQUIRED_CONTROLLERS="$required_controllers" \
    RUNNER_BINARY="$runner_binary" \
    RUNNER_VERSION=test-build \
    ALLOW_VAULT_READS=1 \
    "$@" 2>&1
}
