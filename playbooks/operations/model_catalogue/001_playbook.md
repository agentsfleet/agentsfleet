---
type: how-to
audience: operator
verified: 2026-10-18
product_version: 0.61.0
executable: false
---

# Model catalogue priming

## What this does

Review and apply the curated catalogue to one environment.
The seeder reads `scripts/model-library-allowlist.json` through Bun's native Postgres client.
It writes identities to `model.platform_models` and dated prices to `model.platform_model_prices`.

An apply deletes identities and price rows absent from the reviewed snapshot.
Identity changes, price changes and the catalogue generation commit together.
A refused write rolls back all three; a repeated identical apply writes nothing.

Prices are decimal strings in United States dollars (USD) per million tokens.
Decimal arithmetic converts them to integer nanodollars without binary floating-point rounding.
Every price records its source, verification time, effective interval and conditions.

## Before you begin

1. Obtain Indy's approval for the named environment and vault reads.
   Development approval does not authorize production.
2. Install Bun 1.4.2 and a Postgres command-line client, `psql`.
3. Authenticate the 1Password command-line interface.
4. Confirm the selected vault item has `migrator-connection-string`.
   Development uses `planetscale-dev`; production uses `planetscale-prod`.
5. Check every model's exact context limit against its provider's models API.
   A missing exact limit is reported as `NOT SEEDED` and excludes that identity.

Apply is currently blocked for published cache-write prices whose lifetime is unspecified.
Resolve the recorded cache-write decision before applying this branch to an environment.
The seeder refuses to discard those rates.

## Steps

1. Read the development diff.

   ```bash
   ACTION=diff ENV=dev ALLOW_VAULT_READS=1 ./playbooks/operations/model_catalogue/00_gate.sh
   ```

   Expected output includes `NOT SEEDED` warnings for unreadable contexts and `<varies> added; <varies> changed; <varies> deleted`.
   The final line is `✅ model-catalogue diff complete (env: dev)`.

2. After approval, apply the reviewed development snapshot.
   This deletes unlisted identities and price rows.
   Type `dev` at the confirmation prompt.

   ```bash
   ACTION=apply ENV=dev ALLOW_VAULT_READS=1 ALLOW_MODEL_CATALOGUE_WRITES=1 ./playbooks/operations/model_catalogue/00_gate.sh
   ```

   Expected output ends with `✅ model-catalogue apply complete (env: dev)`.
   A mismatched confirmation stops before the database write.

3. For production, obtain separate approval and run the diff with `ENV=prod`.
   Read that diff before invoking the apply arm with `ENV=prod`.
   Type `prod` at its confirmation prompt.

## Verify it works

`03_verify.sh` requires at least one identity with a current base price.
It prints `✅ model.platform_models has <varies> currently priced row(s) in env: <varies>`.
Its database URL stays in the environment rather than process arguments.

Run the diff again after applying.
Expected output includes `0 added; 0 changed; 0 deleted`.
Repeated unchanged applies preserve identifiers, timestamps and the catalogue generation.

## Common problems

| Symptom | Cause | Action |
|---|---|---|
| Exit 2 before any step | Missing or invalid `ACTION`, `ENV`, or write approval | Name one environment and obtain its approval |
| Confirmation refused | Typed environment differs from the selected target | Rerun against the intended environment |
| Missing 1Password field | The selected vault item lacks the connection string | Repair the vault item |
| `NOT SEEDED` | Exact provider context evidence is absent | Read the provider models API; never infer an integer from marketing shorthand |
| Refresh failed | Invalid data, unavailable database or a refused write | Review the input and database health, then rerun the diff |
| Zero currently priced rows | Priming failed or no proved identity has a current price | Repair the snapshot and rerun the guarded apply |

The seeder makes no automatic retries.
A transaction failure keeps the preceding catalogue and generation intact.
No credentials are included in its failure output.

## Remove or undo

Correct the reviewed snapshot and run a newly approved apply.
Unlisted identities and prices are deleted together in that transaction.
This playbook never drops an environment database.

## Related pages

- [Schema layout](../../../docs/architecture/postgres_schemas.md)
- [Catalogue implementation and open decisions](../../../docs/v2/active/M229_001_P1_API_DOCS_INFRA_POSTGRES_SCHEMA_CLEAN_SLATE.md)
- [Model catalogue issue](https://github.com/agentsfleet/agentsfleet/issues/771)
