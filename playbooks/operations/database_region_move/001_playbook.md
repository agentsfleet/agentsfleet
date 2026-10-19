# Database Region Move

**Owners:** 🤠 Indy creates the branch, stages its strings, approves the copy
and swaps the vault; 🦉 Orly checks, copies, verifies and redeploys.
**Scope:** exactly one of development or production per run.

A PlanetScale branch's region is fixed at creation. Moving a database is a
new branch in the target region, a migrated schema, a data copy, a
verification, and a credential swap — with writes stopped from the copy to
the swap. This playbook is that sequence, and the scripts refuse to run it
against both environments at once.

## Why `us-east`

The daemon runs in Fly `iad` (Ashburn). AWS us-east-1 is the same metro; AWS
us-east-2 (Ohio) is not, and every connection handshake and every query pays
the round trip. Fly has no Ohio region, so the database moves, not the app.

## Prerequisites (🤠 Indy, PlanetScale console)

For the environment being moved:

1. Open the database → **Branches → New branch**. Region `us-east` (AWS
   us-east-1, Northern Virginia). Same cluster size as the live branch.
2. Open the new branch → **Connect** and copy its connection details. The
   host differs from the live one; the credentials are the branch's own.
3. Open the new branch → **Clusters → Parameters** and read
   `max_connections` (25 on the current cluster size, 3 reserved, 3 held by
   PlanetScale's own admin and exporter roles). Then **PgBouncers →** the
   local bouncer and set `default_pool_size` so that it, those three, and the
   migrator at release all fit — 16 leaves room on a 25-connection cluster.
   Raise the cluster size instead if the pool must stay at 20.
4. Stage the new strings in the vault beside the live ones — do not overwrite
   the live fields yet:

   | Vault | Item | New field | Port |
   |---|---|---|---|
   | `ZMB_CD_DEV` / `ZMB_CD_PROD` | `planetscale-dev` / `planetscale-prod` | `next-api-connection-string` | `6432` |
   | `ZMB_CD_DEV` / `ZMB_CD_PROD` | `planetscale-dev` / `planetscale-prod` | `next-migrator-connection-string` | `5432` |

   The API string is the same host and credentials with the port changed to
   `6432` — PlanetScale's PgBouncer, which every branch ships. The migrator
   stays on `5432`; a transaction pooler cannot hold its session advisory
   lock. The login roles the strings name are branch credentials created in
   the console. The migrator's creates every schema, so it needs what the
   live one has; the API's must be a member of `api_runtime`
   (`schema/1010_roles_and_privileges.sql`) — and of nothing else:

       pscale role create <db> <branch> <name> --inherited-roles api_runtime

   Verify the target login's role memberships before copying.
   `schema/1010_roles_and_privileges.sql` defines the role hierarchy; each table's slot owns its grants.
   Do not add `pg_read_all_data` or `pg_write_all_data` to bypass a missing grant.
   Those roles would bypass the intended `memory` schema boundary.
5. IP restrictions are per database, not per branch, so the Fly egress
   allowlist carries over. Nothing to redo.

## Handoff

| Order | Owner | Action |
|---|---|---|
| 1 | 🤠 Indy | Prerequisites above. |
| 2 | 🦉 Orly | `ACTION=check` — both strings staged, different host, right ports. |
| 3 | 🦉 Orly | Stop writers: scale the environment's daemon to zero machines. |
| 4 | 🦉 Orly | Migrate the new branch (below), so schema, roles, grants and triggers exist the way a deploy makes them. |
| 5 | 🤠 Indy | Approve the copy: `ALLOW_PROVIDER_WRITES=1`. |
| 6 | 🦉 Orly | `ACTION=copy` — copies data, then verifies every table's row count and the ledger version. |
| 7 | 🤠 Indy | Swap the vault: `next-*` values become `api-connection-string` and `migrator-connection-string`. Keep the old values in the item's history. |
| 8 | 🦉 Orly | Redeploy (the workflow re-stages Fly secrets from the vault) and scale back up. |
| 9 | 🦉 Orly | Verify `/readyz`, then the acceptance walk. |
| 10 | 🤠 Indy | After a soak, promote the new branch and delete the old one. |

## Run

```bash
export ALLOW_VAULT_READS=1

# 2. Read-only: fields present, hosts differ, ports right.
ENV=dev ACTION=check ./playbooks/operations/database_region_move/00_gate.sh

# 3. Stop writers.
fly scale count 0 --app agentsfleetd-dev --yes

# 4. Migrate the new branch with the image that is serving. The string is
#    forwarded by environment name so it never appears in argv.
DATABASE_URL_MIGRATOR="$(op read "op://${VAULT_DEV:?Set the development vault name}/planetscale-dev/next-migrator-connection-string")" \
  docker run --rm -e DATABASE_URL_MIGRATOR \
  ghcr.io/agentsfleet/agentsfleetd:dev-latest /usr/local/bin/agentsfleetd migrate

# 6. After 🤠 Indy approves: copy, then verify.
export ALLOW_PROVIDER_WRITES=1
ENV=dev ACTION=copy ./playbooks/operations/database_region_move/00_gate.sh

# 8. After the vault swap: redeploy, scale up, confirm.
gh workflow run deploy-dev.yml
fly scale count 2 --app agentsfleetd-dev --yes
curl -fsS https://api-dev.agentsfleet.net/readyz

# Any time later, without writing anything.
ENV=dev ACTION=verify ./playbooks/operations/database_region_move/00_gate.sh
```

Production is the same with `ENV=prod`, `agentsfleetd-prod`, the
`ghcr.io/agentsfleet/agentsfleetd:latest` image, three machines, and the
release workflow instead of `deploy-dev.yml`. Schedule the window: writes are
stopped from step 3 to step 8.

## One thing the connection strings must not carry

`sslrootcert=system` is added to a URL **inside the container only**, by
`region_move_with_system_roots`, because the `postgres` image has no trust
store at `~/.postgresql/root.crt` and PlanetScale issues `verify-full` without
naming a root. It must never reach a vault field: `afd_db` parses
`sslrootcert` as a file path and refuses to boot when it cannot read one
(`TlsCertFileUnreadable`), so a staged string carrying it is a daemon that
does not start.

## What the copy does

`02_copy.sh` first compares both databases' `audit._sqlx_migrations` versions.
A missing, older, or newer target ledger refuses the copy.
This operation requires identical schema history; use the clean-database priming route after an incompatible rebuild.

The script dumps data as plain SQL inside `postgres:18-alpine`.
It excludes `audit._sqlx_migrations` from both the dump and the target truncate.
The migrated target keeps its own checksums and ledger rows.

The load discovers application tables, truncates their existing rows, and suppresses triggers with `session_replication_role = 'replica'`.
Seeded rows therefore cannot collide, and activity counters retain the source values.
The target role must be allowed to change that session setting.

Truncate and restore run in one `psql --single-transaction` invocation.
A failure restores the target's preceding contents; fix the blocker before retrying.
Do not exclude additional application tables to work around duplicate rows.

`03_verify.sh` counts every user table on both branches from the catalog —
never from a hand-kept list — and diffs the two censuses with the ledger
version. Any difference fails the run.

## Required result

- `ACTION=check` prints two different hosts and three `✓ port` lines.
- `ACTION=copy` ends with `✅ section 3 passed - every table and the ledger match`.
- After the swap and redeploy, `/readyz` answers 200 and
  `ENV=<env> STAGE=deployment ./playbooks/founding/02_preflight/00_gate.sh`
  passes its port checks against the swapped strings.
