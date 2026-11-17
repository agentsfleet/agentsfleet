#!/usr/bin/env bash
# Writes the two machine environment files `make acceptance` reads, owner-only,
# from credentials already in the environment, and prints the variables that
# point at them. Every acceptance machine uses it, so none can drift.
# Usage: machine-env.sh <directory>
set -euo pipefail

# The dev Clerk session token carries this audience (deploy-dev-fly.yml, docs/AUTH.md).
readonly OIDC_AUDIENCE_DEV="https://api-dev.agentsfleet.net"
readonly CLERK_API_BASE_DEV="https://api.clerk.com/v1"

directory="${1:?usage: machine-env.sh <directory>}"
for name in CLERK_SECRET_KEY CLERK_PUBLISHABLE_KEY CLERK_WEBHOOK_SECRET OIDC_ISSUER AUTH_SESSION_CODE_PEPPER; do
  if [ -z "${!name:-}" ]; then
    printf 'machine-env: %s is not set\n' "$name" >&2
    exit 1
  fi
done

umask 077
mkdir -p "$directory"
{
  printf 'NEXT_PUBLIC_CLERK_PUBLISHABLE_KEY=%s\n' "$CLERK_PUBLISHABLE_KEY"
  printf 'CLERK_PUBLISHABLE_KEY=%s\n' "$CLERK_PUBLISHABLE_KEY"
  printf 'CLERK_SECRET_KEY=%s\n' "$CLERK_SECRET_KEY"
  printf 'CLERK_WEBHOOK_SECRET=%s\n' "$CLERK_WEBHOOK_SECRET"
} >"$directory/ui.env"
{
  printf 'OIDC_ISSUER=%s\n' "$OIDC_ISSUER"
  printf 'OIDC_AUDIENCE=%s\n' "$OIDC_AUDIENCE_DEV"
  printf 'CLERK_API_BASE=%s\n' "$CLERK_API_BASE_DEV"
  printf 'CLERK_SECRET_KEY=%s\n' "$CLERK_SECRET_KEY"
  printf 'AUTH_SESSION_CODE_PEPPER=%s\n' "$AUTH_SESSION_CODE_PEPPER"
} >"$directory/agentsfleetd.env"

printf 'AGENTSFLEET_UI_ENV_FILE=%s\n' "$directory/ui.env"
printf 'AGENTSFLEET_RUSTD_ENV_FILE=%s\n' "$directory/agentsfleetd.env"
