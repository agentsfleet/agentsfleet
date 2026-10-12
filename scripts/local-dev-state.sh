#!/usr/bin/env bash
# Persistent stack snapshots and ownership, sourced by local-dev.sh (Bash 3).

dev_state_home() {
  DEV_STATE_BASE="${XDG_STATE_HOME:-$HOME/.local/state}"
  case "$DEV_STATE_BASE" in /*) ;; *) dev_fail 'state home must be an absolute path' ;; esac
  DEV_STATE_HOME="$DEV_STATE_BASE/agentsfleet"
  dev_directory "$DEV_STATE_BASE"
  dev_directory "$DEV_STATE_HOME"
}

dev_directory() {
  [ ! -L "$1" ] || dev_fail "development state directory is a symlink: $1"
  if [ -e "$1" ]; then
    [ -d "$1" ] && [ -O "$1" ] || dev_fail "development state directory is not owned by this user: $1"
  fi
}

dev_identity() {
  DEV_ROOT="$(git rev-parse --show-toplevel)"
  DEV_ROOT="$(cd "$DEV_ROOT" && pwd -P)"
  DEV_COMMON="$(git rev-parse --path-format=absolute --git-common-dir)"
  DEV_COMMON="$(cd "$DEV_COMMON" && pwd -P)"
  DEV_REPO_ID="$(dev_hash "$DEV_COMMON")"
  DEV_ID="wt_$(dev_hash "$DEV_ROOT")"
  dev_state_home
  DEV_SHARED="$DEV_STATE_HOME/dev-$(id -u)-$DEV_REPO_ID"
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

dev_stack_paths() {
  dev_directory "$DEV_SHARED"
  dev_directory "$DEV_SHARED/worktrees"
  dev_directory "$DEV_INFRA"
}

dev_state() {
  umask 077
  dev_stack_paths
  dev_directory "$DEV_STATE"
  mkdir -p "$DEV_STATE"
}

dev_marker() {
  local file="$1" value="$2"
  [ ! -L "$file" ] || dev_fail "development marker is a symlink: $file"
  if [ -e "$file" ]; then
    [ -f "$file" ] && [ -O "$file" ] || dev_fail "invalid development marker: $file"
    [ "$(cat "$file")" = "$value" ] || dev_fail 'development identifier collision'
  else
    printf '%s\n' "$value" >"$file"
  fi
}

dev_lock() {
  DEV_LOCK="$1"
  [ ! -L "$DEV_LOCK" ] || dev_fail "development lock is a symlink: $DEV_LOCK"
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
}

dev_snapshot_check() {
  dev_directory "$DEV_INFRA"
  local file
  for file in "$DEV_INFRA/compose.yml" "$DEV_INFRA/ports.env"; do
    [ ! -L "$file" ] || dev_fail "infrastructure snapshot file is a symlink: $file"
    [ -f "$file" ] && [ -s "$file" ] && [ -O "$file" ] || dev_fail 'incomplete infrastructure snapshot'
  done
}

dev_compose() {
  dev_snapshot_check
  docker compose --project-name "$DEV_PROJECT" --project-directory "$DEV_INFRA" \
    --env-file "$DEV_INFRA/ports.env" --file "$DEV_INFRA/compose.yml" "$@"
}

dev_snapshot() {
  dev_directory "$DEV_INFRA"
  if [ -d "$DEV_INFRA" ]; then
    dev_snapshot_check
    return
  fi
  local staging="$DEV_SHARED/infrastructure-new-$$"
  dev_directory "$staging"
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
  dev_marker "$DEV_SHARED/repository" "$DEV_COMMON"
  dev_marker "$DEV_STATE/worktree" "$DEV_ROOT"
  case "$1" in
    up) dev_infra_up ;;
    database) dev_database ;;
  esac
)

dev_stack_select() {
  DEV_PROJECT="$1"
  local suffix="${DEV_PROJECT#agentsfleet-dev-}"
  case "$DEV_PROJECT" in agentsfleet-dev-????????) ;; *) dev_fail 'invalid development compose project' ;; esac
  case "$suffix" in *[!0-9a-f]*) dev_fail 'invalid development compose project' ;; esac
  DEV_SHARED="$DEV_STATE_HOME/dev-$(id -u)-$suffix"
  DEV_INFRA="$DEV_SHARED/infrastructure"
  dev_stack_paths
  [ -d "$DEV_SHARED" ] || dev_fail "unknown development stack: $DEV_PROJECT"
}

dev_owners() {
  DEV_LIVE_ROOTS=''
  local owner marker root state
  for owner in "$DEV_SHARED"/worktrees/*; do
    [ -e "$owner" ] || [ -L "$owner" ] || continue
    dev_directory "$owner"
    marker="$owner/worktree"
    [ -e "$marker" ] || [ -L "$marker" ] || continue
    [ ! -L "$marker" ] || dev_fail "development marker is a symlink: $marker"
    [ -f "$marker" ] && [ -O "$marker" ] || dev_fail "invalid development marker: $marker"
    root="$(cat "$marker")"
    case "$root" in /*) ;; *) dev_fail "invalid registered worktree root: $marker" ;; esac
    state=gone
    if [ -e "$root" ]; then
      state=live
      DEV_LIVE_ROOTS="${DEV_LIVE_ROOTS}${root}"$'\n'
    fi
    printf '  worktree %s: %s (%s)\n' "${owner##*/}" "$root" "$state"
  done
}

dev_stack_down() (
  dev_stack_select "$1"
  dev_lock "$DEV_SHARED/infrastructure.lock"
  dev_owners
  [ -z "$DEV_LIVE_ROOTS" ] || dev_fail "refusing teardown; live worktrees remain:"$'\n'"$DEV_LIVE_ROOTS"
  dev_compose down --volumes
)

dev_stack_status() (
  dev_stack_select "$1"
  dev_lock "$DEV_SHARED/infrastructure.lock"
  printf 'Stack: %s\nState: %s\n' "$DEV_PROJECT" "$DEV_SHARED"
  dev_owners
  if [ ! -d "$DEV_INFRA" ]; then
    printf '  containers: unprovisioned\n'
    exit 0
  fi
  dev_snapshot_check
  local status service state health running=0
  if ! status="$(dev_compose ps --all --format '{{.Service}} {{.State}} {{.Health}}')"; then
    printf '  containers: unavailable\n'
    exit 0
  fi
  if [ -n "$status" ]; then
    while read -r service state health; do
      printf '  container %s: %s %s\n' "$service" "$state" "$health"
      if [ "$state" = running ]; then running=$((running + 1)); fi
    done <<<"$status"
  fi
  if [ "$running" -gt 0 ]; then printf '  containers: running\n'; else printf '  containers: stopped\n'; fi
)

dev_status() {
  dev_state_home
  local stack suffix found=0
  for stack in "$DEV_STATE_HOME"/dev-"$(id -u)"-*; do
    [ -e "$stack" ] || [ -L "$stack" ] || continue
    found=1
    suffix="${stack##*/}"
    dev_stack_status "agentsfleet-dev-${suffix##*-}"
  done
  if [ "$found" = 0 ]; then printf 'No shared development stacks under %s\n' "$DEV_STATE_HOME"; fi
}
