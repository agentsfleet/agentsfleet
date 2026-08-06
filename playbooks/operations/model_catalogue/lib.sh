#!/usr/bin/env bash
# Shared resolution for the model-catalogue steps.
#
# Sourced, never executed. Exists so the vault item name and the connection
# reference are spelled ONCE: three steps need the same value, and three
# spellings is how one of them eventually points at the wrong environment
# (RULE UFS).
#
# It carries the vault preamble itself rather than trusting its callers to have
# run it. Sourcing this file means a vault read is imminent, so the approval
# belongs here too — and `check-vault-gate-parity` enforces exactly that, for
# any file that resolves an `op://` reference. The checks are pure, so a caller
# that already ran them pays nothing.

_CATALOGUE_LIB_DIR="$(cd "${BASH_SOURCE[0]%/*}" && pwd)"
# shellcheck source=../../lib/common.sh
source "${_CATALOGUE_LIB_DIR}/../../lib/common.sh"
playbooks_require_vault_read_approval
playbooks_require_op_auth

readonly CATALOGUE_ITEM_DEV="planetscale-dev"
readonly CATALOGUE_ITEM_PROD="planetscale-prod"
readonly CATALOGUE_URL_FIELD="migrator-connection-string"
readonly CATALOGUE_SEED_SCRIPT="scripts/seed-models.mjs"

# Echoes the repository root so the steps can reach the seed script regardless
# of the directory the operator invoked the gate from.
catalogue_repo_root() {
  cd "${BASH_SOURCE[0]%/*}/../../.." && pwd
}

# Echoes the `op://` reference for the selected environment's migrator
# connection string. The VALUE is never echoed by any caller — it is passed to
# the seed script through the environment, so it cannot land in `ps aux`.
catalogue_url_ref() {
  local vault item
  case "${ENV:-}" in
    dev)
      vault="${VAULT_DEV:-ZMB_CD_DEV}"
      item="$CATALOGUE_ITEM_DEV"
      ;;
    prod)
      vault="${VAULT_PROD:-ZMB_CD_PROD}"
      item="$CATALOGUE_ITEM_PROD"
      ;;
    *)
      echo "ERROR: ENV must be 'dev' or 'prod'" >&2
      return 1
      ;;
  esac
  printf 'op://%s/%s/%s' "$vault" "$item" "$CATALOGUE_URL_FIELD"
}

# Resolves the connection string into DATABASE_URL for a child process.
# Fails loudly rather than letting the seed script fall back to its
# "no DATABASE_URL means fresh-install, treat the catalogue as empty" mode,
# which would silently diff against nothing and report every row as new.
catalogue_export_database_url() {
  local ref value
  ref="$(catalogue_url_ref)" || return 1
  value="$(playbooks_read_ref_or_empty "$ref")"
  if [ -z "$value" ]; then
    echo "ERROR: missing 1Password field: $ref" >&2
    return 1
  fi
  export DATABASE_URL="$value"
}
