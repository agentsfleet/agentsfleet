#!/usr/bin/env bash
# Container-local delegation mirrors the shipping systemd unit's runner subgroup.
set -euo pipefail
root=/sys/fs/cgroup
mkdir -p "$root/agentsfleet/runner"
while read -r pid; do
  printf '%s\n' "$pid" >"$root/agentsfleet/runner/cgroup.procs"
done <"$root/cgroup.procs"
printf '+cpu +io +memory +pids\n' >"$root/cgroup.subtree_control"
printf '+cpu +io +memory +pids\n' >"$root/agentsfleet/cgroup.subtree_control"
if ! mountpoint -q /sys/kernel/security; then
  mount -t securityfs securityfs /sys/kernel/security
fi
if [ ! -e /dev/loop-control ]; then mknod -m 0600 /dev/loop-control c 10 237; fi
storage=/var/lib/agentsfleet-runner
mkdir -p "$storage/toolbox/incoming"
for image in /toolbox/toolbox-*.erofs; do
  [ -f "$image" ] || { printf 'signed toolbox image is missing\n' >&2; exit 1; }
  for suffix in erofs json json.sig; do
    file="${image%.erofs}.$suffix"
    install -m 0644 "$file" "$storage/toolbox/incoming/$(basename "$file")"
  done
done
cat /etc/ssl/certs/ca-certificates.crt /acceptance-ca.crt >"$storage/ca.pem"
if [ -f /run/session-ca.pem ]; then cat /run/session-ca.pem >>"$storage/ca.pem"; fi
exec /usr/local/bin/agentsfleet-runner run
