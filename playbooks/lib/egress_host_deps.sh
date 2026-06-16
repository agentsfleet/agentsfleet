#!/usr/bin/env bash
# egress_host_deps.sh — sourced probe for the runner egress host
# dependencies. The per-lease egress boundary (own netns + veth + host-side
# nftables IP-allowlist) needs THREE host packages plus a capable bubblewrap on
# every runner box — none of which `deploy/baremetal/deploy.sh` installs (it
# ships only the binary). This probe records the installed versions and fails
# loud when a dep is missing or a bwrap lacks the namespace-handoff flags, so a
# box that cannot enforce egress is caught at readiness, not at first lease.
#
# Usage (the caller already has an SSH wrapper named `remote_cmd`):
#   source "$(dirname "$0")/../../lib/egress_host_deps.sh"
#   egress_probe_remote remote_cmd || missing=$((missing + 1))
#
# Requirements asserted:
#   - bubblewrap: present AND exposes --info-fd + --block-fd (the bwrap↔netns
#     handoff: bwrap reports the sandbox pid on --info-fd and pauses on
#     --block-fd until the parent moves the veth in). Present since bwrap 0.8;
#     prod boxes run 0.11 (latest). Version recorded, flag support asserted.
#   - nftables (`nft`): the kernel egress allowlist is nft rules on the veth.
#   - iproute2 (`ip`): veth pair + netns plumbing.
# CAP_NET_ADMIN is granted to the service by the systemd unit, not a package —
# verified separately at deploy (unit AmbientCapabilities) + at runtime by the
# runner's own capability probe; this script checks the binaries + versions.

# Probe egress host deps over an existing remote-command function. Prints one
# ✓/✗ line per dep with its version; returns 0 iff every dep is present and
# bubblewrap exposes both handoff flags.
egress_probe_remote() {
  local remote_cmd_fn="$1"
  local report
  # One round trip: emit KEY=value lines the caller parses locally. Each tool's
  # absence yields an explicit MISSING marker rather than a non-zero exit that
  # would abort the whole readiness run.
  # shellcheck disable=SC2016  # the block runs on the REMOTE host — must not expand locally
  report="$(
    "$remote_cmd_fn" '
      # nft and ip live in /usr/sbin; a non-interactive SSH PATH on Debian omits
      # it, so a present nftables would otherwise read as MISSING. Prepend sbin
      # (expands on the REMOTE host, not locally) before the command -v probes.
      export PATH="/usr/sbin:/sbin:$PATH"
      if command -v bwrap >/dev/null 2>&1; then
        printf "BWRAP_VERSION=%s\n" "$(bwrap --version 2>&1 | head -1)"
        bwrap --help 2>&1 | grep -q -- "--info-fd"  && echo "BWRAP_INFO_FD=1"  || echo "BWRAP_INFO_FD=0"
        bwrap --help 2>&1 | grep -q -- "--block-fd" && echo "BWRAP_BLOCK_FD=1" || echo "BWRAP_BLOCK_FD=0"
      else
        echo "BWRAP_VERSION=MISSING"
      fi
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

  local bwrap_version info_fd block_fd nft_version ip_version
  bwrap_version="$(printf '%s\n' "$report" | sed -n 's/^BWRAP_VERSION=//p')"
  info_fd="$(printf '%s\n' "$report" | sed -n 's/^BWRAP_INFO_FD=//p')"
  block_fd="$(printf '%s\n' "$report" | sed -n 's/^BWRAP_BLOCK_FD=//p')"
  nft_version="$(printf '%s\n' "$report" | sed -n 's/^NFT_VERSION=//p')"
  ip_version="$(printf '%s\n' "$report" | sed -n 's/^IP_VERSION=//p')"

  local missing=0

  if [ "$bwrap_version" = "MISSING" ] || [ -z "$bwrap_version" ]; then
    echo "  ✗ bubblewrap: not installed (egress sandbox cannot be entered)"
    missing=$((missing + 1))
  elif [ "$info_fd" != "1" ] || [ "$block_fd" != "1" ]; then
    echo "  ✗ bubblewrap: $bwrap_version — missing --info-fd/--block-fd (no netns handoff; need ≥ 0.8)"
    missing=$((missing + 1))
  else
    echo "  ✓ bubblewrap: $bwrap_version (--info-fd + --block-fd present)"
  fi

  if [ "$nft_version" = "MISSING" ] || [ -z "$nft_version" ]; then
    echo "  ✗ nftables: not installed (no kernel egress allowlist — apt install nftables)"
    missing=$((missing + 1))
  else
    echo "  ✓ nftables: $nft_version"
  fi

  if [ "$ip_version" = "MISSING" ] || [ -z "$ip_version" ]; then
    echo "  ✗ iproute2: not installed (no veth/netns plumbing — apt install iproute2)"
    missing=$((missing + 1))
  else
    echo "  ✓ iproute2: $ip_version"
  fi

  [ "$missing" -eq 0 ]
}
