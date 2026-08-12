#!/usr/bin/env bash
#
# The unprivileged-migrate lane's role list must match the roles the schema
# creates — exactly.
#
#     bash scripts/check_migrate_role_parity.sh
#
# `check-migrate-unprivileged.sh` mirrors, onto its scratch migrator, the ADMIN
# OPTION that a managed migrator holds by virtue of having created each role.
# That mirror is driven by a hand-written `APP_ROLES` list. A role added to the
# schema and not to that list makes the lane model a cluster deploy never
# produces: the role exists, the migrator did not create it, and slot 110's
# membership GRANT is refused with 42501 — twelve minutes into the coverage lane,
# naming a role rather than the list that omitted it.
#
# That is not hypothetical. The milestone that introduced the elevation roles
# added three of them across slots 110 and 120, left the list at five, and the
# coverage lane went red exactly that way. (No literal milestone identifier
# here: production source must stay milestone-free, RULE TST-NAM.)
#
# This gate is static — no database, no container — so it fails in a second
# inside `lint-all` rather than a quarter of an hour inside the coverage lane.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
readonly SCRIPT_DIR
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
readonly REPO_ROOT
# LANE and SCHEMA_DIR are overridable so the red-green proof can point the gate
# at a pre-fix copy of the lane without checking out the parent commit — the
# same reason check_architecture_doc.sh exposes ARCH_DIR and SPEC_ROOT.
readonly LANE="${LANE:-$SCRIPT_DIR/check-migrate-unprivileged.sh}"
readonly SCHEMA_DIR="${SCHEMA_DIR:-$REPO_ROOT/schema}"

# Statement text only. Slot 110's own prose says "which CREATE ROLE defaults to
# TRUE", so a scan that reads comments invents a role named `defaults` — this
# gate caught exactly that on its first run. Roles are [a-z0-9_], never `--`, so
# truncating each line at the comment marker cannot eat a name.
sql_body() { sed 's/--.*$//' "$SCHEMA_DIR"/*.sql; }

# Roles the schema creates, both spellings the slots use:
#   - slot 110 loops a quoted-name ARRAY into format('CREATE ROLE %I NOLOGIN', r)
#   - slot 120 EXECUTEs a literal 'CREATE ROLE <name> NOLOGIN'
# Scanning all of schema/ rather than the two known files means a third slot
# adding a role is covered the day it lands.
schema_roles() {
  local body
  body="$(sql_body)"
  {
    printf '%s\n' "$body" |
      awk '/FOREACH[[:space:]]+[a-z_]+[[:space:]]+IN[[:space:]]+ARRAY[[:space:]]+ARRAY\[/,/\]/' |
      grep -oE "'[a-z][a-z0-9_]*'" | tr -d "'"
    printf '%s\n' "$body" | grep -oE "CREATE ROLE [a-z][a-z0-9_]*" | awk '{ print $3 }'
  } | sort -u
}

# The lane's list, read from source rather than by sourcing the file — the lane
# runs docker and migrations at import time.
#
# Deliberately awk and not a `sed` range. A sed range starts matching its end
# pattern on the line AFTER the start, so a single-line `readonly APP_ROLES="…"`
# never terminates on itself: the range runs on into su_psql() and harvests
# `docker compose exec postgres psql` as role names. The red-green proof against
# the pre-fix lane caught that — a false-positive generator is worse than no
# gate. This closes on the quote wherever it appears, one line or many.
lane_roles() {
  awk '
    !collecting && /^readonly APP_ROLES="/ {
      collecting = 1
      sub(/^readonly APP_ROLES="/, "")
    }
    collecting {
      done = index($0, "\"") > 0
      if (done) { sub(/".*$/, "") }
      sub(/\\[[:space:]]*$/, "")
      print
      if (done) { exit }
    }
  ' "$LANE" | tr ' \t' '\n\n' | grep -E '^[a-z][a-z0-9_]*$' | sort -u
}

main() {
  [ -r "$LANE" ] || { echo "✗ [role-parity] cannot read $LANE" >&2; exit 1; }

  local schema lane
  schema="$(schema_roles)"
  lane="$(lane_roles)"

  # Empty-scan guard, same reason check-vault-gate-parity carries one: a scan
  # that matched nothing has proved nothing and would pass silently after a
  # refactor renames the SQL or the variable.
  if [ -z "$schema" ]; then
    echo "✗ [role-parity] schema scan matched no CREATE ROLE — the scan is broken, not the tree" >&2
    exit 1
  fi
  if [ -z "$lane" ]; then
    echo "✗ [role-parity] APP_ROLES scan matched nothing in $LANE — the scan is broken" >&2
    exit 1
  fi

  local missing extra fail=0
  missing="$(comm -23 <(printf '%s\n' "$schema") <(printf '%s\n' "$lane"))"
  extra="$(comm -13 <(printf '%s\n' "$schema") <(printf '%s\n' "$lane"))"

  if [ -n "$missing" ]; then
    fail=1
    echo "✗ [role-parity] schema creates roles the unprivileged lane does not mirror:" >&2
    printf '%s\n' "$missing" | sed 's/^/    /' >&2
    echo "  Add them to APP_ROLES in scripts/check-migrate-unprivileged.sh, or the" >&2
    echo "  lane's migrator meets a role it never created and the GRANT fails 42501." >&2
  fi

  if [ -n "$extra" ]; then
    fail=1
    echo "✗ [role-parity] APP_ROLES names roles the schema never creates:" >&2
    printf '%s\n' "$extra" | sed 's/^/    /' >&2
    echo "  Drop them — a mirrored role that no slot creates is dead fixture." >&2
  fi

  [ "$fail" -eq 0 ] || exit 1

  echo "✓ [role-parity] APP_ROLES matches the $(printf '%s\n' "$schema" | wc -l | tr -d ' ') roles schema/ creates"
}

main "$@"
