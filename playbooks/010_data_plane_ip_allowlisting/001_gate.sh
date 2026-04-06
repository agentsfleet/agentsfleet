#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SECTIONS="${SECTIONS:-1,2}"

run_section() {
  local section="$1"
  case "$section" in
    1) "$SCRIPT_DIR/001_gate_section_1.sh" ;;
    2) "$SCRIPT_DIR/002_gate_section_2.sh" ;;
    *)
      echo "Unknown section: $section (supported: 1,2)" >&2
      return 1
      ;;
  esac
}

IFS=',' read -r -a section_list <<< "$SECTIONS"
for section in "${section_list[@]}"; do
  run_section "$section"
done

echo "M29_001 gate complete (sections: $SECTIONS)"
