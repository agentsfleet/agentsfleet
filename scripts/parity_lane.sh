#!/usr/bin/env bash
# Black-box HTTP contract parity harness. Run via `make test-parity`, or directly:
#
#     BASE_URL=http://127.0.0.1:3000 bash scripts/parity_lane.sh
#     BASE_URL=http://127.0.0.1:3000 COMPARE_URL=http://127.0.0.1:8080 \
#       bash scripts/parity_lane.sh
#
# One base URL is RECORD mode: every route answers, and every refusal carries
# the problem+json envelope. Two base URLs is COMPARE mode: the same roster is
# probed against both and any difference in status, contract header or
# normalised body fails naming the route and the method.
#
# Tests covered:
#   * test_parity_lane_detects_difference — a seeded status, header or body
#     difference fails naming route and method; identical daemons diff empty
#
# WHY THIS IS SHELL. The harness is verification scaffolding, not product. A
# Rust crate would join `rustd/crates/` under the 100%-line coverage flag and
# pay that rent for the life of the repository, for code that exists to point
# curl at two daemons.
#
# WHAT IS PROBED, AND WHY IT NEEDS NO CREDENTIALS. Every request goes without
# credentials and without a body. What that grades is the contract at the EDGE:
# which routes exist, what an unauthenticated caller is told, and in what
# envelope. Two daemons that disagree there disagree about their route table or
# their middleware order, which is exactly the class of drift a cutover
# introduces. Authenticated behavioural parity needs the full route surface and
# is deliberately not here.
#
# A daemon that mutates state on an unauthenticated, bodyless request has a
# security defect; this harness does not tiptoe around one.
#
# THE ROSTER IS REFLECTION, NOT A LIST. Routes come from `public/openapi.json`,
# so a new route joins this lane the moment it joins the contract. A hand-kept
# list is one somebody forgets to update, and the forgotten route is the drift
# the lane exists to catch.
#
# PARITY_OPENAPI, PARITY_PROBE and the timeout are overridable so the
# self-tests can drive the harness against fixtures. Nothing else sets them.
#
# Exits 0 on success, 1 on the first failing assertion (with diagnostic).

set -euo pipefail

# Resolved from BASH_SOURCE, not `$PWD`: the lane runs from the repository root
# but the self-tests invoke it by absolute path.
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
readonly SCRIPT_DIR REPO_ROOT

PARITY_OPENAPI="${PARITY_OPENAPI:-$REPO_ROOT/public/openapi.json}"
PARITY_TIMEOUT_SEC="${PARITY_TIMEOUT_SEC:-10}"
# Set by the self-tests to a fixture responder taking <base> <method> <path>.
# Unset in every real run, where `probe_via_curl` is the responder.
PARITY_PROBE="${PARITY_PROBE:-}"
# The declared-divergence register. A difference listed there is intended; a
# difference not listed there is a regression. Without this the lane would fail
# forever on a route the daemon deliberately does not serve, and the only fixes
# available would be to un-declare the decision or to stop running the lane.
PARITY_REGISTER="${PARITY_REGISTER:-$REPO_ROOT/playbooks/operations/cutover/001_playbook.md}"
readonly PARITY_OPENAPI PARITY_TIMEOUT_SEC PARITY_PROBE PARITY_REGISTER

# The value substituted for every `{path_param}` segment. One constant rather
# than a per-parameter table: an unauthenticated probe is refused before a
# handler ever parses it, so the only property that matters is that it is a
# syntactically valid, non-empty segment which cannot collide with a real
# resource on a live deployment.
readonly PATH_PARAM_PLACEHOLDER="00000000-0000-0000-0000-0000000par1ty"

# The status printed when the connection itself failed. curl reports 000 for a
# refused or timed-out request, and the distinction between "answered 502" and
# "never answered" is one this lane must not lose.
readonly NO_ANSWER_STATUS="000"

# Headers that differ between two identical daemons on every single request.
# Comparing them would fail every route and grade nothing. `content-length` is
# here because it is derived from a body whose volatile fields are normalised
# below — two equal normalised bodies can carry different lengths on the wire.
readonly VOLATILE_HEADERS=(
  age alt-svc cf-ray connection content-length date etag keep-alive
  request-id server via x-envoy-upstream-service-time x-ratelimit-remaining
  x-ratelimit-reset x-request-id
)

# Body fields minted per request. Normalised to a constant rather than deleted,
# so a daemon that stops emitting one is still a difference.
readonly VOLATILE_BODY_FIELDS=(request_id trace_id instance)
readonly VOLATILE_BODY_REPLACEMENT="<normalised>"

FAIL=0
err() { printf "FAIL: %s\n" "$*" >&2; FAIL=1; }
ok()  { printf "OK:   %s\n" "$*"; }

require_tools() {
  local tool
  for tool in curl jq; do
    command -v "$tool" >/dev/null 2>&1 || {
      printf "FAIL: %s is required by the parity lane and is not on PATH\n" "$tool" >&2
      exit 1
    }
  done
}

# METHOD<TAB>PATH_TEMPLATE per line, sorted, from the OpenAPI document.
# `paths_to_probe` reads the contract; nothing here knows a route by name.
roster() {
  jq -r '
    .paths
    | to_entries[]
    | .key as $path
    | .value
    | to_entries[]
    | select(.key | IN("get","post","put","patch","delete","head"))
    | "\(.key | ascii_upcase)\t\($path)"
  ' "$PARITY_OPENAPI" | sort
}

# `METHOD /path` per line, read out of the register's table rows. The register
# is prose a human maintains, so the machine-readable part is deliberately the
# smallest thing that can be both: a backticked `GET /metrics` inside the row
# that explains it. A divergence with no explanation cannot be expressed.
declared_divergences() {
  [ -f "$PARITY_REGISTER" ] || return 0
  sed -n '/^| *D[0-9]/p' "$PARITY_REGISTER" \
    | grep -oE "\`(GET|POST|PUT|PATCH|DELETE|HEAD) [^\`]+\`" \
    | tr -d '`' | sort -u
}

# `/v1/workspaces/{workspace_id}/fleets` → `/v1/workspaces/<placeholder>/fleets`
concrete_path() {
  printf '%s\n' "$1" | sed "s|{[^}]*}|$PATH_PARAM_PLACEHOLDER|g"
}

WORK_DIR="$(mktemp -d)"
readonly WORK_DIR
trap 'rm -rf "$WORK_DIR"' EXIT

# One probe, printed in an HTTP-shaped envelope every responder agrees on:
# the status on line one, the response headers, a blank line, then the body.
# The self-tests substitute their own responder through PARITY_PROBE and emit
# the same three parts, so the parsing and the diff below are the SAME code in
# a fixture run as in a live one.
probe_via_curl() {
  local base="$1" method="$2" path="$3"
  local headers="$WORK_DIR/probe.headers" body="$WORK_DIR/probe.body" status
  # `|| true` deliberately: a refused connection is an OUTCOME this lane
  # reports per route, not a reason to abandon the whole roster. curl still
  # writes 000 to stdout, which `NO_ANSWER_STATUS` is compared against.
  status="$(curl --silent --show-error --max-time "$PARITY_TIMEOUT_SEC" \
    --request "$method" --dump-header "$headers" --output "$body" \
    --write-out '%{http_code}' "$base$path" 2>/dev/null || true)"
  printf '%s\n' "${status:-$NO_ANSWER_STATUS}"
  # curl's dump includes the status line and CRLF endings; neither is a header.
  sed -e 's/\r$//' -e '/^HTTP\//d' -e '/^$/d' "$headers" 2>/dev/null || true
  printf '\n'
  cat "$body" 2>/dev/null || true
}

probe() {
  if [ -n "$PARITY_PROBE" ]; then
    "$PARITY_PROBE" "$1" "$2" "$3"
  else
    probe_via_curl "$1" "$2" "$3"
  fi
}

# Header names lowercased, volatile ones dropped, the rest sorted — so two
# daemons that emit the same contract in a different order compare equal.
normalize_headers() {
  local drop_list
  drop_list="$(printf '%s,' "${VOLATILE_HEADERS[@]}")"
  awk -v drop="$drop_list" '
    BEGIN {
      n = split(drop, names, ",")
      for (i = 1; i <= n; i++) if (names[i] != "") volatile[names[i]] = 1
    }
    /^[^:]+:/ {
      name = tolower(substr($0, 1, index($0, ":") - 1))
      value = substr($0, index($0, ":") + 1)
      gsub(/^[ \t]+|[ \t]+$/, "", value)
      if (!(name in volatile)) print name ": " value
    }
  ' | sort
}

# Volatile body fields replaced with a constant and keys sorted, so two equal
# refusals compare equal. Replaced rather than deleted: a daemon that stops
# emitting `request_id` is a difference this lane should still report.
# Anything that is not JSON passes through as bytes.
normalize_body() {
  local fields
  fields="$(printf '%s\n' "${VOLATILE_BODY_FIELDS[@]}" | jq -R . | jq -sc .)"
  local raw
  raw="$(cat)"
  printf '%s' "$raw" | jq -S --argjson fields "$fields" \
    --arg replacement "$VOLATILE_BODY_REPLACEMENT" '
      walk(
        if type == "object"
        then with_entries(if (.key | IN($fields[])) then .value = $replacement else . end)
        else . end
      )
    ' 2>/dev/null || printf '%s' "$raw"
}

# One route×method against one base URL, rendered as the canonical text the
# diff compares. Status first so a status difference is the first line to
# differ, which is what makes the report readable.
snapshot() {
  local base="$1" method="$2" path="$3"
  local raw status
  raw="$(probe "$base" "$method" "$path")"
  status="$(printf '%s\n' "$raw" | head -n 1)"
  printf 'status: %s\n' "$status"
  printf '%s\n' "$raw" | sed -n '2,/^$/p' | normalize_headers | sed 's/^/header: /'
  printf 'body:\n'
  printf '%s\n' "$raw" | sed -n '/^$/,$p' | tail -n +2 | normalize_body
  printf '\n'
}

# The status a mounted route can never answer to a bodyless, credential-less
# probe. Auth refuses before a handler resolves an identifier, so a 404 here
# means the path is not routed at all — which is precisely the drift a port
# introduces and the one thing a black-box probe can prove without credentials.
readonly ROUTE_ABSENT_STATUS="404"

# A path no contract can declare, probed to learn what a daemon says about a
# route it does not have. The placeholder segment is the one already used for
# path parameters, so this cannot collide with a real resource either.
readonly ABSENCE_PROBE_PATH="/v1/parity-lane-absence-probe/$PATH_PARAM_PLACEHOLDER"

# What THIS daemon's router answers for a route it does not serve, minus the
# status line. Learned per run rather than hard-coded: the shape is the
# framework's, and asserting a remembered one would fail the day it changes for
# a reason that is not drift.
absence_shape() {
  snapshot "$1" "$2" "$ABSENCE_PROBE_PATH" | sed '1d'
}

# The register permits exactly ONE difference: that a daemon does not serve the
# route. This predicate has now been wrong in three directions, and the next
# reader needs the map more than the code.
#
#   Trap 1 — too loose. `continue` past a declared route accepted EVERY
#   difference on it, making the register a mute button.
#   Trap 2 — still too loose. Deciding on status codes alone let `000` (never
#   answered) pass as an absence, because it differed from a 404.
#   Trap 3 — too STRICT, and the one the fixtures hid. Requiring the two
#   snapshots to match apart from the status line cannot hold on real daemons:
#   a served response and a route-absence differ in every header and byte of
#   body by construction, so the declared divergence failed the lane it was
#   written to permit. The self-test fixtures passed because they answered the
#   same headers and body whatever the status, which no daemon does.
#
# So the absent side is graded against its OWN daemon's unmatched-route
# response, not against the other daemon. That still refuses an arbitrary 404 a
# handler authored, which is the discrimination the register needs, and it does
# not demand equality between two responses that were never comparable.
#
# `000` is never an absence: a daemon that did not answer has told us nothing,
# and the register cannot declare silence.
#
# The serving side carries no assertion here because there is nothing to assert
# it against — the whole premise is that the other daemon does not serve it.
# RECORD mode against that daemon is what grades it, and the cutover runs both.
declared_absence_only() {
  local status_a="$1" status_b="$2" snap_a="$3" snap_b="$4"
  local base_a="$5" base_b="$6" method="$7"
  local absent_snap absent_base
  [ "$status_a" != "$NO_ANSWER_STATUS" ] && [ "$status_b" != "$NO_ANSWER_STATUS" ] || return 1
  if [ "$status_a" = "$ROUTE_ABSENT_STATUS" ] && [ "$status_b" != "$ROUTE_ABSENT_STATUS" ]; then
    absent_snap="$snap_a"; absent_base="$base_a"
  elif [ "$status_b" = "$ROUTE_ABSENT_STATUS" ] && [ "$status_a" != "$ROUTE_ABSENT_STATUS" ]; then
    absent_snap="$snap_b"; absent_base="$base_b"
  else
    return 1
  fi
  cmp -s <(sed '1d' "$absent_snap") <(absence_shape "$absent_base" "$method")
}

# RECORD mode — one daemon, no second to compare against.
#
# Two claims, both provable black-box with no credentials: every route the
# contract declares answers, and none of them answers `ROUTE_ABSENT_STATUS`.
# Together they say the route table the image serves is the route table the
# contract describes. What an authenticated caller then gets needs credentials
# and the full route surface, and is graded by the cutover milestone, not here.
record_mode() {
  local base="$1" probed=0 declared_count=0 method path concrete status declared
  declared="$(declared_divergences)"
  while IFS=$'\t' read -r method path; do
    [ -n "$method" ] || continue
    concrete="$(concrete_path "$path")"
    status="$(snapshot "$base" "$method" "$concrete" | sed -n 's/^status: //p')"
    probed=$((probed + 1))
    if [ "$status" = "$NO_ANSWER_STATUS" ]; then
      err "$method $path — no answer from $base (refused or timed out)"
    elif [ "$status" = "$ROUTE_ABSENT_STATUS" ]; then
      if printf '%s\n' "$declared" | grep -qxF "$method $path"; then
        declared_count=$((declared_count + 1))
        printf '  declared: %s %s is not served, per the divergence register\n' "$method" "$path"
      else
        err "$method $path — answered $ROUTE_ABSENT_STATUS, so the route is not mounted"
      fi
    fi
  done < <(roster)
  [ "$declared_count" -eq 0 ] || ok "$declared_count declared divergence(s) honoured"
  guard_probed "$probed"
  [ "$FAIL" -eq 0 ] && ok "$probed route/method pairs answered from $base"
  return "$FAIL"
}

# COMPARE mode — the same roster against both, diffed per route×method.
compare_mode() {
  local base_a="$1" base_b="$2" probed=0 differed=0 declared_count=0
  local method path concrete declared
  local snap_a="$WORK_DIR/a.snapshot" snap_b="$WORK_DIR/b.snapshot"
  # The register binds BOTH modes. RECORD reads it to allow a declared 404;
  # COMPARE has to read it for the same reason one level up — a route the
  # register says one daemon deliberately does not serve will differ from one
  # that does, and calling that difference a regression is the register
  # failing to mean anything in the mode that diffs two daemons.
  declared="$(declared_divergences)"
  local is_declared status_a status_b
  while IFS=$'\t' read -r method path; do
    [ -n "$method" ] || continue
    is_declared=0
    printf '%s\n' "$declared" | grep -qxF "$method $path" && is_declared=1
    concrete="$(concrete_path "$path")"
    snapshot "$base_a" "$method" "$concrete" >"$snap_a"
    snapshot "$base_b" "$method" "$concrete" >"$snap_b"
    probed=$((probed + 1))
    diff -u "$snap_a" "$snap_b" >"$WORK_DIR/delta" 2>&1 && continue

    # A declared route is still probed. The predicate rejects a no-answer
    # response and grades the absent side against its own daemon's
    # unmatched-route shape.
    status_a="$(sed -n 's/^status: //p' "$snap_a")"
    status_b="$(sed -n 's/^status: //p' "$snap_b")"
    if [ "$is_declared" -eq 1 ] \
      && declared_absence_only "$status_a" "$status_b" "$snap_a" "$snap_b" \
        "$base_a" "$base_b" "$method"; then
      declared_count=$((declared_count + 1))
      printf '  declared: %s %s — %s on one side, %s on the other, per the divergence register\n' \
        "$method" "$path" "$status_a" "$status_b"
      continue
    fi

    differed=$((differed + 1))
    err "$method $path — $base_a and $base_b disagree"
    sed -n '3,$p' "$WORK_DIR/delta" | sed 's/^/      /' >&2
  done < <(roster)
  [ "$declared_count" -eq 0 ] || ok "$declared_count declared divergence(s) honoured"
  guard_probed "$probed"
  [ "$differed" -eq 0 ] && [ "$FAIL" -eq 0 ] \
    && ok "$probed route/method pairs identical across $base_a and $base_b"
  return "$FAIL"
}

# The lane guard, in the shape `make/test-integration-rustd.mk` states it: a
# run that graded nothing exits non-zero. An empty roster and a green run are
# indistinguishable by exit status alone, and the empty one is a lie.
guard_probed() {
  [ "$1" -gt 0 ] || err "the roster from $PARITY_OPENAPI was empty — a lane that probed nothing is not a pass"
}

main() {
  local base="${BASE_URL:-}" compare="${COMPARE_URL:-}"
  if [ -z "$base" ]; then
    printf "FAIL: BASE_URL is unset. Usage: BASE_URL=<url> [COMPARE_URL=<url>] %s\n" \
      "bash scripts/parity_lane.sh" >&2
    exit 1
  fi
  require_tools
  [ -f "$PARITY_OPENAPI" ] || { printf "FAIL: no contract at %s\n" "$PARITY_OPENAPI" >&2; exit 1; }
  if [ -n "$compare" ]; then
    compare_mode "${base%/}" "${compare%/}"
  else
    record_mode "${base%/}"
  fi
  exit "$FAIL"
}

main "$@"
