#!/usr/bin/env bash
# Shared definitions for the local development commands. Compatible with Bash 3.
set -euo pipefail

DEV_INFRA_PORT_START=10000
DEV_INFRA_SLOTS=500
DEV_INFRA_STRIDE=16
DEV_DAEMON_PORT_START=30000
DEV_DAEMON_PORT_COUNT=9000
DEV_INFRA_LOCK_ATTEMPTS=1800
DEV_BUILD_LOCK_ATTEMPTS=6000
DEV_PROCESS_LOCK_ATTEMPTS=9000
DEV_LOCK_INTERVAL=0.2
DEV_READY_SECONDS=90
DEV_STOP_SECONDS=30

dev_fail() {
  # logging: command diagnostics are the developer interface.
  printf 'local-dev: %s\n' "$*" >&2
  exit 1
}

dev_hash() {
  printf '%s' "$1" | cksum | awk '{printf "%08x", $1}'
}

dev_identity() {
  DEV_ROOT="$(git rev-parse --show-toplevel)"
  DEV_ROOT="$(cd "$DEV_ROOT" && pwd -P)"
  DEV_COMMON="$(git rev-parse --path-format=absolute --git-common-dir)"
  DEV_COMMON="$(cd "$DEV_COMMON" && pwd -P)"
  DEV_REPO_ID="$(dev_hash "$DEV_COMMON")"
  DEV_ID="wt_$(dev_hash "$DEV_ROOT")"
  DEV_SHARED="$DEV_COMMON/agentsfleet-dev-$(id -u)-$DEV_REPO_ID"
  DEV_STATE="$DEV_SHARED/worktrees/$DEV_ID"
  DEV_INFRA="$DEV_SHARED/infrastructure"
  DEV_PROJECT="agentsfleet-dev-$DEV_REPO_ID"
  DEV_PORT=$((DEV_DAEMON_PORT_START + 16#${DEV_ID#wt_} % DEV_DAEMON_PORT_COUNT))
  DEV_URL="http://127.0.0.1:$DEV_PORT"
  DEV_DB="dev_$DEV_ID"
  DEV_PID_FILE="$DEV_STATE/daemon.pid"
  DEV_BINARY="$DEV_STATE/build/debug/agentsfleetd"
  DEV_LOG="$DEV_STATE/daemon.log"
  local base=$((DEV_INFRA_PORT_START + 16#$DEV_REPO_ID % DEV_INFRA_SLOTS * DEV_INFRA_STRIDE))
  export AGENTSFLEET_PG_HOST_PORT="$base" AGENTSFLEET_DRAGONFLY_BASE_PORT="$((base + 1))" AGENTSFLEET_DRAGONFLY_LAST_PORT="$((base + 5))"
  export AGENTSFLEET_QSTASH_HOST_PORT="$((base + 10))" AGENTSFLEET_MAILPIT_SMTP_PORT="$((base + 11))" AGENTSFLEET_MAILPIT_UI_PORT="$((base + 12))"
}

dev_state() {
  umask 077
  [ ! -L "$DEV_SHARED" ] && [ ! -L "$DEV_STATE" ] && [ ! -L "$DEV_SHARED/worktrees" ] || dev_fail 'development state directory is a symlink'
  mkdir -p "$DEV_STATE"
  [ -O "$DEV_SHARED" ] || dev_fail 'development state directory belongs to another user'
}

dev_marker() {
  local file="$1" value="$2"
  if [ -f "$file" ]; then
    [ "$(cat "$file")" = "$value" ] || dev_fail 'development identifier collision'
  else
    printf '%s\n' "$value" >"$file"
  fi
}

dev_lock() {
  DEV_LOCK="$1"
  local attempt=0 attempts=$DEV_PROCESS_LOCK_ATTEMPTS
  case "$DEV_LOCK" in
    "$DEV_SHARED/infrastructure.lock") attempts=$DEV_INFRA_LOCK_ATTEMPTS ;;
    "$DEV_SHARED/build.lock") attempts=$DEV_BUILD_LOCK_ATTEMPTS ;;
  esac
  until mkdir "$DEV_LOCK" 2>/dev/null; do
    attempt=$((attempt + 1))
    [ "$attempt" -lt "$attempts" ] || dev_fail "lock wait expired: $DEV_LOCK"
    sleep "$DEV_LOCK_INTERVAL"
  done
  trap 'rmdir "$DEV_LOCK"' EXIT
  if [ "$DEV_LOCK" = "$DEV_SHARED/infrastructure.lock" ]; then
    dev_marker "$DEV_SHARED/repository" "$DEV_COMMON"
  else
    dev_marker "$DEV_STATE/worktree" "$DEV_ROOT"
  fi
}

dev_compose() {
  docker compose --project-name "$DEV_PROJECT" --project-directory "$DEV_INFRA" \
    --env-file "$DEV_INFRA/ports.env" --file "$DEV_INFRA/compose.yml" "$@"
}


dev_snapshot() {
  [ ! -L "$DEV_INFRA" ] || dev_fail 'infrastructure snapshot is a symlink'
  if [ -d "$DEV_INFRA" ]; then
    [ -s "$DEV_INFRA/compose.yml" ] && [ -s "$DEV_INFRA/ports.env" ] || dev_fail 'incomplete infrastructure snapshot'
    return
  fi
  local staging="$DEV_SHARED/infrastructure-new-$$"
  mkdir -p "$staging/scripts"
  cp "$DEV_ROOT/compose.dev-infra.yml" "$staging/compose.yml"
  cp "$DEV_ROOT/scripts/dragonfly-cluster.sh" "$staging/scripts/dragonfly-cluster.sh"
  cat >"$staging/ports.env" <<EOF
AGENTSFLEET_PG_HOST_PORT=$AGENTSFLEET_PG_HOST_PORT
AGENTSFLEET_DRAGONFLY_BASE_PORT=$AGENTSFLEET_DRAGONFLY_BASE_PORT
AGENTSFLEET_DRAGONFLY_LAST_PORT=$AGENTSFLEET_DRAGONFLY_LAST_PORT
AGENTSFLEET_QSTASH_HOST_PORT=$AGENTSFLEET_QSTASH_HOST_PORT
AGENTSFLEET_MAILPIT_SMTP_PORT=$AGENTSFLEET_MAILPIT_SMTP_PORT
AGENTSFLEET_MAILPIT_UI_PORT=$AGENTSFLEET_MAILPIT_UI_PORT
EOF
  mv "$staging" "$DEV_INFRA"
}

dev_infra_up() {
  dev_snapshot
  local status count=0 service state health
  status="$(dev_compose ps --all --format '{{.Service}} {{.State}} {{.Health}}')"
  if [ -n "$status" ]; then
    while read -r service state health; do
      case "$service" in postgres|dragonfly|mailpit|qstash) ;; *) dev_fail 'unexpected shared service' ;; esac
      [ "$state" = running ] && [ "$health" = healthy ] || dev_fail "shared $service is unhealthy; inspect it before restarting infrastructure"
      count=$((count + 1))
    done <<<"$status"
    [ "$count" = 4 ] || dev_fail 'shared stack is incomplete; inspect it before restarting infrastructure'
    printf 'Reusing shared infrastructure %s\n' "$DEV_PROJECT"
    return
  fi
  dev_compose up -d --wait --wait-timeout 330 --no-recreate
}

dev_database() {
  dev_infra_up
  # DEV_DB comes only from the fixed prefix and worktree checksum.
  printf "SELECT 'CREATE DATABASE \"%s\"' WHERE NOT EXISTS (SELECT FROM pg_database WHERE datname = '%s')\\gexec\n" \
    "$DEV_DB" "$DEV_DB" | dev_compose exec -T postgres psql -U agentsfleet -d postgres -v ON_ERROR_STOP=1
}


dev_infrastructure() (
  dev_state
  dev_lock "$DEV_SHARED/infrastructure.lock"
  case "$1" in
    up) dev_infra_up ;;
    database) dev_database ;;
    down)
      [ -d "$DEV_INFRA" ] || exit 0
      dev_compose down
      ;;
  esac
)


dev_process_stamp() {
  if [ "$(uname -s)" = Linux ]; then
    # Kernel start ticks survive wall-clock, timezone and locale changes.
    python3 - "$1" <<'PY'
from pathlib import Path
import sys
try:
    print(Path(f"/proc/{sys.argv[1]}/stat").read_text().rsplit(") ", 1)[1].split()[19])
except (OSError, IndexError):
    sys.exit(1)
PY
  else
    LC_ALL=C TZ=UTC ps -p "$1" -o lstart=
  fi
}

dev_owned_process() {
  [ -s "$DEV_PID_FILE" ] || return 1
  local pid started command state
  pid="$(sed -n '1p' "$DEV_PID_FILE")"
  case "$pid" in ''|*[!0-9]*) dev_fail 'invalid daemon process record' ;; esac
  kill -0 "$pid" 2>/dev/null || return 1
  state="$(ps -p "$pid" -o stat=)" || return 1
  case "$state" in *Z*) return 1 ;; esac
  started="$(dev_process_stamp "$pid")" || return 1
  command="$(ps -ww -p "$pid" -o command=)" || return 1
  [ "$started" = "$(sed -n '2p' "$DEV_PID_FILE")" ] && \
    [ "$command" = "$DEV_BINARY serve --port $DEV_PORT" ] && return 0
  # A normal exit can change the command to a zombie between the snapshots.
  state="$(ps -p "$pid" -o stat=)" || return 1
  case "$state" in *Z*) return 1 ;; esac
  dev_fail 'daemon process identity changed; refusing to signal it'
}

dev_stop() {
  if dev_owned_process; then
    local pid deadline
    pid="$(sed -n '1p' "$DEV_PID_FILE")"
    if ! kill -TERM "$pid" 2>/dev/null && dev_owned_process; then
      dev_fail 'could not signal the owned daemon'
    fi
    deadline=$(( $(date +%s) + DEV_STOP_SECONDS ))
    while dev_owned_process; do
      [ "$(date +%s)" -lt "$deadline" ] || dev_fail 'daemon did not stop within its shutdown allowance'
      sleep 0.2
    done
  fi
  rm -f "$DEV_PID_FILE"
}

dev_load_environment() {
  local env_file="${AGENTSFLEET_RUSTD_ENV_FILE:-$DEV_ROOT/.env.agentsfleetd.local}"
  if [ -f "$env_file" ]; then
    # This is the developer's trusted shell environment, never generated input.
    set -a
    source "$env_file"
    set +a
  fi
  # Pin addresses back to the shared snapshot after loading machine settings.
  set -a
  source "$DEV_INFRA/ports.env"
  set +a
  [ -n "${AUTH_SESSION_CODE_PEPPER:-}" ] || dev_fail 'AUTH_SESSION_CODE_PEPPER is missing; load the local developer environment'
  [ -n "${ENCRYPTION_MASTER_KEY:-}" ] || dev_fail 'ENCRYPTION_MASTER_KEY is missing; load the local developer environment'
  export DATABASE_URL_API="postgresql://agentsfleet:agentsfleet@127.0.0.1:$AGENTSFLEET_PG_HOST_PORT/$DEV_DB?sslmode=disable"
  export DATABASE_URL_MIGRATOR="$DATABASE_URL_API"
  export DRAGONFLY_URL="redis://:agentsfleet@127.0.0.1:$AGENTSFLEET_DRAGONFLY_BASE_PORT"
  export AGENTSFLEET_DEV_NAMESPACE="$DEV_ID"
  export API_URL
  API_URL="$(dev_api_origin)"
  export QSTASH_URL="http://127.0.0.1:$AGENTSFLEET_QSTASH_HOST_PORT/v2"
  export QSTASH_TOKEN
  QSTASH_TOKEN="$(printf '{"UserID":"%s","Password":"%s"}' defaultUser defaultPassword | base64 | tr -d '\n')"
  local keys
  keys="$(curl --silent --show-error --fail --max-time 5 -H "Authorization: Bearer $QSTASH_TOKEN" "$QSTASH_URL/keys")"
  export QSTASH_CURRENT_SIGNING_KEY QSTASH_NEXT_SIGNING_KEY
  QSTASH_CURRENT_SIGNING_KEY="$(printf '%s' "$keys" | python3 -c 'import json,sys; print(json.load(sys.stdin)["current"])')"
  QSTASH_NEXT_SIGNING_KEY="$(printf '%s' "$keys" | python3 -c 'import json,sys; print(json.load(sys.stdin)["next"])')"
}

dev_api_origin() {
  local host interface
  case "$(uname -s)" in
    Darwin)
      interface="$(route -n get default | awk '/interface:/{print $2}')"
      host="$(ipconfig getifaddr "$interface")"
      ;;
    Linux) host="$(ip -o -4 route get 1.1.1.1 | awk '{for(i=1;i<=NF;i++) if($i=="src") print $(i+1)}')" ;;
    *) dev_fail 'native development requires macOS or Linux' ;;
  esac
  [ -n "$host" ] || dev_fail 'no host IPv4 address is available for API_URL'
  printf 'http://%s:%s\n' "$host" "$DEV_PORT"
}

dev_port_available() {
  python3 - "$DEV_PORT" <<'PY'
import socket, sys
with socket.socket() as probe:
    try:
        probe.bind(("0.0.0.0", int(sys.argv[1])))
    except OSError:
        sys.exit("local-dev: daemon port is occupied; stop its owner or use another worktree path")
PY
}

dev_launch() {
  nohup "$DEV_BINARY" serve --port "$DEV_PORT" >"$DEV_LOG" 2>&1 </dev/null &
  local pid=$! started deadline
  started="$(dev_process_stamp "$pid")"
  printf '%s\n%s\n' "$pid" "$started" >"$DEV_PID_FILE"
  deadline=$(( $(date +%s) + DEV_READY_SECONDS ))
  until curl --silent --fail --max-time 2 "$DEV_URL/readyz" >/dev/null && \
    [ "$(lsof -nP -a -p "$pid" -iTCP:"$DEV_PORT" -sTCP:LISTEN -t 2>/dev/null)" = "$pid" ]; do
    kill -0 "$pid" 2>/dev/null || dev_fail "daemon refused boot; read $DEV_LOG"
    if [ "$(date +%s)" -ge "$deadline" ]; then
      dev_stop
      dev_fail "daemon readiness timed out; read $DEV_LOG"
    fi
    sleep 0.2
  done
  dev_owned_process || dev_fail "daemon refused boot; read $DEV_LOG"
  printf 'API: %s\nLogs: %s\nNamespace: %s\n' "$DEV_URL" "$DEV_LOG" "$DEV_ID"
}

dev_build() (
  dev_lock "$DEV_SHARED/build.lock"
  cd "$DEV_ROOT/rustd"
  CARGO_TARGET_DIR="$DEV_SHARED/build" cargo build -p agentsfleetd --bin agentsfleetd
  mkdir -p "$(dirname "$DEV_BINARY")"
  cp "$DEV_SHARED/build/debug/agentsfleetd" "$DEV_BINARY.new"
  mv "$DEV_BINARY.new" "$DEV_BINARY"
)

dev_up() {
  if ! dev_owned_process; then dev_port_available; fi
  dev_infrastructure database
  dev_load_environment
  dev_build
  "$DEV_BINARY" migrate
  dev_stop
  dev_port_available
  dev_launch
}

dev_runner_environment() {
  local url="${AGENTSFLEET_API_URL:-$DEV_URL}"
  if [ "$(uname -s)" != Linux ] && [ -z "${AGENTSFLEET_API_URL:-}" ]; then
    dev_fail "set AGENTSFLEET_API_URL to this machine's runner-reachable URL on port $DEV_PORT"
  fi
  printf 'AGENTSFLEET_API_URL=%q\nRUNNER_STORAGE_HOME=%q\n' "$url" \
    "${RUNNER_STORAGE_HOME:-/var/lib/agentsfleet-runner}/worktrees/$DEV_REPO_ID/$DEV_ID"
}

dev_identity
[ "${BASH_SOURCE[0]}" = "$0" ] || return 0

case "${1:-up}" in
  url) printf '%s\n' "$DEV_URL" ;;
  identity) printf '%s\n' "$DEV_ID" ;;
  runner-env) dev_runner_environment ;;
  infra-up) dev_infrastructure up ;;
  infra-down) dev_infrastructure down ;;
  up|down)
    dev_state
    dev_lock "$DEV_STATE/process.lock"
    if [ "${1:-up}" = up ]; then dev_up; else dev_stop; fi
    ;;
  logs)
    [ -f "$DEV_LOG" ] || dev_fail 'no daemon log exists; run make up first'
    tail -f "$DEV_LOG"
    ;;
  *) dev_fail 'expected up, down, infra-up, infra-down, url, identity, runner-env or logs' ;;
esac
