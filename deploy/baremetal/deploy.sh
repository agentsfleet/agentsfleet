#!/usr/bin/env bash
# deploy.sh — install the zombie-runner binary and restart its systemd service.
#
# Two modes:
#   Local:    deploy.sh runner <version> <binary-path>
#             Installs from a local file (CI scp'd the binary to the server).
#
#   Release:  deploy.sh runner <version>
#             Downloads from GitHub Releases (tagged release deploys).
#
# Environment:
#   DISCORD_WEBHOOK_URL — if set, sends deploy status to Discord
#   DEPLOY_HOSTNAME     — override hostname in notifications (default: $(hostname))
#   DRAIN_TIMEOUT       — seconds to wait for a graceful stop (default: 120)
#
# Examples:
#   deploy.sh runner v0.1.0 /opt/zombie/bin/zombie-runner
#   deploy.sh runner v0.2.0                              # downloads from GH release
#
# The runner holds zero datastore credentials: if it stops abruptly the control
# plane reclaims its in-flight lease (lease_expires_at + fencing_token), so a
# bounded stop never loses or double-runs work.

set -euo pipefail

# Force line-buffered stdout/stderr so log output streams through SSH in real time.
if [ -z "${_DEPLOY_UNBUFFERED:-}" ] && command -v stdbuf >/dev/null 2>&1; then
  export _DEPLOY_UNBUFFERED=1
  exec stdbuf -oL -eL "$0" "$@"
fi

# Load Discord webhook from the env file when not already in the environment.
# Reading the file here keeps the value out of sudo's argument list and therefore
# out of ps/cmdline output.
readonly _DISCORD_ENV_FILE="/opt/zombie/.discord-env"
if [[ -z "${DISCORD_WEBHOOK_URL:-}" && -r "${_DISCORD_ENV_FILE}" ]]; then
  # shellcheck source=/dev/null
  source "${_DISCORD_ENV_FILE}"
fi

readonly REPO="usezombie/usezombie"
readonly INSTALL_DIR="/usr/local/bin"
readonly SYSTEMD_DIR="/etc/systemd/system"
readonly DEPLOY_DIR="/opt/zombie/deploy"
readonly ENV_FILE="/opt/zombie/.env"
readonly ENV_DEST="/etc/default/zombie-runner"
readonly HOST="${DEPLOY_HOSTNAME:-$(hostname)}"

# The single deployable component. Kept as an explicit argument so the call site
# names what it deploys; the resolver rejects any other value (catches stale
# callers still passing the retired 'worker'/'executor' components).
readonly COMPONENT_RUNNER="runner"
readonly BINARY_NAME="zombie-runner"
readonly SERVICE_NAME="zombie-runner.service"
# Release-download artifact is arch-specific. CI's local-binary mode skips this —
# it scp's the right-arch binary and passes its path.
case "$(uname -m)" in
  x86_64 | amd64) _arch="amd64" ;;
  aarch64 | arm64) _arch="arm64" ;;
  *) _arch="$(uname -m)" ;;
esac
readonly RELEASE_ARTIFACT="${BINARY_NAME}-linux-${_arch}"

# ── Logging ──────────────────────────────────────────────────────────────────

log()  { echo "[deploy] $*"; }
die()  { log "FATAL: $*"; notify_discord "fail"; exit 1; }

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
  [[ -f "$ENV_FILE" ]] \
    || die "missing $ENV_FILE — provision via playbooks/006_runner_bootstrap_dev/04_provision_runner_env.sh (dev) or the equivalent prod path"
  cp "$ENV_FILE" "$ENV_DEST"
  log "Synced .env → ${ENV_DEST}"

  # Fail loud when any required runner env var is absent. The daemon's own
  # startup check (getRequired in src/runner/daemon/config.zig) would catch
  # this too, but a 1/FAILURE systemd loop with `MissingEnvVar` is a confusing
  # surface for an operator — die here with the specific missing keys instead.
  local required=(ZOMBIE_API_URL ZOMBIE_RUNNER_TOKEN RUNNER_HOST_ID)
  local missing=()
  local k
  for k in "${required[@]}"; do
    grep -qE "^${k}=" "$ENV_DEST" || missing+=("$k")
  done
  if [[ ${#missing[@]} -gt 0 ]]; then
    die "missing required runner env vars in $ENV_DEST: ${missing[*]}"
  fi

  # Reject the documented placeholder shape (`zrn_FAKE_…`). The daemon's prefix
  # check only enforces `zrn_*`, which a placeholder satisfies — that would
  # loop on 401s. Better to fail at deploy time with a clear cause.
  if grep -qE '^ZOMBIE_RUNNER_TOKEN=zrn_FAKE' "$ENV_DEST"; then
    die "ZOMBIE_RUNNER_TOKEN in $ENV_DEST is the placeholder; mint a real zrn_ via POST /v1/runners and update 1Password before re-running"
  fi
}

# ── Service restart ──────────────────────────────────────────────────────────

drain_runner() {
  # Bounded graceful stop. Lease reclaim (lease_expires_at + fencing_token) is
  # the safety net for a forced stop, so the timeout only gives an in-flight
  # child a chance to finish before SIGKILL.
  local timeout="${DRAIN_TIMEOUT:-120}"

  if ! systemctl is-active --quiet "$SERVICE_NAME"; then
    log "Runner not running — skipping drain."
    return 0
  fi

  log "Stopping runner (timeout=${timeout}s) ..."
  if ! timeout "$timeout" systemctl stop "$SERVICE_NAME"; then
    log "⚠ Stop timeout (${timeout}s) — killing runner forcefully."
    systemctl kill --signal=SIGKILL "$SERVICE_NAME" 2>/dev/null || true
  fi
}

restart_services() {
  drain_runner
  log "Restarting runner ..."
  systemctl restart "$SERVICE_NAME"
}

verify_healthy() {
  local attempts=5
  local delay=2
  for i in $(seq 1 "$attempts"); do
    sleep "$delay"
    if systemctl is-active --quiet "$SERVICE_NAME"; then
      log "✓ ${SERVICE_NAME} is active (attempt ${i}/${attempts})."
      return 0
    fi
    # Fail fast: if systemd already marked it failed, don't keep waiting.
    if systemctl is-failed --quiet "$SERVICE_NAME" 2>/dev/null; then
      log "✗ ${SERVICE_NAME} entered failed state."
      break
    fi
  done
  log "✗ ${SERVICE_NAME} failed to start. Dumping diagnostics:"
  systemctl status "$SERVICE_NAME" --no-pager || true
  journalctl -u "$SERVICE_NAME" --no-pager -n 30 || true
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
    echo "Usage: deploy.sh runner <version> [binary-path]"
    echo "  version:     GitHub release tag (e.g. v0.1.0) or dev SHA (e.g. dev-abc1234)"
    echo "  binary-path: local path to pre-staged binary (optional; downloads from GH release if omitted)"
    exit 1
  fi

  COMPONENT="$1"
  VERSION="$2"
  LOCAL_BINARY="${3:-}"

  [[ "$COMPONENT" == "$COMPONENT_RUNNER" ]] \
    || die "Unknown component '$COMPONENT'. The only deployable component is '${COMPONENT_RUNNER}'."

  # Skip version check when CI provides a local binary — always do a full
  # install+restart cycle. The shortcut is only for release-download mode.
  if [[ -z "$LOCAL_BINARY" ]] && is_already_installed; then
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
