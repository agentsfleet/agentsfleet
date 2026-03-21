#!/usr/bin/env bash
# M2_001 credential validation runner.
#
# Milestone: M2
# Workstream: 001
# Sections:
#   1 -> startup preflight (tooling/auth prerequisites)
#   2 -> procurement readiness gate (required refs + role separation checks)
#
# Usage:
#   ./scripts/checks/m2_001/run.sh    # runs all sections, both environments

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

export ENV="${ENV:-all}"
export VAULT_DEV="${VAULT_DEV:-ZMB_CD_DEV}"
export VAULT_PROD="${VAULT_PROD:-ZMB_CD_PROD}"
SECTIONS="${SECTIONS:-1,2}"

run_section() {
  local section="$1"
  local script=""
  case "$section" in
    1) script="$SCRIPT_DIR/section-1-preflight.sh" ;;
    2) script="$SCRIPT_DIR/section-2-procurement-readiness-gate.sh" ;;
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

echo "✅ M2_001 check complete (sections: $SECTIONS, env: $ENV)"
