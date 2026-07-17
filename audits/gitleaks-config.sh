#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
GITLEAKS_BIN="${GITLEAKS_BIN:-gitleaks}"
LOWER_DIGEST="0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
UPPER_DIGEST="0123456789ABCDEF0123456789ABCDEF0123456789ABCDEF0123456789ABCDEF"
DETECTED_EXIT=17

if ! command -v "$GITLEAKS_BIN" >/dev/null 2>&1; then
  printf 'FAIL: gitleaks executable not found: %s\n' "$GITLEAKS_BIN" >&2
  exit 1
fi

SCRATCH="$(mktemp -d "${TMPDIR:-/tmp}/agentsfleet-gitleaks.XXXXXX")"
cleanup() {
  rm -rf -- "$SCRATCH"
}
trap cleanup EXIT

run_case() {
  local case_name="$1"
  local expected_exit="$2"
  local source_dir="$SCRATCH/$case_name"
  local output_file="$SCRATCH/$case_name.out"
  local actual_exit

  mkdir -p "$source_dir/.oracle"
  case "$case_name" in
    allowed-lock)
      printf '{"api_key":"%s"}\n' "$LOWER_DIGEST" > "$source_dir/.oracle/ruleset.lock"
      ;;
    outside-lock)
      printf 'api_key = "%s"\n' "$LOWER_DIGEST" > "$source_dir/example.env"
      ;;
    blocked-lock)
      printf '{"api_key":"%s"}\n' "$UPPER_DIGEST" > "$source_dir/.oracle/ruleset.lock"
      ;;
    *)
      printf 'FAIL: unknown gitleaks fixture: %s\n' "$case_name" >&2
      return 1
      ;;
  esac

  set +e
  "$GITLEAKS_BIN" detect \
    --no-git \
    --source "$source_dir" \
    --config "$ROOT/.gitleaks.toml" \
    --no-banner \
    --redact \
    --exit-code "$DETECTED_EXIT" > "$output_file" 2>&1
  actual_exit=$?
  set -e

  if [[ "$actual_exit" -ne "$expected_exit" ]]; then
    printf 'FAIL: gitleaks fixture %s expected exit %s, got %s\n' \
      "$case_name" "$expected_exit" "$actual_exit" >&2
    cat "$output_file" >&2
    return 1
  fi
}

run_case allowed-lock 0
run_case outside-lock "$DETECTED_EXIT"
run_case blocked-lock "$DETECTED_EXIT"

printf '%s\n' 'OK: gitleaks lock allowlist is path- and digest-restricted'
