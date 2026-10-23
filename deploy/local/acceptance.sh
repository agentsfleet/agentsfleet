#!/usr/bin/env bash
# Local deployment only; build ordering and suite recipes belong to Make.
set -euo pipefail
cd "$(dirname "$0")/../.."
source scripts/local-dev.sh
dev_state

action="${1:-all}"
case "$action" in all|e2e|cli|execution) ;; *) dev_fail 'expected e2e, cli or execution (default: both suites)' ;; esac
[ "$(uname -s)" = Darwin ] || dev_fail 'local acceptance requires macOS with a dedicated OrbStack Linux machine'
export LOCAL_DEV_ENV_FILE="${AGENTSFLEET_LOCAL_ACCEPTANCE_ENV_FILE:-$DEV_STATE/acceptance.env}"
[ -f "$LOCAL_DEV_ENV_FILE" ] || dev_fail "configure $LOCAL_DEV_ENV_FILE first; see docs/development.md"
set -a
if [ -f "$HOME/.config/agentsfleet/.env" ]; then source "$HOME/.config/agentsfleet/.env"; fi
if [ -n "${AGENTSFLEET_UI_ENV_FILE:-}" ]; then source "$AGENTSFLEET_UI_ENV_FILE"; fi
source "$LOCAL_DEV_ENV_FILE"
set +a
# The backend resolves live model credentials from its vault.
unset OPENROUTER_API_KEY
case "${NEXT_PUBLIC_CLERK_PUBLISHABLE_KEY:-}" in pk_test_*) ;; *) dev_fail 'local acceptance requires development Clerk credentials' ;; esac
: "${CLERK_SECRET_KEY:?development Clerk secret is required}"
: "${ACCEPTANCE_LOCAL_RUNNER_MACHINE:?dedicated OrbStack machine is required}"
: "${ACCEPTANCE_LOCAL_TOOLBOX_DIR:?signed Linux toolbox directory is required}"
: "${ACCEPTANCE_LOCAL_RUNNER_RECORD:?local runner registration file is required}"

dev_lock "$DEV_STATE/acceptance.lock"
ui_pid=""
cleanup() {
  if [ -n "$ui_pid" ]; then
    if kill -0 "$ui_pid" 2>/dev/null; then kill -TERM "$ui_pid"; fi
    wait "$ui_pid" || true
  fi
  rmdir "$DEV_LOCK"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

export LOCAL_ACCEPTANCE_STATE="$DEV_STATE/acceptance"
export LOCAL_ACCEPTANCE_UI_PORT="${ACCEPTANCE_LOCAL_UI_PORT:-$((DEV_PORT + 9000))}"
export LOCAL_ACCEPTANCE_TLS_PORT="${ACCEPTANCE_LOCAL_TLS_PORT:-$((DEV_PORT + 10000))}"
export LOCAL_ACCEPTANCE_CA="$LOCAL_ACCEPTANCE_STATE/caddy-data/caddy/pki/authorities/local/root.crt"
export NEXT_PUBLIC_API_URL="https://localhost:$LOCAL_ACCEPTANCE_TLS_PORT"
export ACCEPTANCE_RUNNER_MACHINE="$ACCEPTANCE_LOCAL_RUNNER_MACHINE" CARGO_TARGET_DIR="$DEV_SHARED/build"
mkdir -p "$LOCAL_ACCEPTANCE_STATE"
export ACCEPTANCE_RUNNER_TOKEN
ACCEPTANCE_RUNNER_TOKEN="$(bun -e '
const {runner_token: token} = await Bun.file(Bun.argv[1]).json();
if (typeof token !== "string" || !/^[!-~]{1,4096}$/.test(token))
  throw new Error("local acceptance: invalid runner token in private registration file");
process.stdout.write(token);
' "$ACCEPTANCE_LOCAL_RUNNER_RECORD")"
if [ ! -f "$LOCAL_ACCEPTANCE_STATE/audit-token" ]; then openssl rand -hex 32 >"$LOCAL_ACCEPTANCE_STATE/audit-token"; fi
export AGENTSFLEET_E2E_AUDIT=1 AGENTSFLEET_E2E_AUDIT_TOKEN
AGENTSFLEET_E2E_AUDIT_TOKEN="$(cat "$LOCAL_ACCEPTANCE_STATE/audit-token")"

start_runner() {
  orb -m "$ACCEPTANCE_LOCAL_RUNNER_MACHINE" sudo bash -s -- "$DEV_ROOT" \
    "$ACCEPTANCE_LOCAL_TOOLBOX_DIR" "$LOCAL_ACCEPTANCE_STATE/runner.env" "$LOCAL_ACCEPTANCE_CA" "$LOCAL_ACCEPTANCE_TLS_PORT" <<'LINUX'
set -euo pipefail
root="$1" toolbox="$2" env_file="$3" ca="$4" port="$5"
source "$root/deploy/baremetal/deploy.sh"
check_deploy_inputs "$root/dist/acceptance/agentsfleet-runner" "$toolbox" "$env_file"
install -d -m 0755 "$HOST_DEPLOY_DIR" "$RUNNER_STORAGE_HOME_DEFAULT"
for entry in "${HOST_DEPLOY_FILES[@]}"; do
  install -m "${entry##*:}" "$root/deploy/baremetal/${entry%:*}" "$HOST_DEPLOY_DIR/${entry%:*}"
done
install -m 0600 "$env_file" "$HOST_ENV_FILE"
cat /etc/ssl/certs/ca-certificates.crt "$ca" >"$RUNNER_STORAGE_HOME_DEFAULT/ca.pem"
chmod 0644 "$RUNNER_STORAGE_HOME_DEFAULT/ca.pem"
printf '[Socket]\nListenStream=127.0.0.1:%s\n' "$port" >/run/systemd/system/agentsfleet-runner-tls.socket
printf '[Service]\nExecStart=/usr/lib/systemd/systemd-socket-proxyd host.orb.internal:%s\nNoNewPrivileges=yes\nProtectSystem=strict\nProtectHome=yes\n' "$port" \
  >/run/systemd/system/agentsfleet-runner-tls.service
systemctl daemon-reload
systemctl stop agentsfleet-runner-tls.socket agentsfleet-runner-tls.service
systemctl start agentsfleet-runner-tls.socket
bash "$HOST_DEPLOY_DIR/deploy.sh" runner "$(cat "$root/VERSION")" "$root/dist/acceptance/agentsfleet-runner" "$toolbox"
LINUX
}

start_dashboard() {
  local deadline next
  next="$DEV_ROOT/ui/packages/app/node_modules/next/dist/bin/next"
  bun "$next" start "$DEV_ROOT/ui/packages/app" --hostname localhost --port "$LOCAL_ACCEPTANCE_UI_PORT" >"$LOCAL_ACCEPTANCE_STATE/app.log" 2>&1 &
  ui_pid=$!
  deadline=$(( $(date +%s) + DEV_READY_SECONDS ))
  until [ "$(lsof -nP -a -p "$ui_pid" -iTCP:"$LOCAL_ACCEPTANCE_UI_PORT" -sTCP:LISTEN -t 2>/dev/null)" = "$ui_pid" ] && \
    curl --silent --fail --max-time 2 "http://localhost:$LOCAL_ACCEPTANCE_UI_PORT/sign-in" >/dev/null; do
    kill -0 "$ui_pid" 2>/dev/null || dev_fail "dashboard refused boot; read $LOCAL_ACCEPTANCE_STATE/app.log"
    [ "$(date +%s)" -lt "$deadline" ] || dev_fail "dashboard readiness timed out; read $LOCAL_ACCEPTANCE_STATE/app.log"
    sleep 0.2
  done
}

make --no-print-directory acceptance-build
export LOCAL_DEV_BINARY="$DEV_ROOT/dist/acceptance/agentsfleetd"
make --no-print-directory _ensure-local-daemon
mkdir -p "$LOCAL_ACCEPTANCE_STATE/caddy-data" "$LOCAL_ACCEPTANCE_STATE/caddy-config"
printf 'https://localhost {\n tls internal\n reverse_proxy host.docker.internal:%s\n}\n' "$DEV_PORT" >"$LOCAL_ACCEPTANCE_STATE/Caddyfile"
docker compose --project-name "$DEV_PROJECT-$DEV_ID-acceptance" --file deploy/local/compose.yml up --detach --wait
deadline=$(( $(date +%s) + DEV_READY_SECONDS ))
until curl --silent --fail --max-time 2 --cacert "$LOCAL_ACCEPTANCE_CA" "https://localhost:$LOCAL_ACCEPTANCE_TLS_PORT/readyz" >/dev/null; do
  [ "$(date +%s)" -lt "$deadline" ] || dev_fail 'Caddy HTTPS readiness timed out; inspect docker compose logs'
  sleep 0.2
done
export NODE_EXTRA_CA_CERTS="$LOCAL_ACCEPTANCE_CA"
start_dashboard
printf 'AGENTSFLEET_API_URL=https://localhost:%s\nAGENTSFLEET_RUNNER_TOKEN=%s\nSSL_CERT_FILE=/var/lib/agentsfleet-runner/ca.pem\n' \
  "$LOCAL_ACCEPTANCE_TLS_PORT" "$ACCEPTANCE_RUNNER_TOKEN" >"$LOCAL_ACCEPTANCE_STATE/runner.env"
start_runner

export BASE_URL="http://localhost:$LOCAL_ACCEPTANCE_UI_PORT"
export NEXT_PUBLIC_API_URL="https://localhost:$LOCAL_ACCEPTANCE_TLS_PORT"
export AGENTSFLEET_ACCEPTANCE_TARGET="$NEXT_PUBLIC_API_URL" AGENTSFLEET_ACCEPTANCE_DASHBOARD_URL="$BASE_URL"
export AGENTSFLEET_ACCEPTANCE_LOGIN_HANDSHAKE=1 CLERK_PUBLISHABLE_KEY="$NEXT_PUBLIC_CLERK_PUBLISHABLE_KEY"
export AUTH_E2E_REGULAR_EMAIL="${AUTH_E2E_REGULAR_EMAIL:-regular-fixture@e2e.agentsfleet.net}"
export AUTH_E2E_ADMIN_EMAIL="${AUTH_E2E_ADMIN_EMAIL:-admin-fixture@e2e.agentsfleet.net}"
targets=("acceptance-$action")
if [ "$action" = all ]; then targets=(acceptance-e2e acceptance-cli); fi
for target in "${targets[@]}"; do
  make --no-print-directory "$target"
done
