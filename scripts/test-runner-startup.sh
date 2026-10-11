#!/usr/bin/env bash
# Exercise the shipping binary under a copy of the deployed systemd unit.
# No release signing key enters this suite: reaching release_unstaged proves
# host startup succeeded and that admission still refuses an absent toolbox.
# Successful signed admission and lease enforcement belong to the kernel lane;
# a healthy service with the release key is proved by deployment acceptance.
set -euo pipefail

readonly CONTROLLERS=(cpu io memory pids)
readonly POLL_SECONDS=0.1
readonly POLL_COUNT=300

prepare() {
  local work="$1" root="$2" mode="$3" controller commands=()
  for controller in "${CONTROLLERS[@]}"; do
    commands+=("-$controller")
  done
  printf '%s ' "${commands[@]}" > "$root/cgroup.subtree_control"
  case "$mode" in
    enabled) printf '+cpu +io +memory +pids' > "$root/cgroup.subtree_control" ;;
    partial) printf '+memory +pids' > "$root/cgroup.subtree_control" ;;
    missing-*)
      for controller in "${CONTROLLERS[@]}"; do
        if [ "$controller" != "${mode#missing-}" ]; then
          printf '+%s' "$controller" > "$root/cgroup.subtree_control"
        fi
      done
      mkdir -p "$root/narrow/runner"
      ;;
    wrong-subgroup) mkdir -p "$root/unexpected" ;;
  esac
  cat "$root/cgroup.subtree_control" > "$work/before"
}

enter() {
  local work="$1" root="$2" mode="$3" choice="$4" child="" result=0
  local args=(run)
  cat "$(dirname "$root")/cgroup.subtree_control" > "$work/ancestor-before"
  if [ "$choice" != default ]; then args+=(--engine "$choice"); fi
  case "$mode" in
    missing-*) echo "$$" > "$root/narrow/runner/cgroup.procs" ;;
    wrong-subgroup) echo "$$" > "$root/unexpected/cgroup.procs" ;;
    populated)
      sleep 300 &
      child="$!"
      echo "$child" > "$root/cgroup.procs"
      ;;
  esac
  "$work/agentsfleet-runner" "${args[@]}" || result=$?
  cat "$root/cgroup.subtree_control" > "$work/after"
  cat "$(dirname "$root")/cgroup.subtree_control" > "$work/ancestor-after"
  if [ "$mode" = restart ]; then
    echo "$result" > "$work/first-status"
    result=0
    "$work/agentsfleet-runner" "${args[@]}" || result=$?
    cat "$root/cgroup.subtree_control" > "$work/restarted"
  fi
  if [ -n "$child" ]; then kill "$child"; wait "$child" || true; fi
  return "$result"
}

case "${1:-}" in
  --prepare) shift; prepare "$@"; exit ;;
  --enter) shift; enter "$@"; exit ;;
esac

if [ "$#" -ne 2 ] || [ "$(id -u)" -ne 0 ]; then
  echo "usage (root, Linux with systemd): $0 <runner-binary> <deployed-unit>" >&2
  exit 2
fi
systemctl show --property=Version >/dev/null
readonly BINARY="$(realpath "$1")"
readonly TEMPLATE="$(realpath "$2")"
readonly WORK="$(mktemp -d /var/tmp/afr-startup.XXXXXXXX)"
readonly NAME="$(basename "$WORK")"
readonly UNIT="$NAME.service"
readonly UNIT_FILE="/run/systemd/system/$UNIT"
readonly ROOT="/sys/fs/cgroup/system.slice/$UNIT"
readonly STORAGE="/var/lib/$NAME"

cleanup() {
  systemctl stop "$UNIT" >/dev/null 2>&1 || true
  systemctl reset-failed "$UNIT" >/dev/null 2>&1 || true
  rm -f "$UNIT_FILE"
  # These are unique directories this invocation created and owns.
  rm -rf "$WORK" "$STORAGE" "$UNIT_FILE.d"
  systemctl daemon-reload
}
trap cleanup EXIT
cp "$BINARY" "$WORK/agentsfleet-runner"
cp "$0" "$WORK/startup.sh"
chmod 755 "$WORK" "$WORK/agentsfleet-runner" "$WORK/startup.sh"
printf 'Startup host: '
cat /etc/os-release
systemctl --version | head -1

isolate_unit() {
  local drop_in property drop_ins=()
  systemctl daemon-reload
  # Host-wide drop-ins must not replace the deployed unit's security settings.
  # Same-name unit-specific masks affect only this invocation's service.
  read -r -a drop_ins <<< "$(systemctl show "$UNIT" --property=DropInPaths --value)"
  mkdir -p "$UNIT_FILE.d"
  for drop_in in "${drop_ins[@]}"; do
    ln -sf /dev/null "$UNIT_FILE.d/$(basename "$drop_in")"
  done
  systemctl daemon-reload
  for property in NoNewPrivileges=yes ProtectSystem=strict ProtectHome=yes; do
    [ "$(systemctl show "$UNIT" --property="${property%%=*}")" = "$property" ] || {
      echo "test unit did not retain $property" >&2; return 1;
    }
  done
}

write_unit() {
  local mode="$1" choice="$2" extra="$3" profile=""
  if [ -n "${LLVM_PROFILE_FILE:-}" ]; then
    profile="Environment=LLVM_PROFILE_FILE=$WORK/startup-%%p-%%m.profraw"
  fi
  cp "$TEMPLATE" "$UNIT_FILE"
  cat >> "$UNIT_FILE" <<EOF

[Service]
ExecStartPre=
ExecStartPre=+/usr/bin/bash $WORK/startup.sh --prepare $WORK $ROOT $mode
ExecStart=
ExecStart=/usr/bin/bash $WORK/startup.sh --enter $WORK $ROOT $mode $choice
EnvironmentFile=
Environment=AGENTSFLEET_API_URL=http://127.0.0.1:9
Environment=AGENTSFLEET_RUNNER_TOKEN=agt_r_startup_suite
Environment=RUNNER_STORAGE_HOME=$STORAGE
Environment=HOME=/run/$NAME
RuntimeDirectory=
RuntimeDirectory=$NAME
StateDirectory=
StateDirectory=$NAME
ReadWritePaths=
ReadWritePaths=/run/$NAME $ROOT /tmp $WORK
Restart=no
StandardOutput=append:$WORK/output
StandardError=append:$WORK/output
$profile
$extra
EOF
  isolate_unit
}

assert_controllers() {
  local file="$1" controller
  for controller in "${CONTROLLERS[@]}"; do
    grep -qw "$controller" "$file" || {
      echo "missing activated $controller in $file" >&2; cat "$file"; return 1;
    }
  done
}

run_case() {
  local mode="$1" choice="$2" expected="$3" extra="${4:-}" status="${5:-1}"
  local state="" attempt
  : > "$WORK/output"
  write_unit "$mode" "$choice" "$extra"
  systemctl start "$UNIT"
  for ((attempt=0; attempt<POLL_COUNT; attempt++)); do
    state="$(systemctl show "$UNIT" --property=ActiveState --value)"
    case "$state" in failed|inactive) break ;; esac
    sleep "$POLL_SECONDS"
  done
  if [ "$state" != failed ] ||
     [ "$(systemctl show "$UNIT" --property=ExecMainStatus --value)" != "$status" ] ||
     ! grep -Fq "$expected" "$WORK/output"; then
    echo "FAIL: $choice / $mode: expected exit $status and $expected; state=$state" >&2
    cat "$WORK/output"
    systemctl status "$UNIT" --no-pager || true
    return 1
  fi
  cmp -s "$WORK/ancestor-before" "$WORK/ancestor-after" || {
    echo 'startup changed the host ancestor controllers' >&2; return 1;
  }
  case "$mode" in
    disabled|restart|populated|readonly|wrong-subgroup)
      [ -z "$(cat "$WORK/before")" ] || { echo 'fixture was already enabled' >&2; return 1; }
      ;;
  esac
  case "$mode" in
    disabled|enabled|partial|restart|no-*) assert_controllers "$WORK/after" ;;
    populated|readonly|wrong-subgroup)
      [ -z "$(cat "$WORK/after")" ] || { echo 'refused startup changed controllers' >&2; return 1; }
      ;;
    missing-*) cmp "$WORK/before" "$WORK/after" ;;
  esac
  if [ "$mode" = restart ]; then
    [ "$(cat "$WORK/first-status")" = "$status" ]
    assert_controllers "$WORK/restarted"
    [ "$(grep -Fc "$expected" "$WORK/output")" = 2 ]
  fi
  echo "PASS: $choice / $mode: exit $status; $expected"
  systemctl reset-failed "$UNIT"
}

for choice in default bubblewrap; do
  for mode in disabled enabled partial restart; do
    run_case "$mode" "$choice" 'release_unstaged'
  done
  for controller in "${CONTROLLERS[@]}"; do
    run_case "missing-$controller" "$choice" "$controller is unavailable"
  done
  run_case populated "$choice" 'Device or resource busy'
  run_case readonly "$choice" 'Read-only file system' "ReadOnlyPaths=$ROOT/cgroup.subtree_control"
  run_case wrong-subgroup "$choice" 'delegated_cgroup is unavailable'
  run_case no-bubblewrap "$choice" 'bubblewrap is unavailable' 'InaccessiblePaths=/usr/bin/bwrap'
  run_case no-landlock "$choice" 'landlock is unavailable' 'InaccessiblePaths=/sys/kernel/security/lsm'
  run_case no-seccomp "$choice" 'seccomp is unavailable' 'InaccessiblePaths=/proc/sys/kernel/seccomp/actions_avail'
  run_case no-erofs "$choice" 'erofs is unavailable' 'InaccessiblePaths=/proc/filesystems'
done
run_case invalid-engine firecracker "invalid value 'firecracker'" '' 2
if [ -n "${LLVM_PROFILE_FILE:-}" ]; then
  profiles=("$WORK"/*.profraw)
  [ -s "${profiles[0]}" ] || { echo 'startup coverage produced no profiles' >&2; exit 1; }
  cp "${profiles[@]}" "$(dirname "$LLVM_PROFILE_FILE")/"
  echo "Runner startup coverage: ${#profiles[@]} profiles collected"
fi
echo 'Runner systemd startup: 31 cases passed; 0 skipped'
