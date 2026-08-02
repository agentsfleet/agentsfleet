#!/usr/bin/env bash

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"
passed=0
failed=0

ok() {
  printf 'ok   %s\n' "$1"
  passed=$((passed + 1))
}

bad() {
  printf 'FAIL %s\n       %s\n' "$1" "$2" >&2
  failed=$((failed + 1))
}

require_literal() {
  local name="$1"
  local file="$2"
  local literal="$3"
  if grep -Fq "$literal" "$file"; then
    ok "$name"
  else
    bad "$name" "$file does not contain: $literal"
  fi
}

reject_public_service() {
  local name="$1"
  local file="$2"
  if grep -Eq '^[[:space:]]*\[\[?services?\]?\]|^[[:space:]]*\[http_service\]' \
    "$file"; then
    bad "$name" "$file publishes a Fly service"
  else
    ok "$name"
  fi
}

test_environment() {
  local environment="$1"
  local hostname="$2"
  local api_dir="$REPO_ROOT/deploy/fly/agentsfleetd-$environment"
  local tunnel_dir="$REPO_ROOT/deploy/fly/cloudflared-$environment"
  local api_app="agentsfleetd-$environment"
  local tunnel_app="cloudflared-$environment"

  require_literal "${environment}_api_app" "$api_dir/fly.toml" \
    "app = \"$api_app\""
  require_literal "${environment}_tunnel_app" "$tunnel_dir/fly.toml" \
    "app = \"$tunnel_app\""
  require_literal "${environment}_hostname" "$tunnel_dir/config.yml" \
    "hostname: $hostname"
  require_literal "${environment}_private_origin" "$tunnel_dir/config.yml" \
    "service: http://$api_app.internal:3000"
  require_literal "${environment}_fallback" "$tunnel_dir/config.yml" \
    'service: http_status:404'
  require_literal "${environment}_api_restart" "$api_dir/fly.toml" \
    'policy = "always"'
  require_literal "${environment}_tunnel_restart" "$tunnel_dir/fly.toml" \
    'policy = "always"'
  require_literal "${environment}_readiness" "$api_dir/fly.toml" \
    'path = "/readyz"'
  require_literal "${environment}_readiness_port" "$api_dir/fly.toml" \
    'port = 3000'
  reject_public_service "${environment}_api_is_private" "$api_dir/fly.toml"
  reject_public_service "${environment}_tunnel_is_outbound" \
    "$tunnel_dir/fly.toml"
}

test_environment dev api-dev.agentsfleet.net
test_environment prod api.agentsfleet.net

release_workflow="$REPO_ROOT/.github/workflows/release.yml"
require_literal prod_api_desired_count "$release_workflow" \
  'DESIRED_API_MACHINES=3'
require_literal prod_api_scale_command "$release_workflow" \
  'flyctl scale count "$DESIRED_API_MACHINES"'
require_literal prod_api_total_count_check "$release_workflow" \
  'test "$TOTAL" -eq "$DESIRED_API_MACHINES"'
require_literal prod_api_running_count_check "$release_workflow" \
  'test "$RUNNING" -eq "$DESIRED_API_MACHINES"'

printf '\n%d passed, %d failed\n' "$passed" "$failed"
[ "$failed" -eq 0 ]
