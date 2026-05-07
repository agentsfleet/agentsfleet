#!/usr/bin/env bash
# build_and_push.sh — build the three CI Zig base images and push them to GHCR.
#
# Subcommands:
#   build         (default)  build (and push, unless --no-push) the selected images
#   fetch-shas              recompute Zig SHA256s for a given version (writes versions.env)
#   help                    show usage
#
# Flags:
#   --zig-version <v>   override ZIG_VERSION from versions.env (e.g. 0.15.2)
#   --revision <r>      tag suffix for iterating without breaking pinned consumers
#                       (e.g. --revision r3 → :0.15.2-r3). Empty by default.
#   --registry <r>      default: ghcr.io/usezombie
#   --image <name>      alpine | debian-trixie | ubuntu | all (default: all)
#   --no-push           docker buildx --load instead of --push (single-arch only)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
VERSIONS_FILE="$SCRIPT_DIR/versions.env"

REGISTRY_DEFAULT="ghcr.io/usezombie"
IMAGE_DEFAULT="all"

log()   { printf '  %s\n' "$*"; }
ok()    { printf '  ✓ %s\n' "$*"; }
fatal() { printf 'FAIL: %s\n' "$*" >&2; exit 1; }

usage() {
  sed -n '2,15p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
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
  : "${ZIG_VERSION:?ZIG_VERSION missing from versions.env}"
  : "${ZIG_SHA256_X86_64_LINUX:?ZIG_SHA256_X86_64_LINUX missing from versions.env}"
  : "${ZIG_SHA256_AARCH64_LINUX:?ZIG_SHA256_AARCH64_LINUX missing from versions.env}"
}

ensure_buildx() {
  require_tool docker
  docker buildx version >/dev/null 2>&1 \
    || fatal "docker buildx is required (Docker Desktop ships it; on Linux: install docker-buildx-plugin)"
  if ! docker buildx inspect ci-zig-builder >/dev/null 2>&1; then
    log "creating buildx builder 'ci-zig-builder' (multi-arch capable)"
    docker buildx create --name ci-zig-builder --driver docker-container --use >/dev/null
  else
    docker buildx use ci-zig-builder >/dev/null
  fi
  docker buildx inspect --bootstrap >/dev/null
}

ensure_ghcr_login() {
  local token="${GHCR_TOKEN:-}"
  if [ -z "$token" ] && command -v gh >/dev/null 2>&1; then
    token="$(gh auth token 2>/dev/null || true)"
  fi
  [ -n "$token" ] || fatal "GHCR auth missing — set GHCR_TOKEN or run 'gh auth login' (needs write:packages)"
  local user="${GHCR_USER:-${GITHUB_USER:-$(gh api user --jq .login 2>/dev/null || echo usezombie)}}"
  printf '%s' "$token" | docker login ghcr.io -u "$user" --password-stdin >/dev/null
  ok "logged in to ghcr.io as $user"
}

tag_for() {
  local variant="$1" rev_suffix=""
  [ -n "${REVISION:-}" ] && rev_suffix="-${REVISION}"
  printf '%s/ci-zig-%s:%s%s' "$REGISTRY" "$variant" "$ZIG_VERSION" "$rev_suffix"
}

build_one() {
  local variant="$1" dockerfile="$2" platforms="$3"
  local tag; tag="$(tag_for "$variant")"
  local action_flag="--push"
  [ "${PUSH:-1}" -eq 0 ] && action_flag="--load"
  if [ "$action_flag" = "--load" ] && [[ "$platforms" == *","* ]]; then
    log "→ $variant: --no-push set; building single-arch (linux/amd64) for local --load"
    platforms="linux/amd64"
  fi
  log "→ building $variant ($platforms) → $tag"
  docker buildx build \
    --platform "$platforms" \
    --build-arg "ZIG_VERSION=$ZIG_VERSION" \
    --build-arg "ZIG_SHA256_X86_64_LINUX=$ZIG_SHA256_X86_64_LINUX" \
    --build-arg "ZIG_SHA256_AARCH64_LINUX=$ZIG_SHA256_AARCH64_LINUX" \
    -f "$SCRIPT_DIR/$dockerfile" \
    -t "$tag" \
    "$action_flag" \
    "$SCRIPT_DIR"
  ok "$variant complete → $tag"
}

build_selected() {
  case "$IMAGE" in
    alpine)         build_one alpine         Dockerfile.alpine         "linux/amd64,linux/arm64" ;;
    debian-trixie)  build_one debian-trixie  Dockerfile.debian-trixie  "linux/amd64" ;;
    ubuntu)         build_one ubuntu         Dockerfile.ubuntu         "linux/amd64" ;;
    all)
      build_one alpine         Dockerfile.alpine         "linux/amd64,linux/arm64"
      build_one debian-trixie  Dockerfile.debian-trixie  "linux/amd64"
      build_one ubuntu         Dockerfile.ubuntu         "linux/amd64"
      ;;
    *) fatal "unknown --image: $IMAGE (expected alpine|debian-trixie|ubuntu|all)" ;;
  esac
}

cmd_build() {
  load_versions
  # Apply --zig-version override AFTER sourcing versions.env, otherwise the
  # source overwrites the override and the resulting image is mistagged.
  [ -n "${ZIG_VERSION_OVERRIDE:-}" ] && ZIG_VERSION="$ZIG_VERSION_OVERRIDE"
  ensure_buildx
  [ "${PUSH:-1}" -eq 1 ] && ensure_ghcr_login
  build_selected
  ok "done — Zig $ZIG_VERSION images${REVISION:+ (rev=$REVISION)} ready in $REGISTRY"
}

cmd_fetch_shas() {
  require_tool curl
  require_tool jq
  local version="${1:-${ZIG_VERSION_OVERRIDE:-}}"
  [ -n "$version" ] || { load_versions 2>/dev/null || true; version="${ZIG_VERSION:-}"; }
  [ -n "$version" ] || fatal "fetch-shas: pass a version (e.g. fetch-shas 0.15.2)"
  log "fetching SHA256s for Zig $version from ziglang.org/download/index.json"
  local json; json="$(curl -fsSL --max-time 30 https://ziglang.org/download/index.json)"
  local x86 aa x86m aam
  x86="$(printf '%s' "$json" | jq -er --arg v "$version" '.[$v]."x86_64-linux".shasum')"
  aa="$( printf '%s' "$json" | jq -er --arg v "$version" '.[$v]."aarch64-linux".shasum')"
  x86m="$(printf '%s' "$json" | jq -er --arg v "$version" '.[$v]."x86_64-macos".shasum')"
  aam="$( printf '%s' "$json" | jq -er --arg v "$version" '.[$v]."aarch64-macos".shasum')"
  ok "x86_64-linux   $x86"
  ok "aarch64-linux  $aa"
  ok "x86_64-macos   $x86m"
  ok "aarch64-macos  $aam"
  cat >"$VERSIONS_FILE.new" <<EOF
# Pinned versions + checksums for the CI Zig base images.
# Regenerate with: ./build_and_push.sh fetch-shas $version
# Source: https://ziglang.org/download/index.json

ZIG_VERSION=$version

ZIG_SHA256_X86_64_LINUX=$x86
ZIG_SHA256_AARCH64_LINUX=$aa

ZIG_SHA256_X86_64_MACOS=$x86m
ZIG_SHA256_AARCH64_MACOS=$aam
EOF
  mv "$VERSIONS_FILE.new" "$VERSIONS_FILE"
  ok "wrote $VERSIONS_FILE"
}

REGISTRY="$REGISTRY_DEFAULT"
IMAGE="$IMAGE_DEFAULT"
PUSH=1
REVISION=""
ZIG_VERSION_OVERRIDE=""
SUBCOMMAND=""

while [ $# -gt 0 ]; do
  case "$1" in
    build|fetch-shas|help) SUBCOMMAND="$1"; shift ;;
    --zig-version) ZIG_VERSION_OVERRIDE="$2"; shift 2 ;;
    --revision)    REVISION="$2"; shift 2 ;;
    --registry)    REGISTRY="$2"; shift 2 ;;
    --image)       IMAGE="$2"; shift 2 ;;
    --no-push)     PUSH=0; shift ;;
    -h|--help)     usage 0 ;;
    *)
      # `fetch-shas <version>` accepts the version as a positional arg
      # (the documented form). For other subcommands, unknown args fail loudly.
      if [ "$SUBCOMMAND" = "fetch-shas" ] && [ -z "$ZIG_VERSION_OVERRIDE" ]; then
        ZIG_VERSION_OVERRIDE="$1"; shift
      else
        fatal "unknown argument: $1"
      fi
      ;;
  esac
done

[ -z "$SUBCOMMAND" ] && SUBCOMMAND="build"

case "$SUBCOMMAND" in
  build)      cmd_build ;;
  fetch-shas) cmd_fetch_shas "$ZIG_VERSION_OVERRIDE" ;;
  help)       usage 0 ;;
  *)          fatal "unknown subcommand: $SUBCOMMAND" ;;
esac
