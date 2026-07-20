#!/usr/bin/env bash
# Regression tests for the platform workspace pointer in the credential gate.

set -uo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
readonly script_under_test="$script_dir/02_credentials.sh"

passed=0
failed=0

ok() { printf 'ok   %s\n' "$1"; passed=$((passed + 1)); }
bad() { printf 'FAIL %s\n       %s\n' "$1" "$2" >&2; failed=$((failed + 1)); }

work_dir="$(mktemp -d)"
readonly work_dir
readonly stub_dir="$work_dir/bin"
mkdir -p "$stub_dir"
cleanup() { rm -rf "$work_dir"; }
trap cleanup EXIT

cat >"$stub_dir/op" <<'STUB'
#!/usr/bin/env bash
ref="${2:-}"
if [[ -n "${MISSING_REF:-}" && "$ref" == "$MISSING_REF" ]]; then
  exit 1
fi
case "$ref" in
  */platform_admin_workspace_id)
    [[ -n "${PLATFORM_WORKSPACE_VALUE:-}" ]] || exit 1
    printf '%s\n' "$PLATFORM_WORKSPACE_VALUE"
    ;;
  */issuer) printf 'https://identity.example.test\n' ;;
  */migrator-connection-string) printf 'postgres-migrator\n' ;;
  */api-connection-string) printf 'postgres-api\n' ;;
  */qstash/url) printf 'https://qstash-eu-central-1.upstash.io\n' ;;
  *) printf 'stub-value\n' ;;
esac
STUB
chmod +x "$stub_dir/op"

run_gate() {
  local workspace_value="$1"
  local missing_ref="${2:-}"
  env PATH="$stub_dir:$PATH" \
    ENV=dev \
    OP_READ_RETRIES=1 \
    OP_READ_MIN_INTERVAL_SECONDS=0 \
    PLATFORM_WORKSPACE_VALUE="$workspace_value" \
    MISSING_REF="$missing_ref" \
    bash "$script_under_test" 2>&1
}

test_should_accept_uuidv7_workspace_pointer() {
  local name="test_should_accept_uuidv7_workspace_pointer"
  local output status=0
  output="$(run_gate '0190f5a2-4b2d-7c11-8d5e-2a5f31d98210')" || status=$?
  if [[ "$status" -ne 0 ]]; then
    bad "$name" "valid workspace pointer failed: $output"
  elif [[ "$output" != *"agentsfleet-admin/platform_admin_workspace_id"* ]]; then
    bad "$name" "valid pointer was not checked: $output"
  else
    ok "$name"
  fi
}

test_should_reject_missing_workspace_pointer() {
  local name="test_should_reject_missing_workspace_pointer"
  local output status=0
  output="$(run_gate '')" || status=$?
  if [[ "$status" -ne 1 ]]; then
    bad "$name" "missing pointer returned status $status: $output"
  elif [[ "$output" != *"MISSING: op://ZMB_CD_DEV/agentsfleet-admin/platform_admin_workspace_id"* ]]; then
    bad "$name" "missing pointer did not name its 1Password field: $output"
  else
    ok "$name"
  fi
}

test_should_reject_non_uuidv7_workspace_pointer() {
  local name="test_should_reject_non_uuidv7_workspace_pointer"
  local output status=0
  output="$(run_gate '0190f5a2-4b2d-4c11-8d5e-2a5f31d98210')" || status=$?
  if [[ "$status" -ne 1 ]]; then
    bad "$name" "malformed pointer returned status $status: $output"
  elif [[ "$output" != *"INVALID WORKSPACE IDENTIFIER"* ]]; then
    bad "$name" "malformed pointer did not return the specific failure: $output"
  else
    ok "$name"
  fi
}

test_should_check_runtime_connector_credentials() {
  local name="test_should_check_runtime_connector_credentials"
  local output status=0
  output="$(run_gate '0190f5a2-4b2d-7c11-8d5e-2a5f31d98210')" || status=$?
  local -a expected=(
    'approval-signing-secret/credential'
    'github-app/app_id'
    'github-app/app_slug'
    'github-app/client_id'
    'github-app/client_secret'
    'github-app/private_key_pem'
    'github-app/webhook_secret'
    'slack-app/client_id'
    'slack-app/client_secret'
    'slack-app/signing_secret'
    'zoho-app/client_id'
    'zoho-app/client_secret'
    'jira-app/client_id'
    'jira-app/client_secret'
    'linear-app/client_id'
    'linear-app/client_secret'
  )
  local ref
  if [[ "$status" -ne 0 ]]; then
    bad "$name" "complete connector inventory failed: $output"
    return
  fi
  for ref in "${expected[@]}"; do
    if [[ "$output" != *"$ref"* ]]; then
      bad "$name" "runtime credential was not checked: $ref"
      return
    fi
  done
  if [[ "$output" == *'github-app/app-id'* || "$output" == *'github-app/private-key'* ]]; then
    bad "$name" "retired GitHub Fly fields are still checked: $output"
  else
    ok "$name"
  fi
}

test_should_reject_missing_approval_signer() {
  local name="test_should_reject_missing_approval_signer"
  local ref='op://ZMB_CD_DEV/approval-signing-secret/credential'
  local output status=0
  output="$(run_gate '0190f5a2-4b2d-7c11-8d5e-2a5f31d98210' "$ref")" || status=$?
  if [[ "$status" -ne 1 ]]; then
    bad "$name" "missing signer returned status $status: $output"
  elif [[ "$output" != *"MISSING: $ref"* ]]; then
    bad "$name" "missing signer did not name its 1Password field: $output"
  else
    ok "$name"
  fi
}

test_should_reject_missing_canonical_provider_fields() {
  local name="test_should_reject_missing_canonical_provider_fields"
  local -a refs=(
    'op://ZMB_CD_DEV/github-app/private_key_pem'
    'op://ZMB_CD_DEV/slack-app/client_secret'
    'op://ZMB_CD_DEV/linear-app/client_secret'
  )
  local ref output status
  for ref in "${refs[@]}"; do
    status=0
    output="$(run_gate '0190f5a2-4b2d-7c11-8d5e-2a5f31d98210' "$ref")" || status=$?
    if [[ "$status" -ne 1 || "$output" != *"MISSING: $ref"* ]]; then
      bad "$name" "missing canonical field was not rejected: $ref: $output"
      return
    fi
  done
  ok "$name"
}

test_should_scope_deployment_credential_gates() {
  local name="test_should_scope_deployment_credential_gates"
  local repo_root workflow
  repo_root="$(cd "$script_dir/../../.." && pwd)"
  if ! grep -q 'run: ENV=dev ./playbooks/founding/02_preflight/00_gate.sh' \
      "$repo_root/.github/workflows/deploy-dev.yml"; then
    bad "$name" "development deployment does not scope the gate to dev"
    return
  elif ! grep -q 'run: ENV=prod ./playbooks/founding/02_preflight/00_gate.sh' \
      "$repo_root/.github/workflows/release.yml"; then
    bad "$name" "production release does not scope the gate to prod"
    return
  fi

  for workflow in deploy-dev.yml release.yml; do
    if ! grep -q 'APPROVAL_SIGNING_SECRET' "$repo_root/.github/workflows/$workflow"; then
      bad "$name" "$workflow does not provision the callback signer"
      return
    elif grep -Eq 'GITHUB_APP_ID|GITHUB_APP_PRIVATE_KEY' "$repo_root/.github/workflows/$workflow"; then
      bad "$name" "$workflow still provisions retired GitHub App secrets"
      return
    fi
  done
  ok "$name"
}

test_should_accept_uuidv7_workspace_pointer
test_should_reject_missing_workspace_pointer
test_should_reject_non_uuidv7_workspace_pointer
test_should_check_runtime_connector_credentials
test_should_reject_missing_approval_signer
test_should_reject_missing_canonical_provider_fields
test_should_scope_deployment_credential_gates

printf '\n%d passed, %d failed\n' "$passed" "$failed"
[[ "$failed" -eq 0 ]]
