#!/bin/bash
# 02_teardown.sh - FLUSHALL every primary of an environment's Dragonfly cluster
#
# Runs cluster_ops.sh `flush` inside the Dragonfly app (fly_exec.sh). It reads
# the members from the cluster's own view and refuses before flushing anything
# unless all of them are present, online and answering, so it never flushes
# part of a cluster. Replicas empty by replication; 03_verify.sh proves it.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=fly_exec.sh
source "${SCRIPT_DIR}/fly_exec.sh"

echo ""
echo "== dragonfly-teardown Section 2: cluster-wide flush =="
echo ""

# Checked again here: this step is separately runnable and destructive.
if [ "${ALLOW_DRAGONFLY_TEARDOWN:-0}" != "1" ]; then
	echo "ERROR: ALLOW_DRAGONFLY_TEARDOWN=1 required" >&2
	exit 1
fi

env_mode="${ENV:-}"
if [ -z "$env_mode" ]; then
	echo "ERROR: ENV must be set explicitly" >&2
	echo "Usage: ENV=dev ./00_gate.sh   or   ENV=prod ./00_gate.sh" >&2
	exit 1
fi
if [ "$env_mode" != "dev" ] && [ "$env_mode" != "prod" ]; then
	echo "ERROR: ENV must be 'dev' or 'prod' (destructive operations require explicit targeting)" >&2
	exit 1
fi

case "$env_mode" in
	dev) env_label="DEVELOPMENT" ;;
	prod) env_label="PRODUCTION" ;;
esac
app="$(dragonfly_app_for_env "$env_mode")"

echo "================================================"
echo "TARGET: $env_label ($app)"
echo "================================================"
echo "⚠️  WARNING: This will PERMANENTLY FLUSH every key on every cluster member!"
echo ""
echo "To proceed with $env_label teardown, type the environment name: $env_label"
read -r confirmation || confirmation=""
if [ "$confirmation" != "$env_label" ]; then
	echo "❌ Confirmation failed. Expected '$env_label', got '$confirmation'"
	exit 1
fi

echo "Confirmation accepted. Flushing every primary inside $app..."
if dragonfly_cluster_run "$app" flush; then
	echo ""
	echo "✅ section 2 passed - every primary flushed"
else
	echo ""
	echo "❌ section 2 failed - the member named above stopped the flush; run 03_verify.sh to see what remains" >&2
	exit 1
fi
