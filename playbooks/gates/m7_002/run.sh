#!/usr/bin/env bash
# M7_002 credential rotation DEV validation runner.
#
# Milestone: M7
# Workstream: 002
# Sections:
#   1 -> vault sync (credential fields exist and are consistent)
#   2 -> service health (API + Vercel bypass after rotation)
#
# Usage:
#   ./playbooks/gates/m7_002/run.sh    # runs all sections

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

export VAULT_DEV="${VAULT_DEV:-ZMB_CD_DEV}"
export VAULT_PROD="${VAULT_PROD:-ZMB_CD_PROD}"
SECTIONS="${SECTIONS:-1,2}"

run_section() {
  local section="$1"
  local script=""
  case "$section" in
    1) script="$SCRIPT_DIR/section-1-vault-sync.sh" ;;
    2) script="$SCRIPT_DIR/section-2-service-health.sh" ;;
    *)
      echo "Unknown section: $section (supported: 1,2)" >&2
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

echo "M7_002 check complete (sections: $SECTIONS)"
