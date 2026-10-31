#!/usr/bin/env bash
# Sourced by acceptance.sh; private state survives retries and worktree restarts.

acceptance_environment() {
  [ ! -L "$LOCAL_DEV_ENV_FILE" ] || dev_fail 'acceptance environment cannot be a symlink'
  if [ ! -f "$LOCAL_DEV_ENV_FILE" ]; then
    [ -z "${AGENTSFLEET_LOCAL_ACCEPTANCE_ENV_FILE:-}" ] || dev_fail 'selected acceptance environment does not exist'
    dev_infrastructure up
    if [ "$(dev_compose exec -T postgres psql -U agentsfleet -d postgres -Atc "SELECT EXISTS(SELECT FROM pg_database WHERE datname = '$DEV_DB')")" = t ]; then
      [ -n "${ENCRYPTION_MASTER_KEY:-}" ] || dev_fail 'existing database needs its original encryption key; refusing to generate a replacement'
    fi
    local pending="$LOCAL_DEV_ENV_FILE.new"
    [ ! -e "$pending" ] && [ ! -L "$pending" ] || dev_fail 'unfinished acceptance environment exists'
    (
      umask 077
      set -o noclobber
      printf 'ENCRYPTION_MASTER_KEY=%q\n' "${ENCRYPTION_MASTER_KEY:-$(openssl rand -hex 32)}" >"$pending"
      printf 'ACCEPTANCE_LOCAL_RUNNER_MACHINE=%q\n' "${ACCEPTANCE_LOCAL_RUNNER_MACHINE:-afr-acceptance-${DEV_ID#wt_}}" >>"$pending"
      printf 'ACCEPTANCE_LOCAL_TOOLBOX_DIR=%q\n' "${ACCEPTANCE_LOCAL_TOOLBOX_DIR:-$DEV_STATE/acceptance/toolbox}" >>"$pending"
      printf 'ACCEPTANCE_LOCAL_RUNNER_RECORD=%q\n' "$DEV_STATE/acceptance/runner.json" >>"$pending"
    )
    mv "$pending" "$LOCAL_DEV_ENV_FILE"
  fi
  [ -O "$LOCAL_DEV_ENV_FILE" ] || dev_fail 'acceptance environment must belong to this user'
  chmod 600 "$LOCAL_DEV_ENV_FILE"
}

acceptance_platform() {
  local vault refs field
  vault="${ACCEPTANCE_LOCAL_CONNECTOR_VAULT:-$(acceptance_vault DEV github-app)}"
  refs="$LOCAL_ACCEPTANCE_STATE/connector-fields.env"
  : >"$refs"
  for field in app_id app_slug client_id client_secret private_key_pem webhook_secret; do
    printf 'LOCAL_GITHUB_%s=op://%s/github-app/%s\n' "$(printf '%s' "$field" | tr '[:lower:]' '[:upper:]')" "$vault" "$field" >>"$refs"
  done
  (
    cd "$DEV_ROOT/ui/packages/app"
    export NEXT_PUBLIC_API_URL="$DEV_URL" LOCAL_ACCEPTANCE_API="$DEV_URL" LOCAL_ACCEPTANCE_NAMESPACE="$DEV_ID"
    FIREWORKS_API_KEY="${model_bootstrap_key:-${FIREWORKS_API_KEY:-}}" op run --env-file "$refs" -- bun "$DEV_ROOT/deploy/local/bootstrap.ts"
  )
}

acceptance_vault() {
  op item list --format json | bun -e '
const rows = await Bun.stdin.json();
const suffix = new RegExp(`(?:^|_)${Bun.argv[1]}$`);
const matches = rows.filter(row => row.title === Bun.argv[2] && suffix.test(row.vault?.name ?? ""));
if (matches.length !== 1) throw new Error("select the acceptance vault in the private environment file");
process.stdout.write(matches[0].vault.id);
' "$1" "$2"
}

acceptance_sign() {
  # Same offline signature and compiled-key check as the build-toolbox action.
  local root="$1" manifest="$2" public expected
  public="$(dirname "$manifest")/release.pub"
  cosign public-key --key env://COSIGN_PRIVATE_KEY --outfile "$public"
  expected="$(awk '/-----BEGIN PUBLIC KEY-----/{p=1; sub(/^.*-----BEGIN/, "-----BEGIN")} p{print} /-----END PUBLIC KEY-----/{exit}' \
    "$root/rustd/crates/afr_sandbox/src/toolbox/manifest/mod.rs" | tr -d '[:space:]')"
  [ "$(tr -d '[:space:]' <"$public")" = "$expected" ] || { echo 'toolbox signing key differs from the runner key' >&2; return 1; }
  cosign sign-blob --yes --tlog-upload=false --new-bundle-format=false --use-signing-config=false \
    --key env://COSIGN_PRIVATE_KEY --output-signature "$manifest.sig" "$manifest"
  cosign verify-blob --insecure-ignore-tlog --key "$public" --signature "$manifest.sig" "$manifest"
}

if [ "${BASH_SOURCE[0]}" = "$0" ]; then
  set -euo pipefail
  [ "${1:-}" = sign ] || exit 2
  acceptance_sign "$2" "$3"
fi
