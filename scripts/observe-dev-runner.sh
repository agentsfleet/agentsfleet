#!/usr/bin/env bash
set -euo pipefail

observer_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=../playbooks/lib/runner/common.sh
source "$observer_root/playbooks/lib/runner/common.sh"
manifest="${ACCEPTANCE_BUILD_MANIFEST:-$observer_root/.tmp/acceptance-build.json}"
destination="${ACCEPTANCE_RUNNER_PROCESS_EVIDENCE:-$observer_root/.tmp/acceptance-runner-process.json}"
readonly record_max_bytes=65536
if [ "$(wc -c < "$manifest")" -gt "$record_max_bytes" ]; then
  echo 'Acceptance manifest exceeds its bound' >&2
  exit 1
fi
source_id="$(jq -er '.source_id | select(test("^[0-9a-f]{64}$"))' "$manifest")"
expected_hash="$(jq -er '.runner_hash | select(test("^[0-9a-f]{64}$"))' "$manifest")"
ENV=dev
runner_load_target

# Read the live executable through /proc, and obtain its identity using the
# host's registered credential. No provider or runner credential leaves the host.
record="$(timeout 60 tailscale ssh "$RUNNER_TARGET" "sudo bash -s -- '$SERVICE_NAME' '$UNIT_ENV_FILE'" <<'REMOTE'
set -euo pipefail
unit="$1"
pid="$(systemctl show "$unit" --property=MainPID --value)"
[ "$pid" -gt 0 ]
kill -0 "$pid"
binary_hash="$(sha256sum "/proc/$pid/exe")"
binary_hash="${binary_hash%% *}"
set -a
source "$2"
set +a
runner_id="$(curl --fail --silent --show-error --max-time 15 --max-filesize 65536 \
  -H "Authorization: Bearer $AGENTSFLEET_RUNNER_TOKEN" \
  "$AGENTSFLEET_API_URL/v1/runners/me" | jq -er '.id')"
unset AGENTSFLEET_RUNNER_TOKEN
[ "$(systemctl show "$unit" --property=MainPID --value)" = "$pid" ]
kill -0 "$pid"
[ "$(uname -s)" = Linux ]
case "$(uname -m)" in
  x86_64) arch=x64 ;;
  aarch64) arch=arm64 ;;
  *) echo 'Unsupported runner architecture' >&2; exit 1 ;;
esac
jq -n --arg runner_id "$runner_id" --arg binary_hash "$binary_hash" \
  --argjson pid "$pid" --arg host "$(hostname)" --arg arch "$arch" \
  --arg observed_at "$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
  '{runner_id:$runner_id,binary_hash:$binary_hash,pid:$pid,host:$host,arch:$arch,os:"linux",observed_at:$observed_at}'
REMOTE
)"
if [ "${#record}" -gt "$record_max_bytes" ]; then
  echo 'Runner observation exceeds its bound' >&2
  exit 1
fi
if [ "$(jq -er '.binary_hash' <<< "$record")" != "$expected_hash" ]; then
  echo 'Live runner executable differs from the candidate build' >&2
  exit 1
fi
mkdir -p "$(dirname "$destination")"
jq --arg source_id "$source_id" '. + {source_id:$source_id}' <<< "$record" > "$destination"
