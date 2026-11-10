---
type: how-to
audience: operator
verified: 2026-11-08
product_version: 0.75.0
executable: false
---

# Move a PlanetScale database to another region

## What this does

A branch's region is fixed at creation, so a move is a new `us-east` branch, a migration, a data copy, a verification and a credential swap.
`us-east` shares a metro with Fly `iad`; Fly has no Ohio region.
Each run targets `ENV=dev` or `ENV=prod`, never both.
Writes stay stopped from step 3 to step 8.

## Before you begin

🤠 Indy does these in the PlanetScale console.

1. **Branches → New branch**: region `us-east`, the live branch's cluster size.
2. **Clusters → Parameters**: read `max_connections` (25 today; 3 reserved, 3 held by PlanetScale admin and exporter roles).
3. **PgBouncers →** local bouncer: set `default_pool_size` so the pool, those 6 and the release migrator fit.
   16 fits 25; raise the cluster size if the pool must stay at 20.
4. Create the API login as a member of `api_runtime` only; the migrator login needs the live migrator's privileges.

   ```bash
   pscale role create <DB> <BRANCH> <LOGIN> --inherited-roles api_runtime
   ```

5. On port `5432`, set the bounds `afd_db::login` declares on the API login itself, not on `api_runtime`.

   ```sql
   ALTER ROLE <LOGIN> SET lock_timeout = '2000ms';
   ALTER ROLE <LOGIN> SET statement_timeout = '5000ms';
   ```

6. Check memberships against `schema/20261108120100_roles_and_privileges.sql`.
   Never grant `pg_read_all_data` or `pg_write_all_data`; they bypass the `memory` schema boundary.
7. In item `planetscale-<ENV>` of the vault `VAULT_DEV` or `VAULT_PROD` names, stage beside the live fields:
   `next-api-connection-string` on port `6432` (PgBouncer) and `next-migrator-connection-string` on `5432`.
   The migrator stays direct because a transaction pooler cannot hold its advisory lock.

Never stage `sslrootcert` in a vault string; `afd_db` reads it as a file and fails boot with `TlsCertFileUnreadable`.
The scripts add `sslrootcert=system` inside the container only.
IP restrictions are per database, so the egress allowlist carries over.

## Steps

Production uses `ENV=prod`, `agentsfleetd-prod`, image `agentsfleetd:latest`, three machines and `release.yml`.

1. 🤠 Indy completes **Before you begin**.
2. 🦉 The agent checks the staged strings, read-only.

   ```bash
   export ALLOW_VAULT_READS=1
   ENV=dev ACTION=check ./playbooks/operations/database_region_move/00_gate.sh
   ```

   ```text
   ✓ target is a different host from the source
   ✓ port 5432: live migrator (direct)
   ✓ port 5432: next migrator (direct)
   ✓ port 6432: next api (PgBouncer)
   PASS: database-region-move check completed for dev
   ```

3. 🦉 The agent stops writers with `fly scale count 0 --app agentsfleetd-dev --yes`.
4. 🦉 The agent migrates the new branch with the serving image; the string passes by name, never in process arguments.

   ```bash
   DATABASE_URL_MIGRATOR="$(op read "op://${VAULT_DEV:?Set the development vault name}/planetscale-dev/next-migrator-connection-string")" \
     docker run --rm -e DATABASE_URL_MIGRATOR \
     ghcr.io/agentsfleet/agentsfleetd:dev-latest /usr/local/bin/agentsfleetd migrate
   ```

5. 🤠 Indy approves the copy: `ALLOW_PROVIDER_WRITES=1`.
6. 🦉 The agent copies and verifies; the script asks for the environment name before writing.

   ```bash
   ALLOW_PROVIDER_WRITES=1 ENV=dev ACTION=copy ./playbooks/operations/database_region_move/00_gate.sh
   ```

   ```text
   ✅ section 3 passed - every table and the ledger match
   ```

7. 🤠 Indy swaps the vault: `next-*` values become `api-connection-string` and `migrator-connection-string`.
   Keep the old values in the item's history.
8. 🦉 The agent redeploys, which re-stages Fly secrets from the vault, and scales back up.

   ```bash
   gh workflow run deploy-dev.yml
   fly scale count 2 --app agentsfleetd-dev --yes
   curl -fsS https://api-dev.agentsfleet.net/readyz
   ```

9. 🦉 The agent confirms `/readyz` answers 200, then runs the [acceptance walk](../acceptance/001_playbook.md).
10. 🤠 Indy promotes the new branch after a soak and deletes the old one.

## How the copy behaves

- `02_copy.sh` refuses unless both `audit._sqlx_migrations` versions match; after an incompatible rebuild, use clean-database priming.
- It dumps data only, as plain SQL in `postgres:18-alpine`, excluding the ledger table.
- The load truncates catalog-discovered tables under `session_replication_role = 'replica'`, which the target role must be allowed to set.
- Truncate and restore share one `psql --single-transaction`, so a failure leaves the target unchanged.
  Fix the blocker and retry; never exclude more tables to dodge duplicate rows.
- `03_verify.sh` diffs per-table row counts from the catalog, plus the ledger version.

## Verify it works

```bash
ENV=dev ACTION=verify ./playbooks/operations/database_region_move/00_gate.sh
ENV=dev STAGE=deployment ./playbooks/founding/02_preflight/00_gate.sh
```

```text
PASS: database-region-move verify completed for dev
<varies: preflight port checks pass against the swapped strings>
```

## Common problems

| Message | Fix |
|---|---|
| `❌ the target is not migrated to the source's version` | Run step 4. |
| `❌ section 3 failed: the branches differ` | A missing table means another binary migrated the target; a row gap means a partial copy or a live writer. |
