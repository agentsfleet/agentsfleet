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
  *) printf '%s\n' "${SECRET_SENTINEL:-stub-value}" ;;
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
    SECRET_SENTINEL='do-not-print-provider-secret' \
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
  elif [[ "$output" == *'do-not-print-provider-secret'* ]]; then
    bad "$name" "preflight emitted a provider secret value: $output"
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
    if [[ "$status" -ne 1 || "$output" != *"MISSING: $ref"* || "$output" == *'do-not-print-provider-secret'* ]]; then
      bad "$name" "missing canonical field was not rejected: $ref: $output"
      return
    fi
  done
  ok "$name"
}

test_should_scope_deployment_credential_gates() {
  local name="test_should_scope_deployment_credential_gates"
  local repo_root workflow vault
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
    vault="VAULT_DEV"
    [[ "$workflow" == "release.yml" ]] && vault="VAULT_PROD"
    # The dollar expression is the literal Fly command contract, not shell input.
    # shellcheck disable=SC2016
    if ! grep -Fq "APPROVAL_SIGNING_SECRET: op://\${{ vars.$vault }}/approval-signing-secret/credential" "$repo_root/.github/workflows/$workflow"; then
      bad "$name" "$workflow does not load the environment-scoped callback signer"
      return
    elif ! grep -Fq 'APPROVAL_SIGNING_SECRET="$APPROVAL_SIGNING_SECRET"' "$repo_root/.github/workflows/$workflow"; then
      bad "$name" "$workflow does not pass the callback signer to Fly"
      return
    elif grep -Eq 'GITHUB_APP_ID|GITHUB_APP_PRIVATE_KEY' "$repo_root/.github/workflows/$workflow"; then
      bad "$name" "$workflow still provisions retired GitHub App secrets"
      return
    fi
  done
  ok "$name"
}

test_should_pin_issue_tracker_registration_contracts() {
  local name="test_should_pin_issue_tracker_registration_contracts"
  local repo_root jira_spec linear_spec jira_doc linear_doc
  repo_root="$(cd "$script_dir/../../.." && pwd)"
  jira_spec="$repo_root/src/agentsfleetd/http/handlers/connectors/jira/spec.zig"
  linear_spec="$repo_root/src/agentsfleetd/http/handlers/connectors/linear/spec.zig"
  jira_doc="$repo_root/playbooks/operations/jira_app_registration/001_playbook.md"
  linear_doc="$repo_root/playbooks/operations/linear_app_registration/001_playbook.md"

  # Backticks below are literal Markdown delimiters.
  # shellcheck disable=SC2016
  if ! grep -Fq 'const SCOPES = "read:jira-work read:jira-user write:jira-work read:servicedesk-request write:servicedesk-request offline_access";' "$jira_spec" \
      || ! grep -Fq 'exactly `read:jira-work read:jira-user write:jira-work read:servicedesk-request write:servicedesk-request`' "$jira_doc"; then
    bad "$name" "Jira source and registration playbook do not pin the exact selected scopes"
  elif ! grep -Fq 'const SCOPES = "read,comments:create";' "$linear_spec" \
      || ! grep -Fq 'The authorization request supplies `read,comments:create`.' "$linear_doc"; then
    bad "$name" "Linear source and registration playbook do not pin the exact selected scopes"
  elif ! grep -Fq 'registration alone does not claim that outbound delivery is implemented' "$jira_doc" \
      || ! grep -Fq 'Registration does not claim that the outbound Linear poster is implemented.' "$linear_doc"; then
    bad "$name" "registration docs claim outbound Jira or Linear posting is implemented"
  else
    ok "$name"
  fi
}

# A worker item is only a real worker once it has joined the tailnet: CI reaches
# workers by tailscale-hostname, never by the provider hostname.
test_should_report_a_worker_on_the_tailnet_as_onboarded() {
  local name="test_should_report_a_worker_on_the_tailnet_as_onboarded"
  local output status=0
  output="$(run_gate '0190f5a2-4b2d-7c11-8d5e-2a5f31d98210')" || status=$?
  if [[ "$status" -ne 0 ]]; then
    bad "$name" "gate failed unexpectedly: $output"
    return
  fi
  if [[ "$output" != *"onboarded: zombie-dev-worker-ant"* ]]; then
    bad "$name" "a worker with a tailscale-hostname was not reported as onboarded: $output"
    return
  fi
  ok "$name"
}

# A machine that was never bought is not a credential fault. The gate must name
# the placeholder without failing, or a full credential set reads as a ready
# worker — which is exactly how zombie-prod-worker-bird passed for months.
test_should_report_a_worker_without_tailnet_identity_as_a_non_fatal_placeholder() {
  local name="test_should_report_a_worker_without_tailnet_identity_as_a_non_fatal_placeholder"
  local output status=0
  output="$(run_gate '0190f5a2-4b2d-7c11-8d5e-2a5f31d98210' \
    'op://ZMB_CD_DEV/zombie-dev-worker-ant/tailscale-hostname')" || status=$?
  if [[ "$output" != *"PLACEHOLDER: zombie-dev-worker-ant"* ]]; then
    bad "$name" "a worker with no tailscale-hostname was not flagged as a placeholder: $output"
    return
  fi
  if [[ "$status" -ne 0 ]]; then
    bad "$name" "the placeholder was fatal; a missing machine must not fail the credential gate: $output"
    return
  fi
  ok "$name"
}

# The duplicate-hostname branch: vault items get made by copying an existing
# one, which carries the provider hostname over verbatim and leaves two entries
# pointing at one box. Exercised directly because it only triggers on the prod
# path, and prod mode reaches out to the Vercel API.
test_should_name_the_sibling_when_a_placeholder_shares_its_host() {
  local name="test_should_name_the_sibling_when_a_placeholder_shares_its_host"
  local output
  output="$(
    # Trusted input: the function is lifted verbatim out of the script under test.
    eval "$(sed -n '/^check_worker_onboarded()/,/^}/p' "$script_under_test")"
    op_read_with_retry() {
      case "$1" in
        */zombie-prod-worker-bird/tailscale-hostname) return 1 ;;
        */hostname) printf 'one-and-the-same-box\n' ;;
        *) printf 'stub\n' ;;
      esac
    }
    check_worker_onboarded ZMB_CD_PROD zombie-prod-worker-bird zombie-prod-worker-ant 2>&1
  )"
  if [[ "$output" != *"hostname is zombie-prod-worker-ant's host"* ]]; then
    bad "$name" "shared hostname was not attributed to the sibling worker: $output"
    return
  fi
  ok "$name"
}

test_should_accept_uuidv7_workspace_pointer
test_should_reject_missing_workspace_pointer
test_should_reject_non_uuidv7_workspace_pointer
test_should_check_runtime_connector_credentials
test_should_reject_missing_approval_signer
test_should_reject_missing_canonical_provider_fields
test_should_scope_deployment_credential_gates
test_should_pin_issue_tracker_registration_contracts
test_should_report_a_worker_on_the_tailnet_as_onboarded
test_should_report_a_worker_without_tailnet_identity_as_a_non_fatal_placeholder
test_should_name_the_sibling_when_a_placeholder_shares_its_host

printf '\n%d passed, %d failed\n' "$passed" "$failed"
[[ "$failed" -eq 0 ]]
