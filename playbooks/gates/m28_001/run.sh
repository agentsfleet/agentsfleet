#!/usr/bin/env bash
# M28_001 Grafana observability gate runner.
#
# Sections:
#   1 -> Credentials check (vault items exist)
#   2 -> Prometheus scrape (zombie_* metrics reachable)
#   3 -> Dashboard import + verify panels
#
# Usage:
#   ./playbooks/gates/m28_001/run.sh              # runs all sections
#   SECTIONS=1,2 ./playbooks/gates/m28_001/run.sh # run specific sections

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

export VAULT_DEV="${VAULT_DEV:-ZMB_CD_DEV}"
SECTIONS="${SECTIONS:-1,2,3}"

run_section() {
  local section="$1"
  local script=""
  case "$section" in
    1) script="$SCRIPT_DIR/section-1-credentials.sh" ;;
    2) script="$SCRIPT_DIR/section-2-prometheus.sh" ;;
    3) script="$SCRIPT_DIR/section-3-dashboard.sh" ;;
    *)
      echo "Unknown section: $section (supported: 1,2,3)" >&2
      return 1
      ;;
  esac
  echo "── section $section: $(basename "$script" .sh) ──"
  bash "$script"
}

IFS=',' read -ra requested <<< "$SECTIONS"
passed=0
failed=0
for s in "${requested[@]}"; do
  if run_section "$s"; then
    passed=$((passed + 1))
  else
    failed=$((failed + 1))
  fi
done

echo ""
echo "── M28_001 gate: $passed passed, $failed failed ──"
[ "$failed" -eq 0 ]
