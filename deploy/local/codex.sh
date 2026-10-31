#!/usr/bin/env bash
# Linux adapter; the runner keeps its shipping sandbox and signed-toolbox checks.
source "$DEV_ROOT/deploy/local/codex-certificate.sh"
acceptance_runner_owned=0

acceptance_adapter() {
  [ "$(uname -s)" = Linux ] || dev_fail 'acceptance-codex requires Linux and privileged Docker'
  docker info >/dev/null || dev_fail 'acceptance-codex requires a running Docker daemon'
  command -v certutil >/dev/null || dev_fail 'install libnss3-tools for browser certificate trust'
  if [ -n "${CODEX_PROXY_CERT:-}" ]; then
    export LOCAL_DEV_COMPOSE_FILE="$DEV_STATE/acceptance/cloud-infra.json"
    mkdir -p "$(dirname "$LOCAL_DEV_COMPOSE_FILE")"
    python3 - "$CODEX_PROXY_CERT" "$LOCAL_DEV_COMPOSE_FILE" <<'PY'
import json, sys
from pathlib import Path
volume = {'type': 'bind', 'source': sys.argv[1], 'target': '/run/session-ca.pem', 'read_only': True}
overlay = {'services': {'qstash': {'volumes': [volume], 'environment': {'NODE_EXTRA_CA_CERTS': '/run/session-ca.pem'}}}}
Path(sys.argv[2]).write_text(json.dumps(overlay))
PY
  fi
}

acceptance_adapter_build() {
  unset ACCEPTANCE_RUNNER_MACHINE
}

acceptance_machine() {
  acceptance_runner_image="agentsfleet-acceptance-$DEV_ID"
  acceptance_runner_container="$DEV_PROJECT-$DEV_ID-runner"
  local args=(--file deploy/local/codex-runner.Dockerfile --tag "$acceptance_runner_image"
    --build-arg HTTP_PROXY --build-arg HTTPS_PROXY --build-arg NO_PROXY
    --build-arg http_proxy --build-arg https_proxy --build-arg no_proxy)
  if [ -n "${CODEX_PROXY_CERT:-}" ]; then
    args+=(--secret "id=session-ca,src=$CODEX_PROXY_CERT")
  fi
  docker build "${args[@]}" .
}

acceptance_toolbox() {
  source "$DEV_ROOT/deploy/baremetal/toolbox.sh"
  local files image manifest public signature
  files="$(toolbox_set_files "$ACCEPTANCE_LOCAL_TOOLBOX_DIR")" || dev_fail 'provide an existing signed toolbox set in ACCEPTANCE_LOCAL_TOOLBOX_DIR'
  image="$(printf '%s\n' "$files" | head -1)"
  manifest="${image%.erofs}.json"
  public="$LOCAL_ACCEPTANCE_STATE/runner-release.pub"
  signature="$LOCAL_ACCEPTANCE_STATE/toolbox-signature.der"
  awk '/-----BEGIN PUBLIC KEY-----/{p=1; sub(/^.*-----BEGIN/, "-----BEGIN")} p{print} /-----END PUBLIC KEY-----/{exit}' \
    "$DEV_ROOT/rustd/crates/afr_sandbox/src/toolbox/manifest/mod.rs" >"$public"
  base64 --decode "$manifest.sig" >"$signature"
  openssl dgst -sha256 -verify "$public" -signature "$signature" "$manifest" >/dev/null || dev_fail 'toolbox signature differs from the compiled release key'
  python3 - "$manifest" "$DEV_ROOT/VERSION" <<'PY'
import json, sys
from pathlib import Path
manifest = json.loads(Path(sys.argv[1]).read_text())
if Path(sys.argv[2]).read_text().strip() not in manifest['runner_versions']:
    sys.exit('local acceptance: signed toolbox does not admit this runner version')
PY
}

acceptance_proxy_config() {
  export LOCAL_ACCEPTANCE_ADMIN_PORT="$((DEV_PORT + 10002))"
  printf '{\n admin 127.0.0.1:%s\n auto_https disable_redirects\n}\n' "$LOCAL_ACCEPTANCE_ADMIN_PORT" >"$LOCAL_ACCEPTANCE_STATE/Caddyfile"
  printf 'https://localhost:%s {\n bind 127.0.0.1\n tls internal\n reverse_proxy localhost:%s\n}\n' \
    "$LOCAL_ACCEPTANCE_TLS_PORT" "$DEV_PORT" >>"$LOCAL_ACCEPTANCE_STATE/Caddyfile"
  printf 'https://localhost:%s {\n bind 127.0.0.1\n tls internal\n reverse_proxy localhost:%s\n}\n' \
    "$LOCAL_ACCEPTANCE_UI_TLS_PORT" "$LOCAL_ACCEPTANCE_UI_PORT" >>"$LOCAL_ACCEPTANCE_STATE/Caddyfile"
}

acceptance_proxy() {
  docker compose --project-name "$DEV_PROJECT-$DEV_ID-acceptance" --file deploy/local/compose.yml \
    --file deploy/local/codex-compose.yml "$@"
}

start_runner() {
  if docker container inspect "$acceptance_runner_container" >/dev/null 2>&1; then
    dev_fail 'the acceptance runner container already exists; inspect its owner before removing it'
  fi
  local args=(--init --name "$acceptance_runner_container" --privileged --cgroupns=private --network host
    --mount "type=bind,src=$DEV_ROOT/dist/acceptance/agentsfleet-runner,dst=/usr/local/bin/agentsfleet-runner,readonly"
    --mount "type=bind,src=$DEV_ROOT/deploy/local/codex-runner.sh,dst=/usr/local/bin/acceptance-runner,readonly"
    --mount "type=bind,src=$ACCEPTANCE_LOCAL_TOOLBOX_DIR,dst=/toolbox,readonly"
    --mount "type=bind,src=$LOCAL_ACCEPTANCE_CA,dst=/acceptance-ca.crt,readonly"
    --env-file "$LOCAL_ACCEPTANCE_STATE/runner.env")
  if [ -n "${CODEX_PROXY_CERT:-}" ]; then
    args+=(--mount "type=bind,src=$CODEX_PROXY_CERT,dst=/run/session-ca.pem,readonly")
  fi
  acceptance_runner_container="$(docker create "${args[@]}" "$acceptance_runner_image" \
    bash /usr/local/bin/acceptance-runner)"
  acceptance_runner_owned=1
  docker start "$acceptance_runner_container" >/dev/null
}

acceptance_adapter_cleanup() {
  local status=0
  if [ "$acceptance_runner_owned" = 1 ]; then
    docker logs "$acceptance_runner_container" >"$LOCAL_ACCEPTANCE_STATE/runner.log" 2>&1 || status=$?
    docker stop --time 30 "$acceptance_runner_container" >/dev/null || status=$?
    if docker rm "$acceptance_runner_container" >/dev/null; then
      acceptance_runner_owned=0
    else
      status=$?
    fi
  fi
  return "$status"
}
