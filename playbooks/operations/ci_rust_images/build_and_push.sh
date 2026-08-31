#!/usr/bin/env bash
# build_and_push.sh — build the Rust musl CI base image and push it to GHCR.
#
# Subcommands:
#   build         (default)  build (and push, unless --no-push) the image
#   help                     show usage
#
# Flags:
#   --rust-version <v>   override RUST_VERSION from versions.env (e.g. 1.98.0)
#   --alpine-series <s>  override ALPINE_SERIES from versions.env (e.g. 3.24)
#   --revision <r>       tag suffix for iterating without breaking pinned
#                        consumers (e.g. --revision r2 → :1.98.0-alpine3.24-r2)
#   --registry <r>       default: ghcr.io/agentsfleet
#   --no-push            docker buildx --load instead of --push (single-arch)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
readonly SCRIPT_DIR
readonly VERSIONS_FILE="$SCRIPT_DIR/versions.env"
readonly REGISTRY_DEFAULT="ghcr.io/agentsfleet"
readonly IMAGE_NAME="ci-rust-alpine"
# Both, always, for a pushed image: `--platform` on a foreign host runs the
# whole compile under QEMU, and emulation costs more than the packages this
# image exists to cache.
readonly PLATFORMS="linux/amd64,linux/arm64"

log()   { printf '  %s\n' "$*"; }
ok()    { printf '  ✓ %s\n' "$*"; }
fatal() { printf 'FAIL: %s\n' "$*" >&2; exit 1; }

usage() {
  sed -n '2,15p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
  exit "${1:-0}"
}

load_versions() {
  [ -f "$VERSIONS_FILE" ] || fatal "versions.env not found at $VERSIONS_FILE"
  set -a
  # shellcheck source=/dev/null
  . "$VERSIONS_FILE"
  set +a
  : "${RUST_VERSION:?RUST_VERSION missing from versions.env}"
  : "${ALPINE_SERIES:?ALPINE_SERIES missing from versions.env}"
}

# The pin has to match the repository's own toolchain. An image compiling with a
# different rustc than every other lane is a second compiler nobody chose, and
# the binary it produces is not the one the tests graded.
assert_toolchain_matches() {
  local pinned
  pinned="$(sed -n 's/^channel *= *"\([^"]*\)".*/\1/p' "$SCRIPT_DIR/../../../rustd/rust-toolchain.toml" 2>/dev/null || true)"
  [ -n "$pinned" ] || { log "no rust-toolchain.toml channel found — skipping the match check"; return 0; }
  [ "$pinned" = "$RUST_VERSION" ] || fatal \
    "versions.env RUST_VERSION=$RUST_VERSION but rust-toolchain.toml pins $pinned — the image would compile with a different rustc"
}

main() {
  local registry="$REGISTRY_DEFAULT" revision="" push=1
  local subcommand="build"
  [ $# -gt 0 ] && case "$1" in build|help) subcommand="$1"; shift ;; esac
  [ "$subcommand" = "help" ] && usage 0

  load_versions
  while [ $# -gt 0 ]; do
    case "$1" in
      --rust-version)   RUST_VERSION="${2:?}"; shift 2 ;;
      --alpine-series)  ALPINE_SERIES="${2:?}"; shift 2 ;;
      --revision)       revision="-${2:?}"; shift 2 ;;
      --registry)       registry="${2:?}"; shift 2 ;;
      --no-push)        push=0; shift ;;
      *) fatal "unknown argument: $1" ;;
    esac
  done

  command -v docker >/dev/null 2>&1 || fatal "required tool not found: docker"
  assert_toolchain_matches

  local tag="$registry/$IMAGE_NAME:${RUST_VERSION}-alpine${ALPINE_SERIES}${revision}"
  local -a args=(
    buildx build "$SCRIPT_DIR"
    -f "$SCRIPT_DIR/Dockerfile.alpine"
    --build-arg "RUST_VERSION=$RUST_VERSION"
    --build-arg "ALPINE_SERIES=$ALPINE_SERIES"
    -t "$tag"
  )
  if [ "$push" = "1" ]; then
    args+=(--platform "$PLATFORMS" --push)
    log "building $tag for $PLATFORMS and pushing"
  else
    # `--load` cannot accept a multi-platform manifest, so a local build is the
    # host's architecture only. That is a docker limitation, not a choice.
    args+=(--load)
    log "building $tag for this architecture, loading locally (no push)"
  fi
  docker "${args[@]}" || fatal "buildx failed"
  ok "$tag"
}

main "$@"
