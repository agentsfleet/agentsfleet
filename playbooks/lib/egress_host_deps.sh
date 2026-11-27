#!/usr/bin/env bash
# egress_host_deps.sh — sourced probe for what a runner host must hold to wall
# a run in and hold its egress. Each run's boundary is a Firecracker microVM in
# a network namespace of its own, reached through a tap the host guards with an
# nftables allowlist. The engine first reads its programs and guest files when
# it boots a microVM, so a host missing one starts the runner and then fails its
# runs. This probe records the installed versions and fails loud on a missing
# piece, so such a host is caught at readiness, not at first run.
#
# Usage (the caller already has an SSH wrapper named `remote_cmd`):
#   source "$(dirname "$0")/../../lib/egress_host_deps.sh"
#   egress_probe_remote remote_cmd || missing=$((missing + 1))
#
# Requirements asserted:
#   - firecracker and jailer: the monitor and the program that starts it in a
#     jail and in the run's network namespace. Versions recorded.
#   - the guest kernel and the guest's initial file system every microVM boots.
#   - /dev/kvm and /dev/net/tun: the devices a monitor and its tap are made on.
#   - nftables (`nft`) and iproute2 (`ip`): the runner writes its tables and
#     links over netlink itself. The runner collector's unit loads its own
#     table with `nft` (deploy/baremetal/otelcol-runner.service), and an
#     operator reads a host's tables and links with the pair.
# The runner runs as root (deploy/baremetal/agentsfleet-runner.service), so
# this probe proves each piece is present, and the runner's own boot proves it
# opens: it exits naming `kvm` when /dev/kvm is denied.

# Probes the engine's host pieces over an existing remote-command function.
# Prints one ✓/✗ line per piece, with its version where it has one; returns 0
# iff every piece is present.
egress_probe_remote() {
  local remote_cmd_fn="$1"
  # Where the engine reads each from unless `[firecracker]` in the runner's file
  # names another (`afr_engine_firecracker`'s `config.rs`). The deployed file
  # (deploy/baremetal/agentsfleet-runner.toml) names none.
  local firecracker="/usr/local/bin/firecracker"
  local jailer="/usr/local/bin/jailer"
  local kernel="/usr/local/lib/agentsfleet/vmlinux"
  local initrd="/usr/local/lib/agentsfleet/guest.cpio"
  local kvm="/dev/kvm"
  local tun="/dev/net/tun"
  local report
  # One round trip: emit KEY=value lines the caller parses locally. Each
  # piece's absence yields an explicit MISSING marker rather than a non-zero
  # exit that would abort the whole readiness run.
  # shellcheck disable=SC2016  # the block runs on the REMOTE host — must not expand locally
  report="$(
    "$remote_cmd_fn" '
      # nft and ip live in /usr/sbin; a non-interactive SSH PATH on Debian omits
      # it, so a present nftables would otherwise read as MISSING. Prepend sbin
      # (expands on the REMOTE host, not locally) before the command -v probes.
      export PATH="/usr/sbin:/sbin:$PATH"
      if [ -x "'"$firecracker"'" ]; then
        printf "FIRECRACKER_VERSION=%s\n" "$("'"$firecracker"'" --version 2>&1 | head -1)"
      else
        echo "FIRECRACKER_VERSION=MISSING"
      fi
      if [ -x "'"$jailer"'" ]; then
        printf "JAILER_VERSION=%s\n" "$("'"$jailer"'" --version 2>&1 | head -1)"
      else
        echo "JAILER_VERSION=MISSING"
      fi
      if [ -f "'"$kernel"'" ]; then echo "GUEST_KERNEL=1"; else echo "GUEST_KERNEL=0"; fi
      if [ -f "'"$initrd"'" ]; then echo "GUEST_INITRD=1"; else echo "GUEST_INITRD=0"; fi
      if [ -c "'"$kvm"'" ]; then echo "KVM_DEVICE=1"; else echo "KVM_DEVICE=0"; fi
      if [ -c "'"$tun"'" ]; then echo "TUN_DEVICE=1"; else echo "TUN_DEVICE=0"; fi
      if command -v nft >/dev/null 2>&1; then
        printf "NFT_VERSION=%s\n" "$(nft --version 2>&1 | head -1)"
      else
        echo "NFT_VERSION=MISSING"
      fi
      if command -v ip >/dev/null 2>&1; then
        printf "IP_VERSION=%s\n" "$(ip -V 2>&1 | head -1)"
      else
        echo "IP_VERSION=MISSING"
      fi
    '
  )"

  local firecracker_version jailer_version guest_kernel guest_initrd kvm_device tun_device
  local nft_version ip_version
  firecracker_version="$(printf '%s\n' "$report" | sed -n 's/^FIRECRACKER_VERSION=//p')"
  jailer_version="$(printf '%s\n' "$report" | sed -n 's/^JAILER_VERSION=//p')"
  guest_kernel="$(printf '%s\n' "$report" | sed -n 's/^GUEST_KERNEL=//p')"
  guest_initrd="$(printf '%s\n' "$report" | sed -n 's/^GUEST_INITRD=//p')"
  kvm_device="$(printf '%s\n' "$report" | sed -n 's/^KVM_DEVICE=//p')"
  tun_device="$(printf '%s\n' "$report" | sed -n 's/^TUN_DEVICE=//p')"
  nft_version="$(printf '%s\n' "$report" | sed -n 's/^NFT_VERSION=//p')"
  ip_version="$(printf '%s\n' "$report" | sed -n 's/^IP_VERSION=//p')"

  local missing=0

  if [ "$firecracker_version" = "MISSING" ] || [ -z "$firecracker_version" ]; then
    echo "  ✗ firecracker: not installed at $firecracker (no microVM can boot)"
    missing=$((missing + 1))
  else
    echo "  ✓ firecracker: $firecracker_version"
  fi

  if [ "$jailer_version" = "MISSING" ] || [ -z "$jailer_version" ]; then
    echo "  ✗ jailer: not installed at $jailer (no monitor can be jailed)"
    missing=$((missing + 1))
  else
    echo "  ✓ jailer: $jailer_version"
  fi

  if [ "$guest_kernel" != "1" ]; then
    echo "  ✗ guest kernel: missing at $kernel (no microVM can boot)"
    missing=$((missing + 1))
  else
    echo "  ✓ guest kernel: $kernel"
  fi

  if [ "$guest_initrd" != "1" ]; then
    echo "  ✗ guest initial file system: missing at $initrd (no microVM can boot)"
    missing=$((missing + 1))
  else
    echo "  ✓ guest initial file system: $initrd"
  fi

  if [ "$kvm_device" != "1" ]; then
    echo "  ✗ kvm: $kvm is not a device (enable virtualization; the runner exits naming kvm)"
    missing=$((missing + 1))
  else
    echo "  ✓ kvm: $kvm"
  fi

  if [ "$tun_device" != "1" ]; then
    echo "  ✗ tun: $tun is not a device (no microVM can reach a network)"
    missing=$((missing + 1))
  else
    echo "  ✓ tun: $tun"
  fi

  if [ "$nft_version" = "MISSING" ] || [ -z "$nft_version" ]; then
    echo "  ✗ nftables: not installed (apt install nftables)"
    missing=$((missing + 1))
  else
    echo "  ✓ nftables: $nft_version"
  fi

  if [ "$ip_version" = "MISSING" ] || [ -z "$ip_version" ]; then
    echo "  ✗ iproute2: not installed (apt install iproute2)"
    missing=$((missing + 1))
  else
    echo "  ✓ iproute2: $ip_version"
  fi

  [ "$missing" -eq 0 ]
}
