#!/usr/bin/env bash
# deploy.sh — install the agentsfleet-runner binary and restart its systemd service.
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
# At most one deploy runs per host: main() takes a non-blocking flock and exits
# non-zero when another deploy already holds it. Sourcing this file runs no deploy
# — deploy_test.sh relies on that to exercise the functions directly.
#
# The runner holds zero datastore credentials, so an abrupt stop is safe: the
# control plane reclaims its in-flight lease (see drain_runner).

set -euo pipefail

# Sourcing this file must never deploy: deploy_test.sh sources it to reach the
# individual functions, so both the stdbuf re-exec below and `main` at the bottom
# stay behind this guard.
if [[ "${BASH_SOURCE[0]}" == "${0}" ]]; then
  readonly DEPLOY_EXECUTED=1
else
  readonly DEPLOY_EXECUTED=0
fi

# Force line-buffered stdout/stderr so log output streams through SSH in real time.
if [[ "$DEPLOY_EXECUTED" == 1 && -z "${_DEPLOY_UNBUFFERED:-}" ]] && command -v stdbuf >/dev/null 2>&1; then
  export _DEPLOY_UNBUFFERED=1
  exec stdbuf -oL -eL "$0" "$@"
fi

# Load Discord webhook from the env file when not already in the environment.
# Reading the file here keeps the value out of sudo's argument list and therefore
# out of ps/cmdline output.
readonly _DISCORD_ENV_FILE="/opt/agentsfleet/.discord-env"
if [[ -z "${DISCORD_WEBHOOK_URL:-}" && -r "${_DISCORD_ENV_FILE}" ]]; then
  # shellcheck source=/dev/null
  source "${_DISCORD_ENV_FILE}"
fi

readonly REPO="agentsfleet/agentsfleet"
readonly INSTALL_DIR="/usr/local/bin"
readonly SYSTEMD_DIR="/etc/systemd/system"
readonly DEPLOY_DIR="/opt/agentsfleet/deploy"
readonly ENV_FILE="/opt/agentsfleet/.env"
readonly ENV_DEST="/etc/default/agentsfleet-runner"
readonly HOST="${DEPLOY_HOSTNAME:-$(hostname)}"

# The single deployable component. Kept as an explicit argument so the call site
# names what it deploys; the resolver rejects any other value (catches stale
# callers still passing a retired component name).
readonly COMPONENT_RUNNER="runner"
readonly BINARY_NAME="agentsfleet-runner"
readonly SERVICE_NAME="agentsfleet-runner.service"
# Release-download artifact is arch-specific. CI's local-binary mode skips this —
# it scp's the right-arch binary and passes its path.
case "$(uname -m)" in
  x86_64 | amd64) _arch="amd64" ;;
  aarch64 | arm64) _arch="arm64" ;;
  *) _arch="$(uname -m)" ;;
esac
readonly RELEASE_ARTIFACT="${BINARY_NAME}-linux-${_arch}"

# Serializes install + `systemctl restart`, which is not atomic: a manual run and
# a cancel-orphaned CI run can otherwise interleave on the same host. flock beats a
# lock file — the kernel drops it when the holder dies, so a SIGKILLed deploy never
# strands later ones. Overridable only so deploy_test.sh can use a writable temp
# path (/var/lock is root-owned; the tests are not root). Production never sets it.
readonly DEPLOY_LOCK_PATH="${DEPLOY_LOCK_PATH:-/var/lock/agentsfleet-deploy.lock}"

# `agentsfleet-runner --version` prints `agentsfleet-runner <version> (git <sha>)`
# (src/runner/cmd/version.zig). The version is whitespace-delimited field 2.
readonly VERSION_FIELD_INDEX=2

# ── Logging ──────────────────────────────────────────────────────────────────

log()  { echo "[deploy] $*"; }
die()  { log "FATAL: $*"; notify_discord "fail"; exit 1; }

# Exits without notifying Discord: a run refused because another deploy holds the
# lock is not a failure, and a "deploy FAILED" embed would page for a working deploy.
die_unnotified() { log "FATAL: $*"; exit 1; }

# ── Version check ────────────────────────────────────────────────────────────

# Exact equality, never a substring: `0.1.0-rc1` contains `0.1.0` and `0.10.2`
# contains `0.1`, so a glob match skips a real upgrade, leaves the old binary
# running, and reports success.
version_token_matches() {
  local version_output="$1"
  local target="${2#v}"

  local token
  token=$(printf '%s\n' "$version_output" \
    | awk -v field="$VERSION_FIELD_INDEX" 'NR == 1 && NF >= field { print $field }')

  [[ -n "$token" && "$token" == "$target" ]]
}

# `dest` is injectable so the test can point at a stub binary (production passes
# nothing). An unreadable or unexpected `--version` shape yields no token → reports
# "not installed"; a redundant reinstall is safe, a wrong skip is not.
is_already_installed() {
  local dest="${1:-${INSTALL_DIR}/${BINARY_NAME}}"
  [[ -x "$dest" ]] || return 1

  local current
  current=$("$dest" --version 2>/dev/null || true)
  version_token_matches "$current" "$VERSION" || return 1

  log "✓ ${BINARY_NAME} ${VERSION} already installed — ensuring service is up."
  systemctl is-active --quiet "$SERVICE_NAME" && return 0

  # Not active: start it AND verify it stays up. `systemctl start` exits zero once
  # systemd accepts the job, so a runner that starts then dies would otherwise skip
  # to "ok" over a dead service. Any failure → report not-installed so the caller
  # runs the full reinstall path, which surfaces the real fault.
  if ! systemctl start "$SERVICE_NAME" || ! verify_healthy; then
    log "✗ ${SERVICE_NAME} is installed but will not stay up — forcing a full redeploy."
    return 1
  fi
  return 0
}

# ── Deploy mutex ─────────────────────────────────────────────────────────────

# Holds the lock on a descriptor open for the life of the process, so it releases
# on any exit — normal, fatal, or killed. Non-blocking: an operator wants to hear
# "a deploy is already running", not queue silently behind one.
acquire_deploy_lock() {
  command -v flock >/dev/null 2>&1 \
    || die_unnotified "flock not found — install util-linux; refusing to deploy without a mutex."

  exec {DEPLOY_LOCK_FD}>"$DEPLOY_LOCK_PATH" \
    || die_unnotified "cannot open deploy lock $DEPLOY_LOCK_PATH"

  flock -n "$DEPLOY_LOCK_FD" \
    || die_unnotified "another deploy holds $DEPLOY_LOCK_PATH — refusing to run install+restart concurrently."
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
    || die "missing $ENV_FILE — provision via playbooks/founding/06_runner_bootstrap_dev/04_provision_runner_env.sh (dev) or the equivalent prod path"
  cp "$ENV_FILE" "$ENV_DEST"
  log "Synced .env → ${ENV_DEST}"

  # Fail loud when any required runner env var is absent. The daemon's own
  # startup check (getRequired in src/runner/daemon/config.zig) would catch
  # this too, but a 1/FAILURE systemd loop with `MissingEnvVar` is a confusing
  # surface for an operator — die here with the specific missing keys instead.
  local required=(AGENTSFLEET_API_URL AGENTSFLEET_RUNNER_TOKEN RUNNER_HOST_ID)
  local missing=()
  local k
  for k in "${required[@]}"; do
    grep -qE "^${k}=" "$ENV_DEST" || missing+=("$k")
  done
  if [[ ${#missing[@]} -gt 0 ]]; then
    die "missing required runner env vars in $ENV_DEST: ${missing[*]}"
  fi

  # Reject the documented placeholder shape (`agt_rFAKE_…`). The daemon's prefix
  # check only enforces `agt_r*`, which a placeholder satisfies — that would
  # loop on 401s. Better to fail at deploy time with a clear cause.
  if grep -qE '^AGENTSFLEET_RUNNER_TOKEN=agt_rFAKE' "$ENV_DEST"; then
    die "AGENTSFLEET_RUNNER_TOKEN in $ENV_DEST is the placeholder; mint a real agt_r via POST /v1/runners and update 1Password before re-running"
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
  # One-time transition off any pre-rename unit before the renamed unit takes
  # over. The fleet's rename chain is zombie-runner → agent-runner →
  # agentsfleet-runner; a host still carrying either legacy unit gets it stopped,
  # disabled, AND its unit file removed here so the transition fires exactly once
  # (a left-behind disabled unit would otherwise re-trip this every deploy). Live
  # bare-metal boxes were provisioned as zombie-runner, so that name MUST be
  # covered — the prior shim named only agent-runner and so left
  # zombie-runner.service enabled alongside the new unit. We warn LOUDLY rather
  # than clean up silently, so a box that still carried pre-rename residue is
  # visible in the deploy log + Discord; non-fatal because the cutover is
  # self-healing. Harmless no-op on a freshly-bootstrapped box.
  local legacy_unit found_stale=0
  for legacy_unit in zombie-runner.service agent-runner.service; do
    if systemctl cat "$legacy_unit" >/dev/null 2>&1; then
      found_stale=1
      log "⚠ STALE LEGACY UNIT ${legacy_unit} found on ${HOST} — stopping, disabling, and removing it (pre-rename residue; investigate why this host was not re-bootstrapped if unexpected)."
      systemctl stop "$legacy_unit" 2>/dev/null || true
      systemctl disable "$legacy_unit" 2>/dev/null || true
      rm -f "${SYSTEMD_DIR}/${legacy_unit}" 2>/dev/null || true
    fi
  done
  [[ "$found_stale" -eq 1 ]] && systemctl daemon-reload
  systemctl restart "$SERVICE_NAME"
}

verify_healthy() {
  # attempts/delay overridable only so deploy_test.sh avoids a real 10s wait.
  local attempts="${VERIFY_HEALTH_ATTEMPTS:-5}"
  local delay="${VERIFY_HEALTH_DELAY:-2}"
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

  # After argument validation, before anything that touches the host: a usage or
  # bad-component error needs no lock, and must not fail on an unwritable /var/lock.
  acquire_deploy_lock

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

if [[ "$DEPLOY_EXECUTED" == 1 ]]; then
  main "$@"
fi
