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
   (`schema/110_roles_and_privileges.sql`). unverified: the grant that made
   the live API login a member of `api_runtime` is not recorded in
   `playbooks/` — when you create the new branch's credentials, apply the
   same grant and write it into this step.
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
DATABASE_URL_MIGRATOR="$(op read 'op://ZMB_CD_DEV/planetscale-dev/next-migrator-connection-string')" \
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

`02_copy.sh` refuses to run until the target's migration ledger stands at the
source's version — that is the proof step 4 happened. It then runs
`pg_dump --data-only` on the live branch and `pg_restore --exit-on-error` on
the new one inside `postgres:18-alpine`, excluding the two ledger tables the
migration already wrote. Data only, because a managed database will not let a
dump restore ownership or privileges, and the migrator has already created
every role and grant exactly as a deploy would.

If `pg_restore` aborts on a duplicate key, a migration seeded that table on
the target; confirm the seeded rows match the source's and rerun with the
table added to `LEDGER_TABLES` in `02_copy.sh`. The verify step then proves
the counts agree.

`03_verify.sh` counts every user table on both branches from the catalog —
never from a hand-kept list — and diffs the two censuses with the ledger
version. Any difference fails the run.

## Required result

- `ACTION=check` prints two different hosts and three `✓ port` lines.
- `ACTION=copy` ends with `✅ section 3 passed - every table and the ledger match`.
- After the swap and redeploy, `/readyz` answers 200 and
  `ENV=<env> STAGE=deployment ./playbooks/founding/02_preflight/00_gate.sh`
  passes its port checks against the swapped strings.
