#!/usr/bin/env bash
# service.sh — stop, restart and health-check the runner's systemd unit.
#
# Sourced by deploy.sh, which runs as root on the host and sits beside this
# file in /opt/agentsfleet/deploy (the runner playbook copies both). Defines
# functions only; sourcing it changes nothing on the host. SERVICE_NAME,
# SYSTEMD_DIR, HOST, log and die come from deploy.sh.

# How long a restarted runner must stay up, with systemd restarting it no time
# in between, before the deploy calls it healthy. A runner that exits during
# boot (a refused token, a toolbox it cannot admit) still reads active between
# exits: Restart=always brings it back RestartSec after each one, and that
# cycle never reaches `failed`. So the window outlasts the unit's RestartSec
# plus a first boot, which copies and hashes the toolbox image before the
# runner dials the control plane. service_test.sh holds it above RestartSec.
readonly HEALTH_STABLE_SECONDS=30

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
  systemctl enable "$SERVICE_NAME"
  systemctl restart "$SERVICE_NAME"
}

# How many times systemd has restarted the unit on its own since it was last
# started by hand. Empty when systemd cannot say.
restart_count() {
  systemctl show -p NRestarts --value "$SERVICE_NAME" 2>/dev/null || true
}

# Succeeds when the unit is still active after `window` seconds and systemd
# restarted it no time in between.
stays_up() {
  local window="$1" before after
  before="$(restart_count)"
  sleep "$window"
  after="$(restart_count)"
  if [[ -z "$before" || "$before" != "$after" ]]; then
    log "✗ ${SERVICE_NAME} restarted within ${window}s (NRestarts ${before:-unknown} → ${after:-unknown})."
    return 1
  fi
  if ! systemctl is-active --quiet "$SERVICE_NAME"; then
    log "✗ ${SERVICE_NAME} was not active ${window}s after it came up."
    return 1
  fi
  log "✓ ${SERVICE_NAME} stayed up for ${window}s."
}

verify_healthy() {
  # attempts, delay and window overridable only so the tests avoid real waits.
  local attempts="${VERIFY_HEALTH_ATTEMPTS:-5}"
  local delay="${VERIFY_HEALTH_DELAY:-2}"
  local window="${VERIFY_HEALTH_WINDOW:-$HEALTH_STABLE_SECONDS}"
  local i
  for i in $(seq 1 "$attempts"); do
    sleep "$delay"
    if systemctl is-active --quiet "$SERVICE_NAME"; then
      log "✓ ${SERVICE_NAME} is active (attempt ${i}/${attempts}); holding it ${window}s."
      stays_up "$window" && return 0
      break
    fi
    # Fail fast: if systemd already marked it failed, don't keep waiting.
    if systemctl is-failed --quiet "$SERVICE_NAME" 2>/dev/null; then
      log "✗ ${SERVICE_NAME} entered failed state."
      break
    fi
  done
  log "✗ ${SERVICE_NAME} did not come up and stay up. Dumping diagnostics:"
  systemctl status "$SERVICE_NAME" --no-pager || true
  journalctl -u "$SERVICE_NAME" --no-pager -n 30 || true
  return 1
}
