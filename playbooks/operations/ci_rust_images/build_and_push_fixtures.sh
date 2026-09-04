#!/usr/bin/env bash
# build_and_push_fixtures.sh — hermetic fixtures and stubs for
# `build_and_push_test.sh`. Sourced by it, never run on its own; the suite
# outgrew one file and the harness is the half that splits cleanly.
#
# The stubs are deliberately picky. `stub_curl` serves DIFFERENT bytes per
# architecture, because one blob for both let a swapped architecture-to-key
# mapping pass every test. `stub_curl_slow` announces itself before stalling,
# so a cancellation test can signal a refresh that is genuinely in flight.
#
# Self-contained: it resolves the script under test from its own location and
# owns the parent directory every fixture lives under, so the suite next door
# carries assertions and nothing else.

FIXTURES_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
readonly FIXTURES_DIR
readonly REAL_SCRIPT="$FIXTURES_DIR/build_and_push.sh"

# Every fixture lives under ONE parent, so cleanup is a single removal that a
# Ctrl-C also gets. `fixture` runs inside a command substitution — a subshell —
# so anything it appended to an array in there would never reach the suite;
# a parent directory sidesteps that entirely. Single-quoted so $ROOT expands at
# signal time rather than being baked into the trap body.
ROOT="$(mktemp -d "${TMPDIR:-/tmp}/build-push-test.XXXXXX")"
readonly ROOT
trap 'rm -rf "$ROOT"' EXIT INT TERM

sha_of_string() {
  if command -v sha256sum >/dev/null 2>&1; then printf '%s' "$1" | sha256sum | awk '{print $1}'
  else printf '%s' "$1" | shasum -a 256 | awk '{print $1}'; fi
}

# A tree shaped the way the script resolves paths: it reads versions.env beside
# itself, and rust-toolchain.toml plus Cargo.toml three directories up.
fixture() {
  local dir pinned="${1:-1.98.1}" rust="${2:-1.98.1}" floor="${3:-1.98.1}"
  dir="$(mktemp -d "$ROOT/case.XXXXXX")"
  mkdir -p "$dir/playbooks/operations/ci_rust_images" "$dir/rustd" "$dir/bin"
  cp "$REAL_SCRIPT" "$dir/playbooks/operations/ci_rust_images/"
  printf '[toolchain]\nchannel = "%s"\n' "$pinned" > "$dir/rustd/rust-toolchain.toml"
  printf '[workspace.package]\nrust-version = "%s"\n' "$floor" > "$dir/rustd/Cargo.toml"
  cat > "$dir/playbooks/operations/ci_rust_images/versions.env" <<EOF
# Prose that must survive a rewrite.
RUST_VERSION=$rust
ALPINE_SERIES=3.24

# More prose.
RUSTUP_VERSION=1.29.0
RUSTUP_SHA256_X86_64_MUSL=1111111111111111111111111111111111111111111111111111111111111111
RUSTUP_SHA256_AARCH64_MUSL=2222222222222222222222222222222222222222222222222222222222222222
EOF
  printf '%s\n' "$dir"
}

env_file() { printf '%s/playbooks/operations/ci_rust_images/versions.env' "$1"; }

# Records its argv instead of building anything.
stub_docker() {
  cat > "$1/bin/docker" <<'STUB'
#!/usr/bin/env bash
printf '%s\n' "$@" > "$STUB_ARGS"
STUB
  chmod +x "$1/bin/docker"
}

# MODE=ok        bytes and published checksum agree, and differ per architecture
# MODE=mismatch  published checksum does not describe the bytes served
# MODE=halfway   aarch64 fails outright, so x86_64 has already succeeded
stub_curl() {
  cat > "$1/bin/curl" <<'STUB'
#!/usr/bin/env bash
out=""; url=""
while [ $# -gt 0 ]; do
  case "$1" in
    -o) out="$2"; shift 2 ;;
    --max-time) shift 2 ;;
    -*) shift ;;
    *) url="$1"; shift ;;
  esac
done
base="${url%.sha256}"
case "$base" in
  *aarch64*) body="rustup-init-aarch64" ;;
  *x86_64*)  body="rustup-init-x86_64" ;;
  *)         body="rustup-init-unknown" ;;
esac
[ "${MODE:-ok}" = "halfway" ] && case "$base" in *aarch64*) exit 22 ;; esac
sum() {
  if command -v sha256sum >/dev/null 2>&1; then printf '%s' "$1" | sha256sum | awk '{print $1}'
  else printf '%s' "$1" | shasum -a 256 | awk '{print $1}'; fi
}
if [ "$url" != "$base" ]; then
  [ "${MODE:-ok}" = "mismatch" ] && body="something-else-entirely"
  printf '%s *./rustup-init\n' "$(sum "$body")"
else
  printf '%s' "$body" > "$out"
fi
STUB
  chmod +x "$1/bin/curl"
}

# Announces itself, then stalls, so a signal can arrive while the refresh is
# genuinely in flight rather than in a race with process startup.
stub_curl_slow() {
  cat > "$1/bin/curl" <<'STUB'
#!/usr/bin/env bash
out=""; url=""
while [ $# -gt 0 ]; do
  case "$1" in
    -o) out="$2"; shift 2 ;;
    --max-time) shift 2 ;;
    -*) shift ;;
    *) url="$1"; shift ;;
  esac
done
case "$url" in
  *.sha256) printf '%s *./rustup-init\n' "0000000000000000000000000000000000000000000000000000000000000000" ;;
  *) : > "${MARKER:-/dev/null}"; sleep 3; printf 'x' > "$out" ;;
esac
STUB
  chmod +x "$1/bin/curl"
}

run_script() {
  local dir="$1"; shift
  ( cd "$dir" && PATH="$dir/bin:$PATH" STUB_ARGS="$dir/args" \
      bash "$dir/playbooks/operations/ci_rust_images/build_and_push.sh" "$@" ) 2>&1
}
