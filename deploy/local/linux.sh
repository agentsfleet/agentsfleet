#!/usr/bin/env bash
# Linux adapter; the runner keeps its shipping sandbox and signed-toolbox checks.
# Chromium reads the user's NSS database; Bun uses the caller's extra CA bundle.
acceptance_certificate_added=0

acceptance_trust_certificate() {
  local certificate="$1" database="${2:-$HOME/.pki/nssdb}"
  acceptance_certificate_database="$database"
  acceptance_certificate_name="agentsfleet-local-$DEV_ID"
  mkdir -p "$database"
  if [ ! -f "$database/cert9.db" ]; then certutil -N --empty-password -d "sql:$database"; fi
  if certutil -L -d "sql:$database" -n "$acceptance_certificate_name" >/dev/null 2>&1; then
    printf 'Local acceptance certificate name already exists; refusing to replace its trust.\n' >&2
    return 1
  fi
  certutil -A -d "sql:$database" -n "$acceptance_certificate_name" -t 'C,,' -i "$certificate" || return $?
  acceptance_certificate_added=1
}

acceptance_untrust_certificate() {
  local certificate="$1" current expected
  if [ "$acceptance_certificate_added" = 1 ]; then
    expected="$(openssl x509 -in "$certificate" -noout -fingerprint -sha256)" || return $?
    current="$(certutil -L -d "sql:$acceptance_certificate_database" -n "$acceptance_certificate_name" -a | openssl x509 -noout -fingerprint -sha256)" || return $?
    [ "$current" = "$expected" ] || { printf 'Local acceptance trust identity changed; refusing to remove it.\n' >&2; return 1; }
    certutil -D -d "sql:$acceptance_certificate_database" -n "$acceptance_certificate_name" || return $?
    acceptance_certificate_added=0
  fi
}
acceptance_runner_owned=0

acceptance_adapter() {
  [ "$(uname -s)" = Linux ] || dev_fail 'acceptance-linux requires Linux and privileged Docker'
  docker info >/dev/null || dev_fail 'acceptance-linux requires a running Docker daemon'
  command -v certutil >/dev/null || dev_fail 'install libnss3-tools for browser certificate trust'
}

acceptance_adapter_build() {
  export LOCAL_ACCEPTANCE_RUNNER_STREAM_HOST=localhost
}

acceptance_machine() {
  acceptance_runner_image="agentsfleet-acceptance-$DEV_ID"
  acceptance_runner_container="$DEV_PROJECT-$DEV_ID-runner"
  local args=(--file deploy/local/linux-runner.Dockerfile --tag "$acceptance_runner_image"
    --build-arg HTTP_PROXY --build-arg HTTPS_PROXY --build-arg NO_PROXY
    --build-arg http_proxy --build-arg https_proxy --build-arg no_proxy)
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
    "$DEV_ROOT/rustd/crates/runner/afr_sandbox/src/toolbox/manifest/mod.rs" >"$public"
  # On stdin: macOS base64 takes no positional input file.
  base64 --decode <"$manifest.sig" >"$signature"
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
  docker compose --project-name "$DEV_PROJECT-$DEV_ID-acceptance" --file deploy/local/compose.yml "$@"
}

acceptance_runner_stream_environment() {
  printf 'AGENTSFLEET_RUNNER_CONFIG_FILE=/etc/agentsfleet/agentsfleet-runner.toml\n'
  printf 'AGENTSFLEET_RUNNER_STREAM_URL=https://%s:%s\n' \
    "$LOCAL_ACCEPTANCE_RUNNER_STREAM_HOST" "$LOCAL_ACCEPTANCE_RUNNER_STREAM_PORT"
  printf 'AGENTSFLEET_RUNNER_STREAM_TLS_CA_CERT_FILE=/acceptance-ca.crt\n'
}

start_runner() {
  if docker container inspect "$acceptance_runner_container" >/dev/null 2>&1; then
    dev_fail 'the acceptance runner container already exists; inspect its owner before removing it'
  fi
  local args=(--init --name "$acceptance_runner_container" --privileged --cgroupns=private --network host
    --mount "type=bind,src=$DEV_ROOT/dist/acceptance/agentsfleet-runner,dst=/usr/local/bin/agentsfleet-runner,readonly"
    --mount "type=bind,src=$DEV_ROOT/deploy/local/linux-runner.sh,dst=/usr/local/bin/acceptance-runner,readonly"
    --mount "type=bind,src=$DEV_ROOT/deploy/baremetal/agentsfleet-runner.toml,dst=/etc/agentsfleet/agentsfleet-runner.toml,readonly"
    --mount "type=bind,src=$ACCEPTANCE_LOCAL_TOOLBOX_DIR,dst=/toolbox,readonly"
    --mount "type=bind,src=$LOCAL_ACCEPTANCE_CA,dst=/acceptance-ca.crt,readonly"
    --env-file "$LOCAL_ACCEPTANCE_STATE/runner.env")
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
