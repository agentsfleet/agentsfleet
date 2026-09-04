#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../../../.." && pwd)"

for asset in "$SCRIPT_DIR/assets/dashboard.json" "$SCRIPT_DIR/assets/alerts.json"; do
  jq -e . "$asset" >/dev/null
done

jq -e '
  (.panels | length) >= 8 and
  ([.panels[].id] | length == (unique | length)) and
  ([.panels[].targets[].expr] | all(contains("agentsfleet_"))) and
  ([.panels[].datasource.uid] | all(. == "__PROMETHEUS_UID__"))
' "$SCRIPT_DIR/assets/dashboard.json" >/dev/null

jq -e '
  length == 6 and
  ([.[].name] | length == (unique | length)) and
  ([.[].expr] | all(contains("agentsfleet_"))) and
  ([.[] | select(.name == "runner-silent")] | length == 1)
' "$SCRIPT_DIR/assets/alerts.json" >/dev/null

metrics="$(
  jq -r '.. | strings' \
    "$SCRIPT_DIR/assets/dashboard.json" \
    "$SCRIPT_DIR/assets/alerts.json" |
    awk '{
      text = $0
      while (match(text, /agentsfleet_[a-z0-9_]+/)) {
        print substr(text, RSTART, RLENGTH)
        text = substr(text, RSTART + RLENGTH)
      }
    }' |
    sort -u
)"
while IFS= read -r metric; do
  [ -n "$metric" ] || continue
  if ! grep -RFq -- "$metric" \
    "$REPO_ROOT/rustd/crates"; then
    echo "ERROR: Grafana asset references an unowned metric: $metric" >&2
    exit 1
  fi
done <<<"$metrics"

echo "PASS: Grafana assets are valid and reference source-owned metrics"
