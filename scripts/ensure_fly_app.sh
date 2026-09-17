#!/usr/bin/env bash
# Deploy a Fly app from its build context, bring it to a desired count of
# machines whose health checks are PASSING, and record the image digest
# actually deployed.
#
#     scripts/ensure_fly_app.sh <app> <build-context-dir> <desired-count> [config]
#     scripts/ensure_fly_app.sh --create-only <app>
#
# The optional fourth argument is a path to the app's fly.toml, for an app
# whose Dockerfile has to reach OUTSIDE its own directory. dragonfly-dev
# copies scripts/dragonfly-cluster.sh, the same file the integration lane
# runs, so its build context is the repository root while its configuration
# stays in deploy/fly/dragonfly-dev/. Without it flyctl looks for fly.toml in
# the context directory and would find the repository root's, which is not
# one. Omitted, behaviour is exactly what it was.
#
# CREATE-ONLY EXISTS BECAUSE OF AN ORDERING CONSTRAINT, not for symmetry. A
# fresh app must exist before `flyctl secrets set --app` addresses it, and it
# must NOT be deployed until after — a collector that boots without its
# upstream credentials fails its health check, and this script then refuses,
# correctly, for a reason that is nobody's bug. So a caller priming a new
# environment creates, stages secrets, then ensures.
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

# The organisation new apps are created in. NO DEFAULT, deliberately.
# Development and production live in separate Fly organisations
# (`agentsfleet-dev` and `agentsfleet-prod`), so there is no single value that
# is right for both callers, and a default would be silently wrong for one of
# them — creating a production app inside the development organisation is not
# an error Fly reports, it is an error somebody finds later.
#
# An earlier revision of this file defaulted to `agentsfleet`, an organisation
# that has never existed. Refusing beats guessing: the caller knows which
# environment it is deploying and can say so.
FLY_ORG="${FLY_ORG:-}"
readonly FLY_ORG

usage() {
  printf 'usage: %s <app> <build-context-dir> <desired-count> [config]\n' "${0##*/}" >&2
  printf '       %s --create-only <app>\n' "${0##*/}" >&2
}

# Create the app when it is absent, and say which of the two it was. The deploy
# workflows address an app they never created; the priming playbook creates
# apps a later-added service does not appear in. Between those two habits an
# app can be referenced everywhere and exist nowhere, which is what took the
# development deploy down at `flyctl secrets set --app` — the first command to
# address it. Creating here closes that gap without depending on a human
# having read a playbook.
ensure_app_exists() {
  local app="$1"
  if "$FLYCTL" status --app "$app" >/dev/null 2>&1; then
    printf '%s already exists\n' "$app"
    return 0
  fi
  if [ -z "$FLY_ORG" ]; then
    # Only the create path needs the org, so this refuses HERE rather than at
    # the top of the script: an existing app deploys fine without it, and
    # demanding it up front would break every caller that never creates.
    printf '%s does not exist and FLY_ORG is unset — refusing to guess which organisation to create it in\n' \
      "$app" >&2
    return 1
  fi
  printf '%s does not exist — creating it in %s\n' "$app" "$FLY_ORG"
  if ! "$FLYCTL" apps create "$app" --org "$FLY_ORG"; then
    printf 'could not create %s — refusing to continue\n' "$app" >&2
    return 1
  fi
}

main() {
  if [ "${1:-}" = "--create-only" ]; then
    if [ "$#" -ne 2 ]; then
      usage
      return 2
    fi
    ensure_app_exists "$2"
    return
  fi

  if [ "$#" -lt 3 ] || [ "$#" -gt 4 ]; then
    usage
    return 2
  fi

  local app="$1" context_dir="$2" desired="$3" config="${4:-}"

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
  ensure_app_exists "$app"

  # An array, not a string: an unquoted empty string would reach flyctl as an
  # empty argument, and flyctl reads that as a positional it does not want.
  local config_flag=()
  if [ -n "$config" ]; then
    if [ ! -f "$config" ]; then
      printf 'config %s does not exist — refusing to deploy %s against a file that is not there\n' \
        "$config" "$app" >&2
      return 1
    fi
    config_flag=(--config "$config")
    # An app whose context is the repository root needs its own ignore file,
    # and it lives beside its fly.toml rather than arriving as a fifth
    # argument: a caller passing both paths separately will eventually pass a
    # mismatched pair. flyctl otherwise reads the CONTEXT's .dockerignore,
    # which for the repository root is the daemon image's and does not exclude
    # rustd/target -- 12GB uploaded to the builder on every deploy.
    # A config path with no slash leaves ${config%/*} equal to the filename,
    # which would derive `fly.toml/.dockerignore` and silently skip an ignore
    # file the caller meant to apply. Resolve the directory properly.
    local config_dir
    config_dir="$(dirname "$config")"
    local ignorefile="$config_dir/.dockerignore"
    if [ -f "$ignorefile" ]; then
      config_flag+=(--ignorefile "$ignorefile")
      printf 'using ignore file %s\n' "$ignorefile"
    fi
  fi

  printf 'deploying %s from %s%s\n' "$app" "$context_dir" "${config:+ using $config}"
  "$FLYCTL" deploy "$context_dir" --app "$app" --wait-timeout 60 "${config_flag[@]}"

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
