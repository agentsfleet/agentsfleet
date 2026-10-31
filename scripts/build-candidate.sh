#!/usr/bin/env bash
# The shipping build formerly repeated by test, deploy-dev and release.
set -euo pipefail

linux() {
  : "${CANDIDATE_PACKAGE:?}" "${CANDIDATE_BINARY:?}" "${CANDIDATE_FILENAME:?}"
  : "${CANDIDATE_ARCH:?}" "${CANDIDATE_BUILDER:?}" "${GIT_COMMIT:?}"
  mkdir -p .cargo-cache/registry .cargo-cache/git rustd/target dist/candidate/bin
  docker run --rm --platform "linux/$CANDIDATE_ARCH" -v "$PWD:/w" -w /w/rustd \
    -v "$PWD/.cargo-cache/registry:/usr/local/cargo/registry" \
    -v "$PWD/.cargo-cache/git:/usr/local/cargo/git" \
    -e GIT_COMMIT -e CANDIDATE_PACKAGE -e CANDIDATE_BINARY \
    "$CANDIDATE_BUILDER" sh -c \
    'cargo build --profile dist -p "$CANDIDATE_PACKAGE" --bin "$CANDIDATE_BINARY"'
  sudo chown -R "$(id -u):$(id -g)" .cargo-cache rustd/target
  install -m 755 "rustd/target/dist/$CANDIDATE_BINARY" "dist/candidate/bin/$CANDIDATE_FILENAME"
  local bin="dist/candidate/bin/$CANDIDATE_FILENAME"
  readelf -h "$bin"
  if readelf -d "$bin" | grep -q ' (NEEDED)'; then
    echo "FAIL: dynamic NEEDED entries in $bin"; exit 1
  fi
  if readelf -l "$bin" | grep -q INTERP; then
    echo "FAIL: dynamic loader in $bin"; exit 1
  fi
}

macos() {
  : "${GIT_COMMIT:?}"
  (cd rustd && cargo build --profile dist -p agentsfleetd --bin agentsfleetd)
  mkdir -p dist/candidate/bin
  install -m 755 rustd/target/dist/agentsfleetd dist/candidate/bin/agentsfleetd-darwin-arm64
}

image() {
  : "${CANDIDATE_ARCH:?}" "${CANDIDATE_FILENAME:?}"
  mkdir -p dist/candidate/images
  cp "dist/candidate/bin/$CANDIDATE_FILENAME" "dist/$CANDIDATE_FILENAME"
  local image="agentsfleet-candidate:daemon-linux-$CANDIDATE_ARCH" out
  docker build --platform "linux/$CANDIDATE_ARCH" --build-arg "TARGETARCH=$CANDIDATE_ARCH" -t "$image" .
  # No configuration: the preflight must report, rather than fail to load.
  out="$(docker run --rm "$image" /usr/local/bin/agentsfleetd 2>&1 || true)"
  printf '%s\n' "$out"
  [ -n "$out" ]
  if printf '%s' "$out" | grep -qiE 'no such file|exec format|not found|cannot execute'; then
    echo 'FAIL: daemon cannot load on its actual runtime image'; exit 1
  fi
  docker save --output "dist/candidate/images/daemon-linux-$CANDIDATE_ARCH.tar" "$image"
}

case "${1:-}" in
  linux) linux ;;
  macos) macos ;;
  image) image ;;
  *) echo 'usage: build-candidate.sh linux|macos|image' >&2; exit 2 ;;
esac
