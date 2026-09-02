#!/usr/bin/env bash
# Deploy a Fly app from its build context, bring it to a desired count of
# machines whose health checks are PASSING, and record the image digest
# actually deployed.
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

  # EVERY run deploys. This used to deploy only when the app had no machines,
  # which quietly made this milestone's central claim false: `config.yml` is
  # baked into the image by the Dockerfile's `COPY`, so on every run after the
  # first, a changed receiver, authentication policy or exporter pipeline was
  # built and never shipped. Choosing a backend is supposed to be a collector
  # configuration change; a configuration change that never deploys is not one.
  #
  # Positional path is the BUILD CONTEXT. Without it flyctl uses the working
  # directory and a Dockerfile `COPY config.yml` cannot resolve.
  printf 'deploying %s from %s\n' "$app" "$context_dir"
  "$FLYCTL" deploy "$context_dir" --app "$app" --wait-timeout 60

  "$FLYCTL" scale count "$desired" --app "$app" --yes

  # Readiness is the health check passing, NOT the machine state. Fly reports
  # `started` when the VM is running, which happens before the collector inside
  # it binds 4318 — so a caller gated on `started` can point a daemon at a
  # receiver that is not listening yet and lose the export. `fly.toml` already
  # declares [checks.health] against the collector's own health_check extension
  # on 13133; this reads the verdict it was already producing.
  #
  # A machine with NO checks counts as not ready, deliberately. It means
  # readiness cannot be proven from here, and this script's entire contract is
  # refusing to report a success it cannot prove.
  local attempt machines total started ready
  started=0
  ready=0
  for attempt in $(seq 1 "$POLL_ATTEMPTS"); do
    machines="$("$FLYCTL" machine list --app "$app" --json 2>/dev/null || echo '[]')"
    total="$(printf '%s' "$machines" | jq 'length')"
    started="$(printf '%s' "$machines" | jq '[.[] | select(.state == "started")] | length')"
    ready="$(printf '%s' "$machines" | jq '
      [ .[]
        | select(.state == "started")
        | select((.checks // []) as $c
                 | ($c | length) > 0 and ($c | all(.status == "passing")))
      ] | length')"
    printf '%s (attempt %s/%s): %s/%s started, %s health-passing, want %s\n' \
      "$app" "$attempt" "$POLL_ATTEMPTS" "$started" "$total" "$ready" "$desired"
    [ "$ready" -ge "$desired" ] && break
    sleep "$POLL_SLEEP_SECONDS"
  done

  if [ "$ready" -lt "$desired" ]; then
    # Name which half failed. "Not running" and "running but never healthy" are
    # different incidents with different first moves, and an operator reading a
    # deploy log at 3am should not have to guess which one this was.
    if [ "$started" -lt "$desired" ]; then
      printf '%s never reached %s running machines (%s started) — refusing to report success\n' \
        "$app" "$desired" "$started" >&2
    else
      printf '%s reached %s running machines but only %s passed health checks — refusing to report success\n' \
        "$app" "$started" "$ready" >&2
    fi
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
