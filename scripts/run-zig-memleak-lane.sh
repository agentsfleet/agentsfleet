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

# A lane's binaries are independent processes over already-built artifacts, so
# they gate concurrently. That matters most for the `lib` lane, which carries
# three binaries and previously ran them one after another under Valgrind's
# 10-30x slowdown while the other two lanes had already finished.
#
# Output is captured per binary and replayed in list order after the wait, so a
# concurrent run reads exactly like the serial one did. Interleaved Valgrind
# reports from three processes are unreadable, and a readable report is this
# gate's whole value.
run_gates_concurrently() {
  local log_dir
  log_dir=$(mktemp -d)
  # shellcheck disable=SC2064  # expand log_dir now: it must survive the return.
  trap "rm -rf '$log_dir'" RETURN

  local -a pids=() names=()
  local binary index
  for binary in "$@"; do
    gate_one "$binary" > "$log_dir/$binary.log" 2>&1 &
    pids+=("$!")
    names+=("$binary")
  done

  local status=0
  for index in "${!pids[@]}"; do
    wait "${pids[index]}" || status=1
  done
  for index in "${!names[@]}"; do
    cat "$log_dir/${names[index]}.log"
  done
  return "$status"
}

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
    gate_one() {
      echo "→ [$lane] Valgrind leak gate: $1..."
      valgrind --quiet --leak-check=full --show-leak-kinds=all \
        --errors-for-leak-kinds=definite,possible --undef-value-errors=no \
        --error-exitcode=1 "zig-out/bin/$1"
    }
    run_gates_concurrently "$@"
    ;;
  Darwin)
    echo "→ [$lane] Building for the allocator gate..."
    zig "${build_args[@]}"
    gate_one() {
      echo "→ [$lane] allocator leak gate: $1..."
      "zig-out/bin/$1"
      if [[ "${MACOS_LEAKS_SUPPORTED:-0}" == 1 ]]; then
        echo "→ [$lane] leaks advisory: $1..."
        MallocStackLogging=1 leaks -atExit -- "zig-out/bin/$1" >/dev/null ||
          echo "→ [$lane] leaks advisory failed; allocator result remains authoritative"
      fi
    }
    run_gates_concurrently "$@"
    ;;
  *)
    echo "→ [$lane] platform=$platform: allocator gate only"
    zig "${build_args[@]}"
    gate_one() {
      echo "→ [$lane] allocator leak gate: $1..."
      "zig-out/bin/$1"
    }
    run_gates_concurrently "$@"
    ;;
esac

echo "✓ [$lane] memleak lane passed"
