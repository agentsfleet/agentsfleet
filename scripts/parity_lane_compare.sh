#!/usr/bin/env bash
# Response normalisation and route-absence discrimination for the parity lane.
#
# Sourced by `parity_lane.sh`, never run on its own: every function here reads a
# constant that file declares, and the split is along what each half DOES. That
# file decides which routes to probe and what to report; this one decides when
# two responses are the same response — which is where the lane's subtlety
# lives, and where three separate bugs have already been fixed.
#
# Sourced AFTER the volatile-field constants and `probe`, because these read
# them at call time and bash resolves neither until then.

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

# The status an unrouted path answers. A 404 is NOT by itself proof of that,
# and reading it as such is a bug this lane shipped: the premise was "auth
# refuses before a handler resolves an identifier, so a 404 means the path is
# not routed". That holds for a guarded route and fails for an open one, which
# has no auth to refuse — its handler runs, looks up the placeholder segment,
# finds nothing, and answers a correct 404. Three mounted open routes were
# reported as missing before the shape check below was added.
#
# What separates them is the SHAPE. An unmatched path is answered by the
# router, identically for every path it does not have; a handler that ran and
# found nothing answers with its own envelope. So route-absence is decided by
# comparing against `absence_shape`, never by the status alone.
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
