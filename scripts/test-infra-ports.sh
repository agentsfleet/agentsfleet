#!/usr/bin/env bash
# test-infra-ports.sh — assign STABLE host ports to this worktree's test infra.
#
#     bash scripts/test-infra-ports.sh        # writes ./.env, prints the ports
#
# ## Why this exists
#
# The test-infra services carry no `container_name`, so Compose namespaces them
# per project and several worktrees can run the integration lane at once. The
# first version of that fix also dropped the host-port side of the mapping
# (`ports: ["6379"]`), letting Docker assign an ephemeral port per container.
#
# That is not stable. An ephemeral published port is reassigned every time the
# container is recreated OR restarted, while make/test-integration.mk resolves it
# into a URL string exactly once per run. Any restart between that resolution and
# the tests leaves the suite dialling a port nothing is listening on. Observed:
# the Redis lane connected to :57390 while the container had moved to :63324, and
# every test that opens a real Redis connection failed at TCP connect — which
# reads as a pile of unrelated pub/sub failures, not as a port fault.
#
# So the port must be per-worktree AND fixed. It is derived from the Compose
# project name, which is already unique per worktree (the directory basename).
# Same worktree always gets the same three ports; a restart cannot move them.
#
# ## Range
#
# 20000-28999, deliberately BELOW the macOS ephemeral range (49152-65535). The
# ephemeral ports Docker was handing out (50932, 57390, 61522, 63324) all sat
# inside it, so a pinned port up there could collide with an unrelated transient
# socket and fail to bind for reasons no one could reproduce.
#
# Two worktrees hashing to the same slot is possible (~0.2% at four worktrees).
# That surfaces as a loud "port is already allocated" from `docker compose up` —
# a failure to bind, never a silent share of one database. Rename the worktree
# directory to move it to a different slot.
set -euo pipefail

# Managed keys. Anything else already in .env is preserved verbatim.
PG_KEY=AGENTSFLEET_PG_HOST_PORT
REDIS_KEY=AGENTSFLEET_REDIS_HOST_PORT
QSTASH_KEY=AGENTSFLEET_QSTASH_HOST_PORT

SLOT_COUNT=3000
RANGE_START=20000

repo_root="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
cd "$repo_root"

project="${COMPOSE_PROJECT_NAME:-$(basename "$repo_root")}"

# shasum on macOS, sha256sum on Linux/CI — the repo runs both.
if command -v shasum >/dev/null 2>&1; then
  digest="$(printf '%s' "$project" | shasum -a 256 | cut -d' ' -f1)"
elif command -v sha256sum >/dev/null 2>&1; then
  digest="$(printf '%s' "$project" | sha256sum | cut -d' ' -f1)"
else
  echo "test-infra-ports: neither shasum nor sha256sum is available" >&2
  exit 1
fi

# Last 6 hex digits is plenty of spread for 3000 slots, and stays well inside
# the 64-bit arithmetic bash 3.2 (macOS) can do.
slot=$(( 16#${digest: -6} % SLOT_COUNT ))
pg_port=$(( RANGE_START + slot * 3 ))
redis_port=$(( pg_port + 1 ))
qstash_port=$(( pg_port + 2 ))

env_file="$repo_root/.env"
tmp_file=""
# `return 0` is load-bearing: an EXIT trap's final status becomes the script's
# exit status, and `[ -n "" ]` is a failing test on the (normal) path where the
# temp file was already renamed away. Without it this script exits 1 on success.
cleanup() {
  if [ -n "$tmp_file" ]; then
    rm -f "$tmp_file"
  fi
  return 0
}
trap cleanup EXIT

tmp_file="$(mktemp "$repo_root/.env.XXXXXX")"

# Preserve any unrelated keys a developer keeps in .env; replace only ours.
if [ -f "$env_file" ]; then
  grep -vE "^(${PG_KEY}|${REDIS_KEY}|${QSTASH_KEY})=" "$env_file" > "$tmp_file" || true
fi

{
  printf '%s=%s\n' "$PG_KEY" "$pg_port"
  printf '%s=%s\n' "$REDIS_KEY" "$redis_port"
  printf '%s=%s\n' "$QSTASH_KEY" "$qstash_port"
} >> "$tmp_file"

mv "$tmp_file" "$env_file"
tmp_file=""

printf 'postgres=%s redis=%s qstash=%s (project %s)\n' \
  "$pg_port" "$redis_port" "$qstash_port" "$project"
