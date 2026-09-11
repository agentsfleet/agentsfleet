#!/usr/bin/env bash
# Shared by every step of the region move: which vault item an environment
# reads, and the two facts a connection string is allowed to reveal.
#
# Sourced, never run. The functions print only a host or a port — never the
# string they were given — so a caller can echo their output freely.

readonly REGION_MOVE_ENV_DEV="dev"
readonly REGION_MOVE_ENV_PROD="prod"
readonly REGION_MOVE_API_PORT="6432"
readonly REGION_MOVE_DIRECT_PORT="5432"

# Refuses anything but an explicit dev or prod. A move rewrites where an
# environment's data lives; "all" is two of those in one keystroke.
region_move_require_env() {
  local env_mode="${ENV:-}"
  if [ "$env_mode" != "$REGION_MOVE_ENV_DEV" ] && [ "$env_mode" != "$REGION_MOVE_ENV_PROD" ]; then
    echo "ERROR: ENV must be '$REGION_MOVE_ENV_DEV' or '$REGION_MOVE_ENV_PROD' (a region move targets one environment)" >&2
    return 1
  fi
  printf '%s' "$env_mode"
}

# The vault holding this environment's PlanetScale item.
region_move_vault() {
  case "$1" in
    "$REGION_MOVE_ENV_DEV") printf '%s' "${VAULT_DEV:-ZMB_CD_DEV}" ;;
    "$REGION_MOVE_ENV_PROD") printf '%s' "${VAULT_PROD:-ZMB_CD_PROD}" ;;
  esac
}

# The PlanetScale item for this environment.
region_move_item() {
  printf 'planetscale-%s' "$1"
}

# The host a Postgres URL names, or nothing when the URL does not parse.
region_move_url_host() {
  printf '%s' "$1" |
    sed -nE 's#^postgres(ql)?://[^@/]*@([^/?:]+)(:[0-9]+)?(/.*)?$#\2#p'
}

# The port a Postgres URL names, or nothing when it names none.
region_move_url_port() {
  printf '%s' "$1" |
    sed -nE 's#^postgres(ql)?://[^@/]*@[^/?]*:([0-9]+)(/.*)?$#\2#p'
}
