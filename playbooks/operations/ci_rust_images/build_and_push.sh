#!/usr/bin/env bash
# build_and_push.sh — build the CI Rust base image and push it to GHCR.
#
# Subcommands:
#   build         (default)  build (and push, unless --no-push) the image
#   help                     show usage
#
# Flags:
#   --rust-version <v>  override RUST_VERSION from versions.env (e.g. 1.98.0)
#   --revision <r>      tag suffix for iterating without breaking pinned
#                       consumers (e.g. --revision r2 → :1.98.0-r2)
#   --registry <r>      default: ghcr.io/agentsfleet
#   --no-push           docker buildx --load instead of --push
#
# There is no `fetch-shas` here, unlike the Zig script: rustup, bun and the
# gitleaks release are each fetched over TLS from their own project, and rustup
# verifies its toolchains against a signed manifest. A hash file would be a
# second thing to keep true, not a second guarantee.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
VERSIONS_FILE="$SCRIPT_DIR/versions.env"
TOOLCHAIN_FILE="$SCRIPT_DIR/../../../rustd/rust-toolchain.toml"

REGISTRY_DEFAULT="ghcr.io/agentsfleet"
BUILDER_NAME="ci-rust-builder"

log()   { printf '  %s\n' "$*"; }
ok()    { printf '  ✓ %s\n' "$*"; }
fatal() { printf 'FAIL: %s\n' "$*" >&2; exit 1; }

usage() {
  sed -n '2,19p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
  exit "${1:-0}"
}

require_tool() {
  command -v "$1" >/dev/null 2>&1 || fatal "required tool not found: $1"
}

load_versions() {
  [ -f "$VERSIONS_FILE" ] || fatal "versions.env not found at $VERSIONS_FILE"
  set -a
  # shellcheck source=/dev/null
  . "$VERSIONS_FILE"
  set +a
  : "${RUST_VERSION:?RUST_VERSION missing from versions.env}"
  : "${BUN_VERSION:?BUN_VERSION missing from versions.env}"
  : "${GITLEAKS_VERSION:?GITLEAKS_VERSION missing from versions.env}"
}

# The one check that makes this image trustworthy: an image whose compiler
# differs from `rust-toolchain.toml` would compile the workspace with a rustc no
# developer and no hook uses, and every difference it caused would look like a
# code defect. Refuse to build rather than publish that.
verify_toolchain_pin() {
  [ -f "$TOOLCHAIN_FILE" ] || fatal "rust-toolchain.toml not found at $TOOLCHAIN_FILE"
  local pinned
  pinned="$(sed -n 's/^channel[[:space:]]*=[[:space:]]*"\([^"]*\)".*/\1/p' "$TOOLCHAIN_FILE")"
  [ -n "$pinned" ] || fatal "could not read the channel from $TOOLCHAIN_FILE"
  [ "$pinned" = "$RUST_VERSION" ] || fatal \
    "RUST_VERSION ($RUST_VERSION) does not match rust-toolchain.toml channel ($pinned) — bump both, see 001_playbook.md"
  ok "toolchain pin agrees with rust-toolchain.toml ($pinned)"
}

ensure_buildx() {
  require_tool docker
  docker buildx version >/dev/null 2>&1 \
    || fatal "docker buildx is required (Docker Desktop ships it; on Linux: install docker-buildx-plugin)"
  if docker buildx inspect "$BUILDER_NAME" >/dev/null 2>&1; then
    docker buildx use "$BUILDER_NAME" >/dev/null
  else
    log "creating buildx builder '$BUILDER_NAME'"
    docker buildx create --name "$BUILDER_NAME" --driver docker-container --use >/dev/null
  fi
  docker buildx inspect --bootstrap >/dev/null
}

ensure_ghcr_login() {
  local token="${GHCR_TOKEN:-}"
  if [ -z "$token" ] && command -v gh >/dev/null 2>&1; then
    token="$(gh auth token 2>/dev/null || true)"
  fi
  [ -n "$token" ] || fatal "GHCR auth missing — set GHCR_TOKEN or run 'gh auth login' (needs write:packages)"
  local user="${GHCR_USER:-${GITHUB_USER:-$(gh api user --jq .login 2>/dev/null || echo agentsfleet)}}"
  printf '%s' "$token" | docker login ghcr.io -u "$user" --password-stdin >/dev/null
  ok "logged in to ghcr.io as $user"
}

cmd_build() {
  load_versions
  # Applied AFTER sourcing, or the source overwrites the override and the image
  # is mistagged — the bug the Zig script records in the same place.
  [ -n "${RUST_VERSION_OVERRIDE:-}" ] && RUST_VERSION="$RUST_VERSION_OVERRIDE"
  verify_toolchain_pin
  ensure_buildx
  [ "$PUSH" -eq 1 ] && ensure_ghcr_login

  local rev_suffix="" tag action_flag="--push"
  [ -n "$REVISION" ] && rev_suffix="-${REVISION}"
  tag="${REGISTRY}/ci-rust-ubuntu:${RUST_VERSION}${rev_suffix}"
  [ "$PUSH" -eq 0 ] && action_flag="--load"

  # An array, so a registry or tag carrying a space cannot split into two words.
  local -a build_args=(
    --platform linux/amd64
    --build-arg "RUST_VERSION=$RUST_VERSION"
    --build-arg "BUN_VERSION=$BUN_VERSION"
    --build-arg "GITLEAKS_VERSION=$GITLEAKS_VERSION"
    -f "$SCRIPT_DIR/Dockerfile.ubuntu"
    -t "$tag"
    "$action_flag"
    "$SCRIPT_DIR"
  )

  log "→ building ci-rust-ubuntu (linux/amd64) → $tag"
  docker buildx build "${build_args[@]}"
  ok "done → $tag"
  log "smoke-verify it with the commands in 001_playbook.md §3"
}

REGISTRY="$REGISTRY_DEFAULT"
PUSH=1
REVISION=""
RUST_VERSION_OVERRIDE=""
SUBCOMMAND=""

while [ $# -gt 0 ]; do
  case "$1" in
    build|help)      SUBCOMMAND="$1"; shift ;;
    --rust-version)  RUST_VERSION_OVERRIDE="${2:?--rust-version needs a value}"; shift 2 ;;
    --revision)      REVISION="${2:?--revision needs a value}"; shift 2 ;;
    --registry)      REGISTRY="${2:?--registry needs a value}"; shift 2 ;;
    --no-push)       PUSH=0; shift ;;
    -h|--help)       usage 0 ;;
    *)               fatal "unknown argument: $1" ;;
  esac
done

[ -z "$SUBCOMMAND" ] && SUBCOMMAND="build"

case "$SUBCOMMAND" in
  build) cmd_build ;;
  help)  usage 0 ;;
  *)     fatal "unknown subcommand: $SUBCOMMAND" ;;
esac
