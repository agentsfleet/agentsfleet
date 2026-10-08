#!/usr/bin/env bash
# service.sh — stop, restart and health-check the runner's systemd unit.
#
# Sourced by deploy.sh, which runs as root on the host and sits beside this
# file in /opt/agentsfleet/deploy (the runner playbook copies both). Defines
# functions only; sourcing it changes nothing on the host. Each function takes
# the unit it acts on, and every bound it waits for, as arguments: deploy.sh
# passes the production values, and service_test.sh passes ones that do not
# wait. `log` comes from log.sh. restart_services' sweep of pre-rename units
# also reads SYSTEMD_DIR and HOST from deploy.sh.

# Bounded graceful stop. Lease reclaim (lease_expires_at + fencing_token) is
# the safety net for a forced stop, so the timeout only gives an in-flight
# child a chance to finish before SIGKILL.
drain_runner() {
  local unit="$1" timeout="$2"

  if ! systemctl is-active --quiet "$unit"; then
    log "Runner not running — skipping drain."
    return 0
  fi

  log "Stopping runner (timeout=${timeout}s) ..."
  if ! timeout "$timeout" systemctl stop "$unit"; then
    log "⚠ Stop timeout (${timeout}s) — killing runner forcefully."
    systemctl kill --signal=SIGKILL "$unit" 2>/dev/null || true
  fi
}

restart_services() {
  local unit="$1" drain_timeout="$2"
  drain_runner "$unit" "$drain_timeout"
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
  # visible in the deploy log; non-fatal because the cutover is
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
  systemctl enable "$unit"
  systemctl restart "$unit"
}

# How many times systemd has restarted the unit on its own since it was last
# started by hand. Empty when systemd cannot say.
restart_count() {
  local unit="$1"
  systemctl show -p NRestarts --value "$unit" 2>/dev/null || true
}

# Succeeds when the unit is still active after `window` seconds and systemd
# restarted it no time in between.
stays_up() {
  local unit="$1" window="$2" before after
  before="$(restart_count "$unit")"
  sleep "$window"
  after="$(restart_count "$unit")"
  if [[ -z "$before" || "$before" != "$after" ]]; then
    log "✗ ${unit} restarted within ${window}s (NRestarts ${before:-unknown} → ${after:-unknown})."
    return 1
  fi
  if ! systemctl is-active --quiet "$unit"; then
    log "✗ ${unit} was not active ${window}s after it came up."
    return 1
  fi
  log "✓ ${unit} stayed up for ${window}s."
}

# Waits up to `attempts` probes, `delay` seconds apart, for the unit to come
# up, then holds it `window` seconds (stays_up).
verify_healthy() {
  local unit="$1" attempts="$2" delay="$3" window="$4"
  local i
  for i in $(seq 1 "$attempts"); do
    sleep "$delay"
    if systemctl is-active --quiet "$unit"; then
      log "✓ ${unit} is active (attempt ${i}/${attempts}); holding it ${window}s."
      stays_up "$unit" "$window" && return 0
      break
    fi
    # Fail fast: if systemd already marked it failed, don't keep waiting.
    if systemctl is-failed --quiet "$unit" 2>/dev/null; then
      log "✗ ${unit} entered failed state."
      break
    fi
  done
  log "✗ ${unit} did not come up and stay up. Dumping diagnostics:"
  systemctl status "$unit" --no-pager || true
  journalctl -u "$unit" --no-pager -n 30 || true
  return 1
}
