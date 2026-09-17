#!/usr/bin/env bash
# dragonfly_cluster_ready.sh — is a Dragonfly app bootstrapped, or merely listening?
#
#     dragonfly_cluster_ready.sh <app>
#
# Fly's own health check is TCP, and a node that has never been handed its
# DFLYCLUSTER CONFIG passes it happily. `healthy` is dragonfly-cluster.sh's own
# verb -- every node answering PING, every node online in CLUSTER SHARDS, and
# the TLS handshake -- which is the shape agentsfleetd's preflight demands
# before it will boot.
#
# Failing HERE costs one red deploy step. Failing later costs a crash-looping
# daemon and a red acceptance lane that reads as a product fault.
#
# Lived in the datastore cutover's probe runner until that runbook retired.
# This was never cutover scaffolding: both deploy pipelines call it on every
# deploy, so it outlived the one-off it shipped with and now sits with the
# other scripts the workflows run.
#
# `flyctl` resolves through $FLYCTL so a caller can point at a different
# binary. No caller sets it today.
set -euo pipefail

FLYCTL="${FLYCTL:-flyctl}"
CLUSTER_SCRIPT="/usr/local/bin/dragonfly-cluster.sh"
readonly FLYCTL CLUSTER_SCRIPT

cluster_ready() {
  local app="$1"
  if "$FLYCTL" ssh console --app "$app" --command "bash $CLUSTER_SCRIPT healthy" >/dev/null 2>&1; then
    printf '✓ cluster-ready: %s is bootstrapped and every node is online\n' "$app"
  else
    printf '✗ cluster-ready: %s did not answer healthy — a TCP-listening node is not a bootstrapped cluster\n' \
      "$app" >&2
    return 1
  fi
}

main() {
  if [ "$#" -ne 1 ]; then
    printf 'usage: %s <app>\n' "${0##*/}" >&2
    return 2
  fi
  cluster_ready "$1"
}

# Sourced by the self-test; executed as a check otherwise.
if [[ "${BASH_SOURCE[0]}" == "$0" ]]; then
  main "$@"
fi
