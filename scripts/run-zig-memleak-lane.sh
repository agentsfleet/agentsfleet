#!/usr/bin/env bash
set -euo pipefail

if (( $# < 5 )); then
  echo "usage: run-zig-memleak-lane.sh <lane> <build-file|-> <step> <openssl-off:0|1> <binary>..." >&2
  exit 2
fi

lane=$1
build_file=$2
build_step=$3
openssl_off=$4
shift 4

: "${ZIG_GLOBAL_CACHE_DIR:?ZIG_GLOBAL_CACHE_DIR is required}"
: "${ZIG_LOCAL_CACHE_DIR:?ZIG_LOCAL_CACHE_DIR is required}"
export ZIG_GLOBAL_CACHE_DIR ZIG_LOCAL_CACHE_DIR

build_args=(build)
if [[ "$build_file" != "-" ]]; then
  build_args+=(--build-file "$build_file")
fi
build_args+=("$build_step")
if [[ "$openssl_off" == 1 ]]; then
  build_args+=(-Dopenssl=false)
fi
if [[ -n "${MEMLEAK_CPU:-}" ]]; then
  build_args+=("-Dcpu=$MEMLEAK_CPU")
fi

platform=$("${UNAME_BIN:-uname}" -s)
case "$platform" in
  Linux)
    command -v valgrind >/dev/null 2>&1 || {
      echo "✗ valgrind is required on Linux for make memleak"
      exit 1
    }
    build_args+=(-Doptimize=ReleaseSafe)
    echo "→ [$lane] Building for the Valgrind gate..."
    zig "${build_args[@]}"
    for binary in "$@"; do
      echo "→ [$lane] Valgrind leak gate: $binary..."
      valgrind --quiet --leak-check=full --show-leak-kinds=all \
        --errors-for-leak-kinds=definite,possible --undef-value-errors=no \
        --error-exitcode=1 "zig-out/bin/$binary"
    done
    ;;
  Darwin)
    echo "→ [$lane] Building for the allocator gate..."
    zig "${build_args[@]}"
    for binary in "$@"; do
      echo "→ [$lane] allocator leak gate: $binary..."
      "zig-out/bin/$binary"
      if [[ "${MACOS_LEAKS_SUPPORTED:-0}" == 1 ]]; then
        echo "→ [$lane] leaks advisory: $binary..."
        MallocStackLogging=1 leaks -atExit -- "zig-out/bin/$binary" >/dev/null ||
          echo "→ [$lane] leaks advisory failed; allocator result remains authoritative"
      fi
    done
    ;;
  *)
    echo "→ [$lane] platform=$platform: allocator gate only"
    zig "${build_args[@]}"
    for binary in "$@"; do
      "zig-out/bin/$binary"
    done
    ;;
esac

echo "✓ [$lane] memleak lane passed"
