#!/usr/bin/env bash
# M4_001 worker bootstrap DEV validation runner.
#
# Milestone: M4
# Workstream: 001
# Sections:
#   1 -> SSH access (vault keys + connectivity)
#   2 -> host readiness (Tailscale, bubblewrap, git, cgroups v2, OpenSSL)
#   3 -> deploy readiness (/opt/zombie/ structure, systemd units)
#
# Usage:
#   ./playbooks/gates/m4_001/run.sh              # runs all sections
#   SECTIONS=1,2 ./playbooks/gates/m4_001/run.sh # run specific sections

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

export VAULT_DEV="${VAULT_DEV:-ZMB_CD_DEV}"
SECTIONS="${SECTIONS:-1,2,3}"

run_section() {
  local section="$1"
  local script=""
  case "$section" in
    1) script="$SCRIPT_DIR/section-1-ssh-access.sh" ;;
    2) script="$SCRIPT_DIR/section-2-host-readiness.sh" ;;
    3) script="$SCRIPT_DIR/section-3-deploy-readiness.sh" ;;
    *)
      echo "Unknown section: $section (supported: 1,2,3)" >&2
      return 1
      ;;
  esac

  if [ ! -x "$script" ]; then
    echo "Missing executable section script: $script" >&2
    return 1
  fi

  "$script"
}

IFS=',' read -r -a section_list <<< "$SECTIONS"
for section in "${section_list[@]}"; do
  run_section "$section"
done

echo "✅ M4_001 check complete (sections: $SECTIONS)"
