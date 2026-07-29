#!/usr/bin/env bash

set -euo pipefail

playbooks_require_tool() {
  local tool="$1"
  command -v "$tool" >/dev/null 2>&1 || {
    echo "ERROR: required tool not found: $tool" >&2
    exit 1
  }
}

playbooks_require_vault_read_approval() {
  if [ "${ALLOW_VAULT_READS:-0}" != "1" ]; then
    echo "ERROR: vault read approval required. Set ALLOW_VAULT_READS=1." >&2
    exit 1
  fi
}

playbooks_require_op_auth() {
  playbooks_require_tool op
  op whoami >/dev/null 2>&1 || {
    echo "ERROR: op not authenticated; run 'op signin'" >&2
    exit 1
  }
}

playbooks_read_ref_or_empty() {
  local ref="$1"
  op read "$ref" 2>/dev/null || true
}

playbooks_is_ipv4_cidr_json_array() {
  local payload="$1"
  printf '%s' "$payload" | jq -e '
    type == "array" and
    length > 0 and
    all(.[]; type == "string" and test("^(25[0-5]|2[0-4][0-9]|1?[0-9]{1,2})(\\.(25[0-5]|2[0-4][0-9]|1?[0-9]{1,2})){3}/([0-9]|[12][0-9]|3[0-2])$"))
  ' >/dev/null
}

# Name the layer that refused an SSH or SCP call. A worker running
# `tailscale up --ssh` hands tailnet port 22 to tailscaled, so an access
# decision made by the tailnet policy surfaces to the caller as a bare exit 255
# with no indication of which layer said no.
playbooks_explain_ssh_failure() {
  local output="$1"
  case "$output" in
    *"tailnet policy does not permit"*)
      echo "  → cause: the tailnet policy has no ssh rule matching this source." >&2
      echo "    CI joins the tailnet as a TAGGED node (tag:ci) with no user" >&2
      echo "    identity, so autogroup:member rules never match it. The policy" >&2
      echo "    needs an accept rule from tag:ci to tag:worker." >&2
      echo "    See playbooks/founding/02_preflight/tailnet-policy.hujson" >&2
      ;;
    *"Host key verification failed"*)
      echo "  → cause: the node is not advertising Tailscale SSH host keys." >&2
      echo "    Re-run 'tailscale up ... --ssh' on the worker host." >&2
      ;;
    *"Permission denied"*)
      echo "  → cause: the host's sshd rejected the key (authorized_keys path)." >&2
      echo "    Note that Tailscale SSH bypasses authorized_keys entirely on the" >&2
      echo "    tailnet address; this path only applies to the public IP." >&2
      ;;
  esac
}

# Run an ssh/scp command, and on failure print the transcript followed by the
# named cause. Returns the original exit status so `set -e` callers still die.
playbooks_ssh_run() {
  local description="$1"
  shift
  local output status=0
  output="$("$@" 2>&1)" || status=$?
  if [ "$status" -ne 0 ]; then
    echo "  ✗ ${description} failed (exit ${status})" >&2
    printf '%s\n' "$output" >&2
    playbooks_explain_ssh_failure "$output"
    return "$status"
  fi
  if [ -n "$output" ]; then
    printf '%s\n' "$output"
  fi
  return 0
}
