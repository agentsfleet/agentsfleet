#!/bin/sh
# cgroup-delegate.sh — prepare a cgroup-v2 controller subtree for the runner's
# kernel-enforcement integration lane (M100 runner GA hardening). Idempotent. Requires a
# privileged context (CAP_SYS_ADMIN) with a writable cgroup-v2 mount. Both the
# CI step and `make test-enforcement-docker` source this — one source of truth.
#
# cgroup v2 forbids a non-root cgroup from holding member processes AND enabling
# controllers for its children ("no internal processes"). So we drain every
# process in the current cgroup into an `init` leaf, then enable the controllers
# on the (now process-free) cgroup so child scopes — fleet.runner/exec-* — inherit
# memory/pids/cpu. CgroupScope.create then writes memory.max / pids.max there.
set -eu
CG="${CGROUP_ROOT:-/sys/fs/cgroup}"

if [ ! -w "$CG/cgroup.subtree_control" ]; then
  echo "cgroup-delegate: $CG/cgroup.subtree_control not writable — skipping (lane will SkipZigTest)" >&2
  exit 0
fi

mkdir -p "$CG/init"
# Drain processes out of the root cgroup into the init leaf. `cgroup.procs` is a
# snapshot taken at open(2): a process that forks mid-drain stays behind and
# would make the subtree_control write below fail EBUSY ("no internal
# processes"). Re-drain until the root reads empty, bounded so a pathological
# fork storm can't spin forever.
attempt=0
while [ "$attempt" -lt 10 ]; do
  attempt=$((attempt + 1))
  drained=0
  while read -r pid; do
    echo "$pid" > "$CG/init/cgroup.procs" 2>/dev/null || true
    drained=1
  done < "$CG/cgroup.procs"
  if [ "$drained" = 0 ]; then break; fi
done

# Enable the controllers the runner needs for its child scopes. No `|| true`:
# under `set -e` an EBUSY here aborts loudly. Then verify each controller is
# actually present so a partial enable surfaces now, not later as an opaque
# CgroupScope.create failure with no SkipZigTest.
echo "+cpu +memory +pids" > "$CG/cgroup.subtree_control"
sc="$(cat "$CG/cgroup.subtree_control")"
for c in cpu memory pids; do
  case " $sc " in
    *" $c "*) : ;;
    *) echo "cgroup-delegate: controller '$c' not enabled after write (subtree_control='$sc')" >&2; exit 1 ;;
  esac
done

# Pre-create the runner base with delegation so exec-<id> scopes inherit them.
mkdir -p "$CG/fleet.runner"

# Sweep stale per-exec scopes a crashed/aborted prior run may have left behind: a
# create() that fails partway leaves an orphan exec-<id> dir, and the next create()
# trips on it. Each is process-free once its child died. `-depth` removes leaf-first;
# `-exec rmdir` is whitespace-safe (no word-splitting). Idempotent and CI-safe — a
# previous run's debris never fails the next.
find "$CG/fleet.runner" -mindepth 1 -depth -type d -exec rmdir {} \; 2>/dev/null || true

echo "+cpu +memory +pids" > "$CG/fleet.runner/cgroup.subtree_control"

echo "cgroup-delegate: controllers ready: $(cat "$CG/cgroup.subtree_control")" >&2
