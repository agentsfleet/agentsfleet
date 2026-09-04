#!/usr/bin/env bash
# build_and_push.sh — build the Rust musl CI base image and push it to GHCR.
#
# Subcommands:
#   build         (default)  build (and push, unless --no-push) the image
#   fetch-shas               re-read the rustup-init checksums (writes versions.env)
#   help                     show usage
#
# Flags:
#   --rust-version <v>   override RUST_VERSION from versions.env (e.g. 1.98.1)
#   --alpine-series <s>  override ALPINE_SERIES from versions.env (e.g. 3.24)
#   --rustup-version <v> override RUSTUP_VERSION from versions.env (e.g. 1.29.0)
#   --revision <r>       tag suffix for iterating without breaking pinned
#                        consumers (e.g. --revision r2 → :1.98.1-alpine3.24-r2)
#   --registry <r>       default: ghcr.io/agentsfleet
#   --no-push            docker buildx --load instead of --push (single-arch)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
readonly SCRIPT_DIR
readonly VERSIONS_FILE="$SCRIPT_DIR/versions.env"
readonly REGISTRY_DEFAULT="ghcr.io/agentsfleet"
readonly IMAGE_NAME="ci-rust-alpine"
readonly RUSTUP_BASE_URL="https://static.rust-lang.org/rustup/archive"
# Every pin this script reads. Named once so `load_versions` can clear them from
# the environment before sourcing: `${VAR:?}` fires on UNSET, not on absent-from-
# file, so an operator who happens to export RUSTUP_VERSION would otherwise bake
# a value the file does not contain while the error message blames the file.
readonly PINS=(
  RUST_VERSION ALPINE_SERIES RUSTUP_VERSION
  RUSTUP_SHA256_X86_64_MUSL RUSTUP_SHA256_AARCH64_MUSL
)
# The two architectures the image publishes, in the spelling rustup's dist
# layout uses. `apk --print-arch` maps onto these inside the Dockerfile.
readonly RUSTUP_ARCHES=(x86_64-unknown-linux-musl aarch64-unknown-linux-musl)
# Both, always, for a pushed image: `--platform` on a foreign host runs the
# whole compile under QEMU, and emulation costs more than the packages this
# image exists to cache.
readonly PLATFORMS="linux/amd64,linux/arm64"

# Set by cmd_fetch_shas; the traps below expand it at signal time, not at
# trap-installation time, so no path ever lands inside the trap body.
SCRATCH=""
# `return 0` is load-bearing: an EXIT trap's status becomes the script's, so a
# bare `[ -n "$SCRATCH" ] && rm -rf ...` made every successful build exit 1 on
# the path that never allocates a scratch directory.
cleanup() { [ -n "$SCRATCH" ] && rm -rf "$SCRATCH"; return 0; }
trap cleanup EXIT INT TERM

log()   { printf '  %s\n' "$*"; }
ok()    { printf '  ✓ %s\n' "$*"; }
fatal() { printf 'FAIL: %s\n' "$*" >&2; exit 1; }

usage() {
  sed -n '2,17p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
  exit "${1:-0}"
}

require_tool() {
  command -v "$1" >/dev/null 2>&1 || fatal "required tool not found: $1"
}

load_versions() {
  [ -f "$VERSIONS_FILE" ] || fatal "versions.env not found at $VERSIONS_FILE"
  unset "${PINS[@]}"
  set -a
  # shellcheck source=/dev/null
  . "$VERSIONS_FILE"
  set +a
  local pin
  for pin in "${PINS[@]}"; do
    [ -n "${!pin:-}" ] || fatal "$pin missing from versions.env"
  done
}

# The pin has to match the repository's own toolchain. An image compiling with a
# different rustc than every other lane is a second compiler nobody chose, and
# the binary it produces is not the one the tests graded. `rustd/Cargo.toml`'s
# `rust-version` is checked too: it is the workspace's hard floor, so a bump that
# moves it ahead of the pin makes cargo refuse to build for everyone, and one
# that leaves it behind quietly re-opens the gap the floor exists to close.
assert_toolchain_matches() {
  local root="$SCRIPT_DIR/../../.." pinned floor
  pinned="$(sed -n 's/^channel *= *"\([^"]*\)".*/\1/p' "$root/rustd/rust-toolchain.toml" 2>/dev/null || true)"
  [ -n "$pinned" ] || { log "no rust-toolchain.toml channel found — skipping the match check"; return 0; }
  [ "$pinned" = "$RUST_VERSION" ] || fatal \
    "versions.env RUST_VERSION=$RUST_VERSION but rust-toolchain.toml pins $pinned — the image would compile with a different rustc"
  floor="$(sed -n 's/^rust-version *= *"\([^"]*\)".*/\1/p' "$root/rustd/Cargo.toml" 2>/dev/null || true)"
  [ -n "$floor" ] || { log "no Cargo.toml rust-version found — skipping the floor check"; return 0; }
  [ "$floor" = "$RUST_VERSION" ] || fatal \
    "rustd/Cargo.toml rust-version=$floor but the toolchain pin is $RUST_VERSION — the workspace floor must equal the pin, not trail or lead it"
}

# macOS ships `shasum`, Alpine and the GitHub runners ship `sha256sum`. Picking
# at call time keeps this runnable from a laptop and from a lane.
sha256_of() {
  if command -v sha256sum >/dev/null 2>&1; then sha256sum "$1" | awk '{print $1}'
  else require_tool shasum; shasum -a 256 "$1" | awk '{print $1}'; fi
}

# Read the checksum the PUBLISHER states, then confirm the bytes that arrived
# hash to it. Hashing our own download and calling the result a trust anchor
# would only prove the file did not change after we fetched it; upstream serves
# `<artifact>.sha256` beside every artifact, and rust-lang's own image tooling
# reads exactly that. It still cannot outrank a wholly compromised
# static.rust-lang.org — nothing short of a signature can — but it does catch a
# truncated, cached, or tampered-in-transit body, which is the failure a
# self-computed hash would happily bless.
download_verified() {
  local url="$1" out="$2" published local_sum
  published="$(curl -fsSL --max-time 60 "$url.sha256" | awk 'NR==1 {print $1}')" \
    || fatal "could not read the published checksum at $url.sha256"
  [ -n "$published" ] || fatal "the published checksum at $url.sha256 was empty"
  curl -fsSL --max-time 120 -o "$out" "$url" || fatal "could not download $url"
  local_sum="$(sha256_of "$out")"
  [ "$local_sum" = "$published" ] || fatal \
    "checksum mismatch for $url — upstream publishes $published, the bytes that arrived hash to $local_sum"
  printf '%s' "$published"
}

# One rewrite for every key, in place, so the prose beside each pin survives —
# and one `mv`, so an interrupted refresh cannot leave versions.env carrying a
# new checksum beside a stale one.
write_versions() {
  local tmp="$SCRATCH/versions.env" key value
  cp "$VERSIONS_FILE" "$tmp"
  while [ $# -gt 0 ]; do
    key="$1"; value="$2"; shift 2
    grep -q "^${key}=" "$tmp" || fatal "$key is not in versions.env — add it before refreshing"
    sed "s|^${key}=.*|${key}=${value}|" "$tmp" > "$tmp.next" && mv "$tmp.next" "$tmp"
  done
  mv "$tmp" "$VERSIONS_FILE"
}

cmd_fetch_shas() {
  require_tool curl
  log "reading rustup-init $RUSTUP_VERSION checksums published at $RUSTUP_BASE_URL"
  SCRATCH="$(mktemp -d "${TMPDIR:-/tmp}/rustup-shas.XXXXXX")" || fatal "mktemp failed"
  local arch sum key
  local -a updates=(RUSTUP_VERSION "$RUSTUP_VERSION")
  for arch in "${RUSTUP_ARCHES[@]}"; do
    case "$arch" in
      x86_64-*)  key=RUSTUP_SHA256_X86_64_MUSL ;;
      aarch64-*) key=RUSTUP_SHA256_AARCH64_MUSL ;;
      *) fatal "no versions.env key for $arch" ;;
    esac
    sum="$(download_verified "$RUSTUP_BASE_URL/$RUSTUP_VERSION/$arch/rustup-init" "$SCRATCH/$arch")"
    updates+=("$key" "$sum")
    ok "$key=$sum"
  done
  # RUSTUP_VERSION rides along: fetch-shas is the only way to move the installer
  # pin, so writing the checksums without it would leave the file describing one
  # version with another version's bytes.
  write_versions "${updates[@]}"
  ok "versions.env updated (RUSTUP_VERSION=$RUSTUP_VERSION)"
}

cmd_build() {
  local registry="$1" revision="$2" push="$3"
  require_tool docker
  assert_toolchain_matches

  local tag="$registry/$IMAGE_NAME:${RUST_VERSION}-alpine${ALPINE_SERIES}${revision}"
  local -a args=(
    buildx build "$SCRIPT_DIR"
    -f "$SCRIPT_DIR/Dockerfile.alpine"
    --build-arg "RUST_VERSION=$RUST_VERSION"
    --build-arg "ALPINE_SERIES=$ALPINE_SERIES"
    --build-arg "RUSTUP_VERSION=$RUSTUP_VERSION"
    --build-arg "RUSTUP_SHA256_X86_64_MUSL=$RUSTUP_SHA256_X86_64_MUSL"
    --build-arg "RUSTUP_SHA256_AARCH64_MUSL=$RUSTUP_SHA256_AARCH64_MUSL"
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

main() {
  local registry="$REGISTRY_DEFAULT" revision="" push=1
  local subcommand="build"
  [ $# -gt 0 ] && case "$1" in build|fetch-shas|help) subcommand="$1"; shift ;; esac
  [ "$subcommand" = "help" ] && usage 0

  load_versions
  while [ $# -gt 0 ]; do
    case "$1" in
      --rust-version)   RUST_VERSION="${2:?}"; shift 2 ;;
      --alpine-series)  ALPINE_SERIES="${2:?}"; shift 2 ;;
      --rustup-version) RUSTUP_VERSION="${2:?}"; shift 2 ;;
      --revision)       revision="-${2:?}"; shift 2 ;;
      --registry)       registry="${2:?}"; shift 2 ;;
      --no-push)        push=0; shift ;;
      *) fatal "unknown argument: $1" ;;
    esac
  done

  if [ "$subcommand" = "fetch-shas" ]; then cmd_fetch_shas
  else cmd_build "$registry" "$revision" "$push"; fi
}

main "$@"
