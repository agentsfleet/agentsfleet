#!/usr/bin/env bash
# OrbStack adapter; acceptance.sh owns common setup and both acceptance suites.
source "$DEV_ROOT/deploy/local/certificate.sh"

acceptance_adapter() {
  [ "$(uname -s)" = Darwin ] || dev_fail 'local acceptance requires macOS with a dedicated OrbStack Linux machine'
}

acceptance_adapter_build() {
  : "${ACCEPTANCE_LOCAL_RUNNER_MACHINE:?dedicated OrbStack machine is required}"
  export ACCEPTANCE_RUNNER_MACHINE="$ACCEPTANCE_LOCAL_RUNNER_MACHINE"
}

acceptance_adapter_cleanup() { :; }

acceptance_machine() {
  if ! orb list --format json | bun -e '
const rows = await Bun.stdin.json();
process.exit(rows.some(row => row.name === Bun.argv[1]) ? 0 : 1);
' "$ACCEPTANCE_LOCAL_RUNNER_MACHINE"; then
    orb create debian:trixie "$ACCEPTANCE_LOCAL_RUNNER_MACHINE"
  fi
  orb -m "$ACCEPTANCE_LOCAL_RUNNER_MACHINE" bash -s -- "$DEV_ROOT" <<'LINUX'
set -euo pipefail
export PATH="$HOME/.cargo/bin:$PATH"
if ! command -v cargo >/dev/null || ! command -v mmdebstrap >/dev/null || ! command -v bwrap >/dev/null; then
  bash "$1/scripts/toolbox/install-builder-tools.sh" build-essential pkg-config libssl-dev bubblewrap ca-certificates git
fi
if ! command -v cargo >/dev/null; then
  curl --proto '=https' --tlsv1.2 -fsS https://sh.rustup.rs | sh -s -- -y --profile minimal
fi
LINUX
}

acceptance_toolbox() {
  orb -m "$ACCEPTANCE_LOCAL_RUNNER_MACHINE" bash -s -- "$DEV_ROOT" "$ACCEPTANCE_LOCAL_TOOLBOX_DIR" <<'LINUX'
set -euo pipefail
root="$1" out="$2"
mkdir -p "$out"
version="$(tr -d '[:space:]' <"$root/VERSION")"
if python3 - "$out" "$version" <<'PY'
from pathlib import Path
import json, sys
images = list(Path(sys.argv[1]).glob('toolbox-*.erofs'))
if len(images) != 1: sys.exit(1)
manifest = images[0].with_suffix('.json')
if not manifest.is_file(): sys.exit(1)
sys.exit(0 if sys.argv[2] in json.loads(manifest.read_text())['runner_versions'] else 1)
PY
then exit 0; fi
[ -z "$(ls -A "$out")" ] || { echo 'toolbox version differs; select an empty toolbox directory' >&2; exit 1; }
sudo bash "$root/scripts/toolbox/build.sh" "$out"
LINUX
  local image manifest vault refs
  image="$(orb -m "$ACCEPTANCE_LOCAL_RUNNER_MACHINE" bash -c 'source "$1/deploy/baremetal/toolbox.sh"; toolbox_set_files "$2"' -- "$DEV_ROOT" "$ACCEPTANCE_LOCAL_TOOLBOX_DIR" 2>/dev/null | head -1)" || true
  # An unsigned first build has no complete set yet. Locate its single manifest.
  manifest="$(orb -m "$ACCEPTANCE_LOCAL_RUNNER_MACHINE" python3 -c 'from pathlib import Path; import sys; rows=list(Path(sys.argv[1]).glob("toolbox-*.json")); assert len(rows)==1; print(rows[0])' "$ACCEPTANCE_LOCAL_TOOLBOX_DIR")"
  if [ -n "$image" ]; then return; fi
  vault="${ACCEPTANCE_LOCAL_SIGNING_VAULT:-$(acceptance_vault PROD toolbox-release-key)}"
  refs="$LOCAL_ACCEPTANCE_STATE/toolbox-signing.env"
  printf 'COSIGN_PRIVATE_KEY=op://%s/toolbox-release-key/private-key\nCOSIGN_PASSWORD=op://%s/toolbox-release-key/password\n' "$vault" "$vault" >"$refs"
  # OrbStack shares the host filesystem, so signing never needs a private-key file.
  op run --env-file "$refs" -- bash "$DEV_ROOT/deploy/local/bootstrap.sh" sign "$DEV_ROOT" "$manifest"
}

start_runner() {
  orb -m "$ACCEPTANCE_LOCAL_RUNNER_MACHINE" sudo bash -s -- "$DEV_ROOT" \
    "$ACCEPTANCE_LOCAL_TOOLBOX_DIR" "$LOCAL_ACCEPTANCE_STATE/runner.env" "$LOCAL_ACCEPTANCE_CA" "$LOCAL_ACCEPTANCE_TLS_PORT" <<'LINUX'
set -euo pipefail
root="$1" toolbox="$2" env_file="$3" ca="$4" port="$5"
source "$root/deploy/baremetal/deploy.sh"
check_deploy_inputs "$root/dist/acceptance/agentsfleet-runner" "$toolbox" "$env_file"
install -d -m 0755 "$HOST_DEPLOY_DIR" "$RUNNER_STORAGE_HOME_DEFAULT"
for entry in "${HOST_DEPLOY_FILES[@]}"; do
  install -m "${entry##*:}" "$root/deploy/baremetal/${entry%:*}" "$HOST_DEPLOY_DIR/${entry%:*}"
done
install -m 0600 "$env_file" "$HOST_ENV_FILE"
cat /etc/ssl/certs/ca-certificates.crt "$ca" >"$RUNNER_STORAGE_HOME_DEFAULT/ca.pem"
chmod 0644 "$RUNNER_STORAGE_HOME_DEFAULT/ca.pem"
printf '[Socket]\nListenStream=127.0.0.1:%s\n' "$port" >/run/systemd/system/agentsfleet-runner-tls.socket
printf '[Service]\nExecStart=/usr/lib/systemd/systemd-socket-proxyd host.orb.internal:%s\nNoNewPrivileges=yes\nProtectSystem=strict\nProtectHome=yes\n' "$port" \
  >/run/systemd/system/agentsfleet-runner-tls.service
systemctl daemon-reload
systemctl stop agentsfleet-runner-tls.socket agentsfleet-runner-tls.service
systemctl start agentsfleet-runner-tls.socket
bash "$HOST_DEPLOY_DIR/deploy.sh" runner "$(cat "$root/VERSION")" "$root/dist/acceptance/agentsfleet-runner" "$toolbox"
LINUX
}


acceptance_proxy_config() {
  printf 'https://localhost:443 {\n tls internal\n reverse_proxy host.docker.internal:%s\n}\nhttps://localhost:444 {\n tls internal\n reverse_proxy host.docker.internal:%s\n}\n' \
    "$DEV_PORT" "$LOCAL_ACCEPTANCE_UI_PORT" >"$LOCAL_ACCEPTANCE_STATE/Caddyfile"
}

acceptance_proxy() {
  docker compose --project-name "$DEV_PROJECT-$DEV_ID-acceptance" --file deploy/local/compose.yml "$@"
}
