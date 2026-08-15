#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=../../lib/test_search.sh
source "$script_dir/../../lib/test_search.sh"
repo_root="$(cd "$script_dir/../../.." && pwd)"
script_under_test="$script_dir/02_credentials.sh"

passed=0
failed=0

ok() {
  passed=$((passed + 1))
  echo "  ✓ $1"
}

bad() {
  failed=$((failed + 1))
  echo "  ✗ $1: $2" >&2
}

work_dir="$(mktemp -d)"
stub_dir="$work_dir/bin"
mkdir -p "$stub_dir"

cleanup() {
  rm -rf -- "$work_dir"
}
trap cleanup EXIT

cat >"$stub_dir/op" <<'STUB'
#!/usr/bin/env bash
ref="${2:-}"
if [ -n "${MISSING_REF:-}" ] && [ "$ref" = "$MISSING_REF" ]; then
  exit 1
fi
case "$ref" in
  */issuer|*/grafana-url|*/qstash/url)
    printf 'https://provider.example.test\n'
    ;;
  */migrator-connection-string)
    printf 'postgres-migrator\n'
    ;;
  */api-connection-string)
    printf 'postgres-api\n'
    ;;
  *)
    printf '%s\n' "${SECRET_SENTINEL:-stub-value}"
    ;;
esac
STUB
chmod +x "$stub_dir/op"

run_gate() {
  local stage="$1"
  local missing_ref="${2:-}"
  local target_env="${3:-dev}"

  env \
    PATH="$stub_dir:$PATH" \
    ENV="$target_env" \
    STAGE="$stage" \
    OP_READ_RETRIES=1 \
    OP_READ_MIN_INTERVAL_SECONDS=0 \
    MISSING_REF="$missing_ref" \
    SECRET_SENTINEL=do-not-print-provider-secret \
    bash "$script_under_test" 2>&1
}

test_bootstrap_checks_only_pre_priming_inputs() {
  local name="bootstrap checks only pre-priming inputs"
  local output status=0
  output="$(run_gate bootstrap \
    'op://ZMB_CD_DEV/planetscale-dev/api-connection-string')" || status=$?

  if [ "$status" -ne 0 ]; then
    bad "$name" "post-priming database output blocked bootstrap: $output"
  elif [[ "$output" == *"planetscale-dev/api-connection-string"* ]]; then
    bad "$name" "bootstrap read a post-priming database output"
  elif [[ "$output" != *"agentsfleet-admin/username"* ]] || \
       [[ "$output" != *"agentsfleet-admin/credential"* ]]; then
    bad "$name" "admin login inputs were not checked"
  else
    ok "$name"
  fi
}

test_post_deploy_values_are_not_early_inputs() {
  local name="post-deploy values are not early inputs"
  local -a refs=(
    'op://ZMB_CD_DEV/agentsfleet-admin/api-key'
    'op://ZMB_CD_DEV/agentsfleet-admin/platform_admin_workspace_id'
    'op://ZMB_CD_DEV/agentsfleet-dev-runner-ant/tailscale-hostname'
    'op://ZMB_CD_DEV/agentsfleet-dev-runner-ant/runner-token'
  )
  local ref output status

  for ref in "${refs[@]}"; do
    local early_stage
    for early_stage in bootstrap deployment; do
      status=0
      output="$(run_gate "$early_stage" "$ref")" || status=$?
      if [ "$status" -ne 0 ] || [[ "$output" == *"$ref"* ]]; then
        bad "$name" "$ref was read during $early_stage"
        return
      fi
    done
  done
  ok "$name"
}

test_deployment_checks_complete_infrastructure_inputs() {
  local name="deployment checks complete infrastructure inputs"
  local output status=0
  output="$(run_gate deployment)" || status=$?
  local -a refs=(
    'approval-signing-secret/credential'
    'planetscale-dev/api-connection-string'
    'planetscale-dev/migrator-connection-string'
    'upstash-dev/api-url'
    'upstash-dev/url'
    'grafana-dev/otlp-endpoint'
    'grafana-dev/instance-id'
    'grafana-dev/api-key'
    'cloudflare-tunnel-dev/credential'
    'cloudflare-r2/account-id'
    'cloudflare-r2/access-key-id'
    'cloudflare-r2/secret-access-key'
    'cloudflare-r2/bucket'
  )
  local ref

  if [ "$status" -ne 0 ]; then
    bad "$name" "complete development inventory failed: $output"
    return
  fi
  for ref in "${refs[@]}"; do
    if [[ "$output" != *"$ref"* ]]; then
      bad "$name" "missing inventory check: $ref"
      return
    fi
  done
  if [[ "$output" == *"github-app/"* ]] ||
     [[ "$output" == *"qstash/token"* ]] ||
     [[ "$output" == *"agentsfleet-admin/api-key"* ]] ||
     [[ "$output" == *"grafana-observability/"* ]]; then
    bad "$name" "post-deploy operations input blocked initial deployment"
  elif [[ "$output" == *do-not-print-provider-secret* ]]; then
    bad "$name" "gate printed a provider secret"
  else
    ok "$name"
  fi
}

test_deployment_rejects_missing_runtime_input() {
  local name="deployment rejects a missing runtime input"
  local ref='op://ZMB_CD_DEV/approval-signing-secret/credential'
  local output status=0
  output="$(run_gate deployment "$ref")" || status=$?

  if [ "$status" -ne 1 ]; then
    bad "$name" "missing input returned status $status"
  elif [[ "$output" != *"MISSING: $ref"* ]]; then
    bad "$name" "failure did not name $ref"
  elif [[ "$output" == *do-not-print-provider-secret* ]]; then
    bad "$name" "failure printed a provider secret"
  else
    ok "$name"
  fi
}

test_prod_checks_both_discord_webhooks() {
  local name="production checks development and release Discord webhooks"
  local output status=0
  output="$(run_gate bootstrap '' prod)" || status=$?

  if [ "$status" -ne 0 ]; then
    bad "$name" "complete production inventory failed: $output"
  elif [[ "$output" != *"discord-ci-webhook/credential"* ]]; then
    bad "$name" "development webhook was not checked"
  elif [[ "$output" != *"discord-release-webhook/credential"* ]]; then
    bad "$name" "release webhook was not checked"
  elif [[ "$output" == *do-not-print-provider-secret* ]]; then
    bad "$name" "gate printed a webhook secret"
  else
    ok "$name"
  fi
}

test_discord_notifications_route_by_release_stage() {
  local name="Discord notifications route development and production separately"
  local action="$repo_root/.github/actions/notify-discord/action.yml"
  local workflow

  if ! rg --fixed-strings --quiet 'default: discord-ci-webhook' "$action" ||
     ! rg --fixed-strings --quiet 'op://${{ inputs.vault }}/${{ inputs.webhook-item }}/credential' "$action"; then
    bad "$name" "notify action does not default to the development webhook item"
    return
  fi

  for workflow in release.yml post-release.yml; do
    if ! rg --fixed-strings --quiet \
      'webhook-item: discord-release-webhook' \
      "$repo_root/.github/workflows/$workflow"; then
      bad "$name" "$workflow does not select the release webhook"
      return
    fi
  done

  for workflow in deploy-dev.yml deploy-dev-fly.yml deploy-dev-worker.yml; do
    if rg --fixed-strings --quiet \
      'webhook-item: discord-release-webhook' \
      "$repo_root/.github/workflows/$workflow"; then
      bad "$name" "$workflow routes development output to the release webhook"
      return
    fi
  done
  ok "$name"
}

test_workflows_use_deployment_stage_without_generated_pointer() {
  local name="workflows use deployment stage without generated pointer"
  local workflow

  for workflow in deploy-dev.yml release.yml; do
    if ! rg --fixed-strings --quiet \
      'STAGE=deployment ./playbooks/founding/02_preflight/00_gate.sh' \
      "$repo_root/.github/workflows/$workflow"; then
      bad "$name" "$workflow does not use the deployment credential stage"
      return
    fi
  done

  for workflow in deploy-dev.yml deploy-dev-fly.yml release.yml; do
    if rg --quiet 'PLATFORM_ADMIN_WORKSPACE_ID' \
      "$repo_root/.github/workflows/$workflow"; then
      bad "$name" "$workflow still requires the post-signup workspace pointer"
      return
    fi
  done
  ok "$name"
}

test_workflows_load_only_current_connector_boot_secret() {
  local name="workflows load only the current connector boot secret"
  local family workflow vault

  for workflow in deploy-dev release; do
    vault=VAULT_DEV
    [ "$workflow" = release ] && vault=VAULT_PROD
    family="$(cat "$repo_root/.github/workflows/$workflow"*.yml)"
    # The expression is literal GitHub Actions syntax.
    # shellcheck disable=SC2016
    if ! rg --fixed-strings --quiet \
      "APPROVAL_SIGNING_SECRET: op://\${{ vars.$vault }}/approval-signing-secret/credential" \
      <<<"$family"; then
      bad "$name" "$workflow workflow family does not load approval-signing-secret"
      return
    fi
    if ! rg --fixed-strings --quiet \
      'APPROVAL_SIGNING_SECRET="$APPROVAL_SIGNING_SECRET"' \
      <<<"$family"; then
      bad "$name" "$workflow workflow family does not pass approval-signing-secret to Fly"
      return
    fi
    if rg --quiet 'GITHUB_APP_ID|GITHUB_APP_PRIVATE_KEY' <<<"$family"; then
      bad "$name" "$workflow workflow family still loads retired GitHub app boot secrets"
      return
    fi
  done
  ok "$name"
}

test_issue_tracker_docs_pin_current_source_scopes() {
  local name="issue-tracker docs pin current source scopes"
  local jira_spec="$repo_root/src/agentsfleetd/http/handlers/connectors/jira/spec.zig"
  local linear_spec="$repo_root/src/agentsfleetd/http/handlers/connectors/linear/spec.zig"
  local jira_doc="$repo_root/playbooks/operations/jira_app_registration/001_playbook.md"
  local linear_doc="$repo_root/playbooks/operations/linear_app_registration/001_playbook.md"
  local jira_doc_text

  jira_doc_text="$(tr '\n' ' ' <"$jira_doc")"

  # Backticks below are literal Markdown delimiters.
  # shellcheck disable=SC2016
  if ! rg --fixed-strings --quiet \
    'const SCOPES = "read:jira-work read:jira-user write:jira-work read:servicedesk-request write:servicedesk-request offline_access";' \
    "$jira_spec"; then
    bad "$name" "Jira source scope changed"
  elif [[ "$jira_doc_text" != *'exactly `read:jira-work read:jira-user write:jira-work read:servicedesk-request write:servicedesk-request`'* ]]; then
    bad "$name" "Jira registration scope is stale"
  elif ! rg --fixed-strings --quiet \
    'const SCOPES = "read,comments:create";' \
    "$linear_spec"; then
    bad "$name" "Linear source scope changed"
  elif ! rg --fixed-strings --quiet \
    'The authorization request supplies `read,comments:create`.' \
    "$linear_doc"; then
    bad "$name" "Linear registration scope is stale"
  else
    ok "$name"
  fi
}

test_unknown_stage_fails_closed() {
  local name="unknown stage fails closed"
  local output status=0
  output="$(run_gate nope)" || status=$?
  if [ "$status" -eq 2 ] && [[ "$output" == *"Unknown STAGE"* ]]; then
    ok "$name"
  else
    bad "$name" "unknown stage returned status $status: $output"
  fi
}

echo "credential-stage regression tests"
test_bootstrap_checks_only_pre_priming_inputs
test_post_deploy_values_are_not_early_inputs
test_deployment_checks_complete_infrastructure_inputs
test_deployment_rejects_missing_runtime_input
test_prod_checks_both_discord_webhooks
test_discord_notifications_route_by_release_stage
test_workflows_use_deployment_stage_without_generated_pointer
test_workflows_load_only_current_connector_boot_secret
test_issue_tracker_docs_pin_current_source_scopes
test_unknown_stage_fails_closed

echo ""
echo "results: $passed passed, $failed failed"
test "$failed" -eq 0
