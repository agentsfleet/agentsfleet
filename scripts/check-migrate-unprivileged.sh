#!/usr/bin/env bash
#
# Migrate a scratch database from empty as a NON-SUPERUSER role.
#
# Every other database this repository migrates in test connects as the compose
# superuser, where PostgreSQL bypasses every privilege check. The managed
# databases (PlanetScale, dev and production) hand the migrator a plain role
# with CREATEROLE and nothing else, so any migration whose success depends on
# the *executing role's* privileges passes locally and fails on deploy — with no
# lane able to tell the difference.
#
# That is not hypothetical. Migration 110 shipped with
# `ALTER DEFAULT PRIVILEGES FOR ROLE db_migrator`, which requires INHERITED
# membership in db_migrator. PostgreSQL 16 grants a CREATEROLE creator only
# ADMIN OPTION (inherit_option = f), so the managed migrator was refused with
# 42501 and the deployment aborted — after every local and Continuous
# Integration (CI) lane went green.
#
# This lane reproduces the managed shape exactly:
#   - LOGIN + CREATEROLE, NOT superuser
#   - membership in the pre-existing app roles as ADMIN TRUE, INHERIT FALSE,
#     SET FALSE — the row PostgreSQL 16 writes for a CREATEROLE creator, and the
#     row observed on the managed database
#
# It asserts nothing about privileges directly. It asserts the migration
# COMPLETES, which is the property that actually broke.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

readonly SCRATCH_DB="unprivileged_migrate_check"
readonly SCRATCH_ROLE="unprivileged_migrator_check"
readonly SCRATCH_PASSWORD="unprivileged_migrate_check"
readonly SUPERUSER="agentsfleet"
readonly SUPERUSER_DB="agentsfleetdb"
# The roles migration 110 expects to already exist on a re-run. Membership is
# mirrored onto the scratch role so it stands where the managed migrator stands.
readonly APP_ROLES="db_migrator api_runtime memory_runtime ops_readonly_human ops_readonly_fleet"

su_psql() {
  docker compose exec -T postgres psql -U "$SUPERUSER" -d "$SUPERUSER_DB" -v ON_ERROR_STOP=1 -q "$@"
}

cleanup() {
  su_psql -c "REASSIGN OWNED BY $SCRATCH_ROLE TO $SUPERUSER;" >/dev/null 2>&1 || true
  su_psql -c "DROP OWNED BY $SCRATCH_ROLE;" >/dev/null 2>&1 || true
  su_psql -c "DROP DATABASE IF EXISTS $SCRATCH_DB;" >/dev/null 2>&1 || true
  su_psql -c "DROP ROLE IF EXISTS $SCRATCH_ROLE;" >/dev/null 2>&1 || true
}
trap cleanup EXIT

echo "→ [unpriv] Reproducing the managed (non-superuser) migrator..."

port="$(docker compose port postgres 5432 2>/dev/null | cut -d: -f2)"
if [ -z "$port" ]; then
  echo "✗ [unpriv] compose postgres is not running — run 'make _ensure-test-infra' first" >&2
  exit 1
fi

cleanup
su_psql -c "CREATE ROLE $SCRATCH_ROLE LOGIN PASSWORD '$SCRATCH_PASSWORD' CREATEROLE;"
su_psql -c "CREATE DATABASE $SCRATCH_DB OWNER $SCRATCH_ROLE;"

# Mirror PostgreSQL 16's implicit CREATEROLE grant for roles that already exist
# in this cluster. Without it the run fails earlier, on `GRANT memory_runtime TO
# api_runtime` — a real second dependency on the migrator having CREATED those
# roles, but not the one this lane is here to pin.
for role in $APP_ROLES; do
  su_psql -c "DO \$\$ BEGIN
      IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = '$role') THEN
        EXECUTE 'GRANT $role TO $SCRATCH_ROLE WITH ADMIN TRUE, INHERIT FALSE, SET FALSE';
      END IF;
    END \$\$;"
done

# Fail loudly if the role is not the shape this lane claims to test — a
# superuser here would make every assertion below vacuous.
su_psql -t -c "SELECT rolsuper FROM pg_roles WHERE rolname='$SCRATCH_ROLE';" |
  grep -q 'f' || { echo "✗ [unpriv] scratch role is a superuser; the lane would prove nothing" >&2; exit 1; }

echo "→ [unpriv] Migrating $SCRATCH_DB from empty as $SCRATCH_ROLE (not superuser)..."

url="postgres://$SCRATCH_ROLE:$SCRATCH_PASSWORD@localhost:$port/$SCRATCH_DB?sslmode=disable"

# Prefer BUILDING over an existing artifact. A stale `zig-out/bin/agentsfleetd`
# left by an earlier checkout would migrate a different schema than the one in
# the working tree, and the lane would report a verdict about code that is not
# under review — passing vacuously in the one place that must not.
#
# The prebuilt binary is the fallback for Continuous Integration (CI), which
# boots compose on the host runner but builds Zig inside a container: the host
# has the artifact and no toolchain, and the artifact was produced from this
# same checkout moments earlier, so it is fresh by construction there.
if command -v zig >/dev/null 2>&1; then
  migrate_cmd=(zig build run -- migrate)
elif [ -x "zig-out/bin/agentsfleetd" ]; then
  migrate_cmd=(./zig-out/bin/agentsfleetd migrate)
else
  echo "✗ [unpriv] neither a zig toolchain nor zig-out/bin/agentsfleetd is available" >&2
  exit 1
fi

out="$(DATABASE_URL_MIGRATOR="$url" "${migrate_cmd[@]}" 2>&1)" || {
  echo "$out" | grep -E 'pg_error|run_failed|migration_start' | tail -20
  echo "✗ [unpriv] migration FAILED as a non-superuser." >&2
  echo "  This is the class of defect that passes every superuser lane and breaks the deploy." >&2
  echo "  A statement depends on a privilege the managed migrator does not hold." >&2
  exit 1
}

printf '%s' "$out" | grep -q 'migrate.completed' || {
  echo "$out" | tail -20
  echo "✗ [unpriv] migration did not report completion" >&2
  exit 1
}

echo "✓ [unpriv] full migration applies from empty as a non-superuser migrator"
