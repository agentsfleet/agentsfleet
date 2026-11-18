---
type: how-to
audience: operator
verified: 2026-11-18
product_version: 0.77.0
executable: false
---

# Tear down a PlanetScale database

## What this does

> **Warning:** this permanently drops every user schema and every `public` table in one PlanetScale database.

`teardown.sql` finds user schemas from the catalog and skips PostgreSQL system and `pscale%` schemas.
Application roles survive, because dropping a schema does not drop roles.
Each run targets `ENV=dev` or `ENV=prod`; the gate refuses `all`.

🤠 Indy authorizes the target and runs the gate; 🦉 the agent runs the other steps and verifies.
Never use a production teardown in a routine deployment.

## Before you begin

1. Stop traffic and every writer in the selected environment.
   The daemon is the only writer: run `flyctl machine list --app agentsfleetd-<ENV>`, then stop every machine it lists with `flyctl machine stop <ID> [<ID> …] --app agentsfleetd-<ENV>`; production runs three.
   It has no public service, so nothing starts them again before the deploy.
2. Confirm Docker and a signed-in 1Password Command-Line Interface (CLI).
3. Confirm item `planetscale-<ENV>` has `migrator-connection-string`, in the vault `VAULT_DEV` or `VAULT_PROD` names.
4. Get 🤠 Indy's approval for the target; production approval is separate from development.

## Steps

1. Run the gate; use `ENV=prod` for production.

   ```bash
   ALLOW_VAULT_READS=1 ALLOW_DATABASE_TEARDOWN=1 ENV=dev \
     ./playbooks/operations/teardown/database/00_gate.sh
   ```

   The gate checks the credential, asks you to type `DEVELOPMENT` or `PRODUCTION`, runs `teardown.sql`, and verifies.
   🤠 Indy runs it in a terminal and types the word; the gate reads standard input, so never pipe the word in.
   It runs `psql` in `postgres:18-alpine` with `PGSSLROOTCERT=system` and passes the URL by environment name, never in process arguments.

   ```text
   ✅ DEVELOPMENT teardown completed successfully
   ✅ section 2 passed - all teardowns completed
   ✅ database-teardown complete (env: dev)
   ```

2. Set the repository variable `DEV_RUNNER_READY` or `PROD_RUNNER_READY` to `false` before the next deployment starts.
   Every push to `main` starts `deploy-dev.yml`, so the merge that needed the wipe is enough to deploy.
   A runner deployed onto the empty database keeps its old token, and the daemon refuses it with `UZ-RUN-001`.
3. Keep traffic stopped until the environment is rebuilt and its migrations are green.
   - The deploy leaves the stopped machines stopped, and its verify job starts them.
   - Verify then fails at the `approval-signing` sync until priming mints the `platform-priming` key; rerun that job once priming is done.
4. Continue with founding step 04 (development) or 07 (production).
   Founding step 05 (development) or 08 (production) sets the variable back to `true` after storing the new token.
5. After migrations, prime the model catalogue the teardown emptied, as the gate prints.

   ```bash
   ACTION=diff ENV=dev ALLOW_VAULT_READS=1 ./playbooks/operations/model_catalogue/00_gate.sh
   ACTION=apply ENV=dev ALLOW_VAULT_READS=1 ALLOW_MODEL_CATALOGUE_WRITES=1 \
     ./playbooks/operations/model_catalogue/00_gate.sh
   ```

## Verify it works

The gate's verification prints a summary.

```text
User schemas: 0
User tables: 0
<varies: App roles: N, unchanged and non-zero>
User functions: 0
User triggers: 0
User indexes: 0
```

`public` holds no tables.
Re-running migrations re-grants everything; to remove a role, run `DROP ROLE <ROLE_NAME>;`.
