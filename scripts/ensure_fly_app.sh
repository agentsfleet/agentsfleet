#!/usr/bin/env bash
# Bring a Fly app to a desired running-machine count, deploying it from its
# build context if it does not exist yet, and record the image digest actually
# deployed.
#
#     scripts/ensure_fly_app.sh <app> <build-context-dir> <desired-count>
#
# WHY THIS IS A SCRIPT AND NOT INLINE WORKFLOW SHELL. It was inline, copied
# into two workflows, and the two copies drifted apart within a day: one
# staged the wrong variable and the other ran the block after the deploy it
# was supposed to precede. Both passed actionlint. A parameterised script has
# one control flow to review and one place for a fix to land.
#
# `flyctl` is resolved through $FLYCTL so the self-test can inject a fake; no
# other caller sets it.
set -euo pipefail

FLYCTL="${FLYCTL:-flyctl}"
readonly FLYCTL

# The poll bound. `flyctl machine start` and `flyctl scale count` both return
# when the API accepts the request, not when anything is listening — so the
# wait is the whole point of this script, and falling through without it is
# what lets a caller deploy against an app that is still starting.
# Overridable for the same reason $FLYCTL is: the self-tests exercise the
# refusal path, which otherwise costs a real minute of sleeping per case.
POLL_ATTEMPTS="${POLL_ATTEMPTS:-12}"
POLL_SLEEP_SECONDS="${POLL_SLEEP_SECONDS:-5}"
readonly POLL_ATTEMPTS POLL_SLEEP_SECONDS

usage() {
  printf 'usage: %s <app> <build-context-dir> <desired-count>\n' "${0##*/}" >&2
}

main() {
  if [ "$#" -ne 3 ]; then
    usage
    return 2
  fi

  local app="$1" context_dir="$2" desired="$3"

  case "$desired" in
    ''|*[!0-9]*)
      printf 'desired-count must be a non-negative integer, got: %s\n' "$desired" >&2
      return 2
      ;;
  esac
  if [ "$desired" -lt 1 ]; then
    printf 'desired-count must be at least 1, got: %s\n' "$desired" >&2
    return 2
  fi

  local machines total
  machines="$("$FLYCTL" machine list --app "$app" --json 2>/dev/null || echo '[]')"
  total="$(printf '%s' "$machines" | jq 'length')"

  if [ "$total" -eq 0 ]; then
    printf 'no machines for %s — deploying from %s\n' "$app" "$context_dir"
    # Positional path is the BUILD CONTEXT. Without it flyctl uses the working
    # directory and a Dockerfile `COPY config.yml` cannot resolve.
    "$FLYCTL" deploy "$context_dir" --app "$app" --wait-timeout 60
  fi

  "$FLYCTL" scale count "$desired" --app "$app" --yes

  local attempt running
  running=0
  for attempt in $(seq 1 "$POLL_ATTEMPTS"); do
    machines="$("$FLYCTL" machine list --app "$app" --json 2>/dev/null || echo '[]')"
    total="$(printf '%s' "$machines" | jq 'length')"
    running="$(printf '%s' "$machines" | jq '[.[] | select(.state == "started")] | length')"
    printf '%s (attempt %s/%s): %s/%s running, want %s\n' \
      "$app" "$attempt" "$POLL_ATTEMPTS" "$running" "$total" "$desired"
    [ "$running" -ge "$desired" ] && break
    sleep "$POLL_SLEEP_SECONDS"
  done

  if [ "$running" -lt "$desired" ]; then
    printf '%s never reached %s running machines — refusing to report success\n' \
      "$app" "$desired" >&2
    return 1
  fi

  # The image is pulled by tag, so the tag alone does not say what ran. Print
  # the resolved digest into the deploy log: it is the only record of which
  # collector build a given rollout actually used.
  printf 'deployed image: '
  "$FLYCTL" image show --app "$app" --json 2>/dev/null \
    | jq -r '.[0].Digest // .Digest // "unknown"' 2>/dev/null \
    || printf 'unknown\n'
}

main "$@"
