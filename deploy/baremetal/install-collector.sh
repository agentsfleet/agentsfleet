#!/usr/bin/env bash
# Run on the runner host only after M233_002 D1 chooses the collector key.
# The key file is staged separately from Indy's vault; never pass its value here.
set -euo pipefail

COLLECTOR_VERSION=0.164.0

collector_archive_sha() {
  case "$1" in
    amd64) printf '%s\n' f0c1ce9f661399beb4a58b711924716e08ea70c3802db0715fcfde3cbbada2d4 ;;
    arm64) printf '%s\n' 8bd4a7f443405522a2175cc1a8675f9e99cc047c7181b73461efd08efbba38d3 ;;
    *) echo 'ERROR: collector supports amd64 and arm64 only' >&2; return 1 ;;
  esac
}

collector_check_credentials() {
  local file="$1" mode
  [ -f "$file" ] && [ ! -L "$file" ] || {
    echo 'ERROR: collector credentials must be a regular root-owned file' >&2; return 1;
  }
  mode="$(stat -c '%u:%a' "$file")"
  case "$mode" in
    0:600 | 0:400) ;;
    *) echo 'ERROR: collector credential file must be root-owned mode 0600 or 0400' >&2; return 1 ;;
  esac
}

install_collector() {
  local credentials="$1" arch dir work archive expected
  [ "$(id -u)" -eq 0 ] || { echo 'ERROR: collector install requires root' >&2; return 1; }
  collector_check_credentials "$credentials"
  case "$(uname -m)" in
    x86_64) arch=amd64 ;;
    aarch64) arch=arm64 ;;
    *) echo 'ERROR: unsupported collector architecture' >&2; return 1 ;;
  esac
  dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
  expected="$(collector_archive_sha "$arch")"
  work="$(mktemp -d)"
  trap 'rm -rf "$work"' RETURN
  archive="$work/collector.tar.gz"
  curl --fail --silent --show-error --location --max-time 180 \
    "https://github.com/open-telemetry/opentelemetry-collector-releases/releases/download/v$COLLECTOR_VERSION/otelcol-contrib_${COLLECTOR_VERSION}_linux_${arch}.tar.gz" \
    --output "$archive"
  printf '%s  %s\n' "$expected" "$archive" | sha256sum --check --status
  tar -xzf "$archive" -C "$work" otelcol-contrib
  # Parse both assets before making a receiver reachable.
  nft --check --file "$dir/otelcol-runner.nft"
  # systemd reads the root-only environment file; the dedicated service user
  # receives the selected key without granting it access to the vault itself.
  id otelcol-runner >/dev/null 2>&1 || \
    useradd --system --no-create-home --shell /usr/sbin/nologin otelcol-runner
  install -d -m 0755 /etc/agentsfleet
  install -m 0755 "$work/otelcol-contrib" /usr/local/bin/otelcol-contrib
  install -m 0644 "$dir/otelcol-runner.yml" /etc/agentsfleet/otelcol-runner.yml
  install -m 0644 "$dir/otelcol-runner.nft" /etc/agentsfleet/otelcol-runner.nft
  install -m 0644 "$dir/otelcol-runner.service" /etc/systemd/system/otelcol-runner.service
  if [ "$(readlink -f "$credentials")" != /etc/agentsfleet/otelcol-runner.env ]; then
    install -m 0600 "$credentials" /etc/agentsfleet/otelcol-runner.env
  fi
  systemctl daemon-reload
  systemctl enable --now otelcol-runner.service
  curl --fail --silent --show-error --retry 5 --retry-delay 1 --retry-connrefused \
    --max-time 5 http://127.0.0.1:13133/ >/dev/null
  install -d -m 0755 /etc/systemd/system/agentsfleet-runner.service.d
  cat >/etc/systemd/system/agentsfleet-runner.service.d/telemetry.conf <<'UNIT'
[Unit]
Wants=otelcol-runner.service
After=otelcol-runner.service
[Service]
Environment=OTEL_EXPORTER_OTLP_ENDPOINT=http://127.0.0.1:4318
UNIT
  systemctl daemon-reload
  echo 'Collector installed. Runner export takes effect on its next normal deployment restart.'
}

if [[ "${BASH_SOURCE[0]}" == "$0" ]]; then
  [ "$#" -eq 1 ] || { echo 'Usage: install-collector.sh ROOT_ONLY_CREDENTIAL_FILE' >&2; exit 2; }
  install_collector "$1"
fi
