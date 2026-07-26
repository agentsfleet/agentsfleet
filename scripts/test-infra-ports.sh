#!/usr/bin/env bash
# test-infra-ports.sh — print the three host ports this checkout's test infra
# should publish on, as "<postgres> <redis> <qstash>".
#
#     bash scripts/test-infra-ports.sh     # e.g. "26933 26934 26935"
#
# Prints only. It deliberately writes no file: an earlier version wrote `.env`,
# which Compose reads automatically — convenient locally, but in CI the make
# target runs inside a container as root, so the `.env` it produced was
# unreadable by the host runner (`docker compose down` died with "permission
# denied") AND it moved the published ports out from under the connection
# strings the workflow had already pinned. The Makefile exports these values
# into the environment instead, which crosses no ownership boundary.
#
# ## Why the ports must be fixed
#
# The test-infra services carry no `container_name`, so Compose namespaces them
# per project and several worktrees can run the integration lane at once. The
# first version of that fix also dropped the host-port side of the mapping
# (`ports: ["6379"]`), letting Docker assign an ephemeral port per container.
#
# That is not stable. An ephemeral published port is reallocated every time the
# container is recreated OR restarted, while make/test-integration.mk resolves it
# into a URL string exactly once per run. Any restart between that resolution and
# the tests leaves the suite dialling a port nothing is listening on. Observed:
# the Redis lane connected to :57390 while the container had moved to :63324, and
# every test that opens a real Redis connection failed at TCP connect — which
# reads as a pile of unrelated pub/sub failures, not as a port fault.
#
# ## Only LINKED worktrees are pinned
#
# A primary checkout — a normal clone, which is what CI always has — keeps the
# conventional 5432/6379/8080. CI's workflow hardcodes `localhost:5432` and
# `localhost:6379` into the connection strings it passes the test container and
# boots Compose on the host before the make target runs inside it; handing those
# two invocations different ports is what broke the lane. Nothing about CI is
# multi-checkout, so it has nothing to gain from pinning and everything to lose.
#
# Linked worktrees are exactly the case that needs isolation, and their ports are
# derived from the Compose project name (the directory basename, already unique).
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

DEFAULT_PG=5432
DEFAULT_REDIS=6379
DEFAULT_QSTASH=8080

SLOT_COUNT=3000
RANGE_START=20000

# Not a git checkout at all -> nothing to isolate from; use the conventional ports.
if ! repo_root="$(git rev-parse --show-toplevel 2>/dev/null)"; then
  printf '%s %s %s\n' "$DEFAULT_PG" "$DEFAULT_REDIS" "$DEFAULT_QSTASH"
  exit 0
fi

# A linked worktree has a private git dir under the main checkout's .git; a
# primary checkout has them equal. This is the whole CI-vs-local discriminator.
git_dir="$(git rev-parse --absolute-git-dir 2>/dev/null || echo "")"
common_dir="$(cd "$repo_root" && cd "$(git rev-parse --git-common-dir 2>/dev/null || echo .)" && pwd)"

if [ -z "$git_dir" ] || [ "$git_dir" = "$common_dir" ]; then
  printf '%s %s %s\n' "$DEFAULT_PG" "$DEFAULT_REDIS" "$DEFAULT_QSTASH"
  exit 0
fi

project="${COMPOSE_PROJECT_NAME:-$(basename "$repo_root")}"

# shasum on macOS, sha256sum on Linux/CI — the repo runs both.
if command -v shasum >/dev/null 2>&1; then
  digest="$(printf '%s' "$project" | shasum -a 256 | cut -d' ' -f1)"
elif command -v sha256sum >/dev/null 2>&1; then
  digest="$(printf '%s' "$project" | sha256sum | cut -d' ' -f1)"
else
  # No hasher is not a reason to fail the build; the conventional ports are a
  # correct answer for a single checkout, which is the only case this can be.
  printf '%s %s %s\n' "$DEFAULT_PG" "$DEFAULT_REDIS" "$DEFAULT_QSTASH"
  exit 0
fi

# Last 6 hex digits is plenty of spread for 3000 slots, and stays well inside
# the 64-bit arithmetic bash 3.2 (macOS) can do.
slot=$(( 16#${digest: -6} % SLOT_COUNT ))
pg_port=$(( RANGE_START + slot * 3 ))

printf '%s %s %s\n' "$pg_port" "$(( pg_port + 1 ))" "$(( pg_port + 2 ))"
