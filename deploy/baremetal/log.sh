#!/usr/bin/env bash
# log.sh — the deploy's log line and its fatal exit.
#
# Sourced by deploy.sh, and by any suite that sources service.sh on its own:
# service.sh logs through `log` but defines it nowhere. Defines functions only.
# Output goes to stdout, which the runner playbook streams back over Tailscale
# SSH as the deploy runs.

log() { echo "[deploy] $*"; }
die() { log "FATAL: $*"; exit 1; }
