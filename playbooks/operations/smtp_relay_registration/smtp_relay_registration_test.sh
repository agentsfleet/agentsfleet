#!/usr/bin/env bash
# The invite email's operator surface stays in step: docs/AUTH.md names the
# three email states and the smtp-relay bag, and this playbook names every
# field the sync script maps.
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/../../.." && pwd)"
playbook="$script_dir/001_playbook.md"
auth_doc="$repo_root/docs/AUTH.md"

passed=0
failed=0

check() {
  local file="$1"
  local literal="$2"
  local label="$3"
  if grep --fixed-strings --quiet -- "$literal" "$file"; then
    passed=$((passed + 1))
    echo "  ✓ $label"
  else
    failed=$((failed + 1))
    echo "  ✗ $label: $file lacks $literal" >&2
  fi
}

test_auth_doc_names_email_states() {
  local state
  for state in sent failed unconfigured; do
    check "$auth_doc" "| \`$state\` |" "AUTH.md names the $state email state"
  done
  check "$auth_doc" "\`smtp-relay\` platform bag" "AUTH.md names the smtp-relay bag"
  check "$auth_doc" "UZ-INV-005" "AUTH.md lists UZ-INV-005"
}

test_playbook_names_every_bag_field() {
  local field
  for field in host port username password from_address; do
    check "$playbook" "- \`$field\`" "the playbook names $field"
  done
}

echo "smtp-relay registration regression tests"
test_auth_doc_names_email_states
test_playbook_names_every_bag_field

echo ""
echo "results: $passed passed, $failed failed"
test "$failed" -eq 0
