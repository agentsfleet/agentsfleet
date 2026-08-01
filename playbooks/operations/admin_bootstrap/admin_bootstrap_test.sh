#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
playbook="$repo_root/playbooks/operations/admin_bootstrap/001_playbook.md"
auth_doc="$repo_root/docs/AUTH.md"
dashboard_action="$repo_root/ui/packages/app/app/(dashboard)/admin/models/actions.ts"

passed=0
failed=0

pass() {
  passed=$((passed + 1))
  echo "  ✓ $1"
}

fail() {
  failed=$((failed + 1))
  echo "  ✗ $1"
}

assert_contains() {
  local file="$1"
  local literal="$2"
  local label="$3"
  if rg --fixed-strings --quiet "$literal" "$file"; then
    pass "$label"
  else
    fail "$label"
  fi
}

assert_absent() {
  local file="$1"
  local pattern="$2"
  local label="$3"
  if rg --quiet "$pattern" "$file"; then
    fail "$label"
  else
    pass "$label"
  fi
}

echo "admin bootstrap regression tests"

operator_scopes="runner:enroll runner:write stream:read model:admin platform-key:admin platform-library:write workspace:any"
assert_contains "$auth_doc" "$operator_scopes" \
  "authorization guide carries the canonical operator scope bundle"
assert_contains "$playbook" "$operator_scopes" \
  "playbook uses the canonical operator scope bundle"
assert_absent "$playbook" 'public[Mm]etadata\.role|public_metadata\.role|role=admin' \
  "retired role metadata is absent"
assert_contains "$playbook" \
  'This key has tenant scopes only.' \
  "tenant provisioning key is not presented as a platform credential"
assert_absent "$playbook" \
  'Authorization: Bearer|/v1/admin/platform-keys' \
  "playbook does not drive platform routes with a long-lived key"
assert_contains "$dashboard_action" \
  'export async function setPlatformDefaultAction' \
  "dashboard owns platform-default setup"
assert_contains "$playbook" \
  'PLATFORM_ADMIN_WORKSPACE_ID=$workspace_id' \
  "runtime admin-workspace pointer is applied after signup"
assert_contains "$playbook" \
  'https://app-dev.agentsfleet.net' \
  "development dashboard target is current"
assert_contains "$playbook" \
  'https://app.agentsfleet.net' \
  "production dashboard target is current"

echo ""
echo "results: $passed passed, $failed failed"
test "$failed" -eq 0
