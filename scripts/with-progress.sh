#!/usr/bin/env bash
# Wrap a long-running command so a developer can tell "still working" from
# "wedged".
#
# The problem it solves is specific. `cargo clippy` and `cargo test` spend
# minutes in a compile phase that writes NOTHING to the terminal, and the lint
# and integration lanes are mostly made of those. A stage line printed before
# the command and a tick line printed after it look identical whether the
# command is running or hung.
#
# So the heartbeat here is quiet-triggered, not interval-triggered: output
# streams through untouched, and a tick is printed only once the command has
# written nothing for QUIET_SECONDS. A chatty command never prints one; a
# silent one prints elapsed time until it speaks again. That makes the tick
# mean something — it fires exactly in the window where a developer starts
# wondering.
#
# Usage:  scripts/with-progress.sh "label" -- command [args...]
# Env:    WITH_PROGRESS_QUIET_SECONDS  (default 15) seconds of silence per tick
#         WITH_PROGRESS_DISABLE=1      pass the command straight through
#
# Progress lines go to STDERR, never stdout. The Rust integration runner merges
# both streams while it owns the child and parses Cargo's summaries for test
# counts. Keeping the heartbeat separate here means the wrapper reports only
# the command's evidence; the runner may still choose to retain the combined
# diagnostic stream.
set -euo pipefail

readonly DEFAULT_QUIET_SECONDS=15

usage() {
  echo "usage: $0 <label> -- <command> [args...]" >&2
  exit 2
}

[ "$#" -ge 3 ] || usage
label="$1"
shift
[ "$1" = "--" ] || usage
shift

quiet_seconds="${WITH_PROGRESS_QUIET_SECONDS:-$DEFAULT_QUIET_SECONDS}"

# Seconds as `12s` under a minute and `2m04s` above it — a bare second count
# stops being readable at exactly the durations this wrapper exists for.
fmt_elapsed() {
  local secs="$1"
  if [ "$secs" -lt 60 ]; then
    printf '%ds' "$secs"
  else
    printf '%dm%02ds' "$((secs / 60))" "$((secs % 60))"
  fi
}

if [ "${WITH_PROGRESS_DISABLE:-0}" = "1" ]; then
  exec "$@"
fi

# `mktemp -d` and one trap covering every exit path, including the signals a
# Ctrl-C in make delivers: the ticker is a background child, so leaving it
# behind would keep printing into a shell that has moved on.
work_dir="$(mktemp -d)"
ticker_pid=""

cleanup() {
  if [ -n "$ticker_pid" ]; then
    kill "$ticker_pid" 2>/dev/null || true
    wait "$ticker_pid" 2>/dev/null || true
  fi
  rm -rf "$work_dir"
}
trap cleanup EXIT INT TERM

stamp_file="$work_dir/last-output"
stamp_tmp="$work_dir/last-output.tmp"

# Bash's own `SECONDS`, not `date`: the reader below touches this clock once per
# output LINE, and cargo emits thousands of them. `SECONDS` is a shell variable
# — no fork — and it is in bash 3.2, which is the floor on a Mac without
# homebrew bash. Both subshells inherit this origin, so their readings share it.
SECONDS=0
printf '%s\n' 0 > "$stamp_file"

echo "→ $label" >&2

# The ticker reads the stamp the output loop refreshes. It never writes the
# stamp on a tick — it tracks its own last-tick second instead, so a tick
# cannot reset the command's quiet clock and mask a real stall.
(
  last_tick=0
  while true; do
    sleep 1
    last_output="$(cat "$stamp_file" 2>/dev/null || true)"
    # A torn read — the file mid-replace, or empty on the first pass — must not
    # reach the arithmetic below, where an empty string is a syntax error and a
    # partial one is a nonsense duration.
    case "$last_output" in
      '' | *[!0-9]*) continue ;;
    esac
    silent_for="$(( SECONDS - last_output ))"
    if [ "$silent_for" -ge "$quiet_seconds" ] && [ "$(( SECONDS - last_tick ))" -ge "$quiet_seconds" ]; then
      printf '   … %s — %s elapsed, quiet for %s\n' \
        "$label" "$(fmt_elapsed "$SECONDS")" "$(fmt_elapsed "$silent_for")" >&2
      last_tick="$SECONDS"
    fi
  done
) &
ticker_pid="$!"

# `pipefail` plus PIPESTATUS: the `while read` is the last stage, so `$?` alone
# would report the loop's status and call every failing command a success.
#
# The stamp is written at most once per second (`last_written` guard) and
# through a temp-then-rename, so the ticker either sees the old value or the new
# one and never a half-written file.
set +e
"$@" 2>&1 | {
  last_written=-1
  while IFS= read -r line; do
    printf '%s\n' "$line"
    if [ "$SECONDS" -ne "$last_written" ]; then
      printf '%s\n' "$SECONDS" > "$stamp_tmp" && mv -f "$stamp_tmp" "$stamp_file"
      last_written="$SECONDS"
    fi
  done
}
status="${PIPESTATUS[0]}"
set -e

elapsed="$(fmt_elapsed "$SECONDS")"
if [ "$status" -eq 0 ]; then
  echo "✓ $label ($elapsed)" >&2
else
  echo "✗ $label failed after $elapsed (exit $status)" >&2
fi
exit "$status"
