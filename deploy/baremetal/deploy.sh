#!/usr/bin/env bash
# deploy.sh — install a zombied component binary and restart its systemd service.
#
# Two modes:
#   Local:    deploy.sh <component> <version> <binary-path>
#             Installs from a local file (CI scp'd the binary to the server).
#
#   Release:  deploy.sh <component> <version>
#             Downloads from GitHub Releases (tagged release deploys).
#
# Environment:
#   DISCORD_WEBHOOK_URL — if set, sends deploy status to Discord
#   DEPLOY_HOSTNAME     — override hostname in notifications (default: $(hostname))
#
# Examples:
#   deploy.sh executor v0.1.0 /opt/zombie/bin/zombied-executor
#   deploy.sh worker   v0.1.0 /opt/zombie/bin/zombied
#   deploy.sh worker   v0.2.0                                  # downloads from GH release

set -euo pipefail

readonly REPO="usezombie/usezombie"
readonly INSTALL_DIR="/usr/local/bin"
readonly SYSTEMD_DIR="/etc/systemd/system"
readonly DEPLOY_DIR="/opt/zombie/deploy"
readonly ENV_FILE="/opt/zombie/.env"
readonly ENV_DEST="/etc/default/zombied-worker"
readonly HOST="${DEPLOY_HOSTNAME:-$(hostname)}"

# Populated by resolve_component()
BINARY_NAME=""
RELEASE_ARTIFACT=""
SERVICE_NAME=""

# ── Logging ──────────────────────────────────────────────────────────────────

log()  { echo "[deploy] $*"; }
die()  { log "FATAL: $*"; notify_discord "fail"; exit 1; }

# ── Component resolution ─────────────────────────────────────────────────────

resolve_component() {
  case "$1" in
    worker)
      BINARY_NAME="zombied"
      RELEASE_ARTIFACT="zombied-linux-amd64"
      SERVICE_NAME="zombied-worker.service"
      ;;
    executor)
      BINARY_NAME="zombied-executor"
      RELEASE_ARTIFACT="zombied-executor-linux-amd64"
      SERVICE_NAME="zombied-executor.service"
      ;;
    *) die "Unknown component '$1'. Must be 'worker' or 'executor'." ;;
  esac
}

# ── Version check ────────────────────────────────────────────────────────────

is_already_installed() {
  local dest="${INSTALL_DIR}/${BINARY_NAME}"
  [[ -x "$dest" ]] || return 1

  local current
  current=$("$dest" --version 2>/dev/null || echo "unknown")
  [[ "$current" == *"${VERSION#v}"* ]] || return 1

  log "✓ ${BINARY_NAME} ${VERSION} already installed — ensuring service is up."
  systemctl is-active --quiet "$SERVICE_NAME" || systemctl start "$SERVICE_NAME"
  return 0
}

# ── Binary acquisition ───────────────────────────────────────────────────────

acquire_from_local() {
  local src="$1"
  [[ -f "$src" ]] || die "Local binary not found: $src"
  log "Installing from local path: $src"
  install -m 755 "$src" "${INSTALL_DIR}/${BINARY_NAME}"
}

acquire_from_release() {
  local url="https://github.com/${REPO}/releases/download/${VERSION}/${RELEASE_ARTIFACT}.tar.gz"
  local tmpdir
  tmpdir=$(mktemp -d)
  # shellcheck disable=SC2064
  trap "rm -rf '$tmpdir'" EXIT

  log "Downloading ${RELEASE_ARTIFACT} ${VERSION} ..."
  curl -fsSL -o "${tmpdir}/${RELEASE_ARTIFACT}.tar.gz" "$url" \
    || die "Download failed. Check that release ${VERSION} includes ${RELEASE_ARTIFACT}."
  tar xzf "${tmpdir}/${RELEASE_ARTIFACT}.tar.gz" -C "$tmpdir"
  install -m 755 "${tmpdir}/${RELEASE_ARTIFACT}" "${INSTALL_DIR}/${BINARY_NAME}"
}

# ── Systemd sync ─────────────────────────────────────────────────────────────

sync_systemd_unit() {
  local src="${DEPLOY_DIR}/${SERVICE_NAME}"
  [[ -f "$src" ]] || return 0
  cp "$src" "${SYSTEMD_DIR}/${SERVICE_NAME}"
  systemctl daemon-reload
  log "Synced ${SERVICE_NAME} → systemd."
}

sync_env() {
  [[ "$COMPONENT" == "worker" && -f "$ENV_FILE" ]] || return 0
  cp "$ENV_FILE" "$ENV_DEST"
  log "Synced .env → ${ENV_DEST}"
}

# ── Service restart ──────────────────────────────────────────────────────────

drain_worker() {
  # 120s is safely within zombied-worker.service TimeoutStopSec=300.
  local timeout="${DRAIN_TIMEOUT:-120}"

  if ! systemctl is-active --quiet zombied-worker.service; then
    log "Worker not running — skipping drain."
    return 0
  fi

  log "Draining worker (timeout=${timeout}s) ..."
  # Use 'stop' not 'kill': stop tells systemd to deactivate the unit
  # (prevents Restart=always from respawning), while kill only signals the
  # process and systemd immediately restarts it.
  systemctl stop zombied-worker.service &
  local stop_pid=$!

  local elapsed=0
  while [ "$elapsed" -lt "$timeout" ]; do
    if ! systemctl is-active --quiet zombied-worker.service; then
      log "✓ Worker drained and stopped after ${elapsed}s."
      wait "$stop_pid" 2>/dev/null || true
      return 0
    fi

    if journalctl -u zombied-worker.service --since "${timeout} seconds ago" --no-pager -q 2>/dev/null \
        | grep -qE 'worker\.drain_complete|worker\.drain_timeout'; then
      log "✓ Worker drain signal detected after ${elapsed}s."
      wait "$stop_pid" 2>/dev/null || true
      return 0
    fi

    sleep 5
    elapsed=$((elapsed + 5))
  done

  log "⚠ Drain timeout (${timeout}s) — killing worker forcefully."
  systemctl kill --signal=SIGKILL zombied-worker.service 2>/dev/null || true
  wait "$stop_pid" 2>/dev/null || true
}

restart_services() {
  if [[ "$COMPONENT" == "executor" ]]; then
    log "Draining worker before executor restart (Requires= dependency) ..."
    drain_worker
    log "Restarting executor ..."
    systemctl restart zombied-executor.service
    sleep 2
    systemctl restart zombied-worker.service || true
  else
    drain_worker
    log "Restarting worker ..."
    systemctl restart zombied-worker.service
  fi
}

verify_healthy() {
  sleep 3
  if systemctl is-active --quiet "$SERVICE_NAME"; then
    log "✓ ${SERVICE_NAME} is active."
    return 0
  fi
  log "✗ ${SERVICE_NAME} failed to start."
  systemctl status "$SERVICE_NAME" --no-pager || true
  journalctl -u "$SERVICE_NAME" --no-pager -n 20 || true
  return 1
}

# ── Discord notification ─────────────────────────────────────────────────────

notify_discord() {
  local status="$1"  # "ok" or "fail"
  [[ -n "${DISCORD_WEBHOOK_URL:-}" ]] || return 0

  local color msg
  if [[ "$status" == "ok" ]]; then
    color=3066993
    msg="✅ **${HOST}**: deployed \`${BINARY_NAME}\` ${VERSION}\\n${SERVICE_NAME}: active"
  else
    color=15158332
    msg="❌ **${HOST}**: deploy FAILED for \`${BINARY_NAME}\` ${VERSION}\\nCheck: \`journalctl -u ${SERVICE_NAME}\`"
  fi

  curl -sf -X POST "$DISCORD_WEBHOOK_URL" \
    -H "Content-Type: application/json" \
    -d "{\"embeds\":[{\"description\":\"$msg\",\"color\":$color}]}" \
    || log "Warning: Discord notification failed (non-fatal)."
}

# ── Main ─────────────────────────────────────────────────────────────────────

main() {
  if [[ $# -lt 2 || $# -gt 3 ]]; then
    echo "Usage: deploy.sh <component> <version> [binary-path]"
    echo "  component:   worker | executor"
    echo "  version:     GitHub release tag (e.g. v0.1.0) or dev SHA (e.g. dev-abc1234)"
    echo "  binary-path: local path to pre-staged binary (optional; downloads from GH release if omitted)"
    exit 1
  fi

  COMPONENT="$1"
  VERSION="$2"
  LOCAL_BINARY="${3:-}"

  resolve_component "$COMPONENT"

  if is_already_installed; then
    notify_discord "ok"
    return 0
  fi

  if [[ -n "$LOCAL_BINARY" ]]; then
    acquire_from_local "$LOCAL_BINARY"
  else
    acquire_from_release
  fi

  sync_systemd_unit
  sync_env
  restart_services

  if verify_healthy; then
    notify_discord "ok"
    log "Deploy complete: ${BINARY_NAME} ${VERSION}"
  else
    notify_discord "fail"
    exit 1
  fi
}

main "$@"
