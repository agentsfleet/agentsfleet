#!/usr/bin/env bash
# fly_exec.sh - run one cluster_ops.sh verb inside an environment's Dragonfly app.
#
# Sourced by 02_teardown.sh and 03_verify.sh. The deployed cluster is one Fly
# machine running four member processes that announce its private 6PN
# address (deploy/fly/dragonfly-{dev,prod}/Dockerfile). The app has no public
# service, so a laptop cannot reach the members; the verbs run inside the
# machine, over the same `flyctl ssh console` the deploy pipelines use for the
# cluster's `healthy` verb (scripts/dragonfly_cluster_ready.sh). No credential
# leaves the machine: cluster_ops.sh reads the cluster's own password there.
#
# The script travels as base64 inside the command (characters that need no
# quoting) because the image carries no copy of it.
#
# `flyctl` resolves through $FLYCTL, as in dragonfly_cluster_ready.sh, so the
# regression test can route the command to a local cluster.

FLY_EXEC_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
readonly FLY_EXEC_DIR
# The supported topology: scripts/dragonfly-cluster.sh starts NODE_COUNT=4
# members, two primaries and a replica each. A view naming fewer refuses.
readonly DRAGONFLY_EXPECTED_MEMBERS=4

# The Fly app holding ENV's cluster. These are the DRAGONFLY_APP names the deploy
# pipelines bring up (.github/workflows/deploy-dev-fly.yml, release.yml).
dragonfly_app_for_env() {
	case "$1" in
		dev) echo "dragonfly-dev" ;;
		prod) echo "dragonfly-prod" ;;
		*)
			echo "ERROR: no Dragonfly app for ENV '$1'" >&2
			return 1
			;;
	esac
}

# Fails unless flyctl is installed and holds a session or an API token.
dragonfly_require_fly_auth() {
	local flyctl="${FLYCTL:-flyctl}"
	command -v "$flyctl" >/dev/null 2>&1 || {
		echo "ERROR: required tool not found: $flyctl" >&2
		return 1
	}
	"$flyctl" auth whoami >/dev/null 2>&1 || {
		echo "ERROR: flyctl is not authenticated. Run 'flyctl auth login', or export" >&2
		echo "       FLY_API_TOKEN from the environment's fly-api-token vault item." >&2
		return 1
	}
}

# dragonfly_cluster_run <app> <members|flush|verify>
dragonfly_cluster_run() {
	local app="$1" verb="$2" payload
	payload="$(base64 <"$FLY_EXEC_DIR/cluster_ops.sh" | tr -d '\n')"
	# stdin from /dev/null: an ssh session reads its caller's stdin, which would
	# swallow the typed confirmation 02_teardown.sh reads after the preflight.
	"${FLYCTL:-flyctl}" ssh console --app "$app" --command \
		"sh -c 'echo $payload | base64 -d | sh -s -- $verb $DRAGONFLY_EXPECTED_MEMBERS'" </dev/null
}
