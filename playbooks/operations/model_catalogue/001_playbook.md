---
type: how-to
audience: operator
verified: 2026-11-08
product_version: 0.75.0
executable: false
---

# Model catalogue priming

## What this does

Applies the curated catalogue in `scripts/model-library-allowlist.json` to one environment.
The seeder writes identities and inference connections to `model.platform_models` and dated prices to `model.platform_model_prices`.
It uses Bun's native Postgres client.

- A connection holds the API, base URL, optional SDK dialect and output token limit.
- Re-priming fills missing connections and keeps saved administrator overrides.
- An apply deletes identities and prices absent from the reviewed snapshot.
- Identity changes, price changes and the catalogue generation commit in one transaction; a refused write rolls back all three.
- A repeated identical apply writes nothing.
- Prices are decimal strings in United States dollars (USD) per million tokens, converted to integer nanodollars without floating-point rounding.
- Each price records its source, verification time, effective interval and conditions.

## Before you begin

1. Get Indy's approval for the named environment and vault reads; development approval does not cover production.
2. Install the repository-pinned Bun version.
3. Sign in to the 1Password command-line interface.
4. Confirm the vault item has `migrator-connection-string`: `planetscale-dev` for development, `planetscale-prod` for production.
5. Check each model's exact context limit against its provider's models API.
   An identity without one is reported as `NOT SEEDED` and left out.
6. Keep every supplied cache-write rate: unspecified-lifetime, five-minute and one-hour.

## Steps

1. Read the development diff.

   ```bash
   ACTION=diff ENV=dev ALLOW_VAULT_READS=1 ./playbooks/operations/model_catalogue/00_gate.sh
   ```

   ```text
   ✓ [models] <n> curated identities; <n> verified contexts; Ollama pricing unknown
   <n> added; <n> changed; <n> deleted
   ✅ model-catalogue diff complete (env: dev)
   ```

   A `NOT SEEDED` line appears only for an identity whose exact context limit is unproved.

2. After approval, apply the reviewed snapshot, which deletes unlisted identities and price rows.
   Type `dev` at the confirmation prompt.

   ```bash
   ACTION=apply ENV=dev ALLOW_VAULT_READS=1 ALLOW_MODEL_CATALOGUE_WRITES=1 ./playbooks/operations/model_catalogue/00_gate.sh
   ```

   ```text
   ✅ model.platform_models has <n> currently priced row(s) in env: dev
   ✅ model-catalogue apply complete (env: dev)
   ```

   A mismatched confirmation writes nothing.

3. For production, get separate approval, run both commands with `ENV=prod`, and type `prod` at the prompt.

## Activate a primed model

Priming does not add credentials or select a default.

1. Sign in with `model:admin` and open **Platform → Model library** (`/admin/models`).
2. Select the row's default star, enter the provider API key and confirm.
3. Open **New fleet** and confirm the model is in the picker.
   Setup chat also needs an SDK route that supports tools.

## Change connection settings or prices

1. Edit the model. Provider and model identity stay locked.
2. Review **Connection**. Primed **Inference API**, **Inference base URL** and **Model output limit (tokens)** rarely change; **Dialect (optional)** picks a provider SDK dialect.
3. Expand **Catalogue details** to change the display name, context evidence or **Advanced model**.
   Advanced models appear only when a user asks for the expanded picker.
4. Expand **Pricing** to review dated prices and token thresholds.
   **Add pricing tier** adds a long-prompt rate or a later interval; a single-rate provider needs one base tier.
5. Select **Save changes**, then reload and reopen the row.

An API update that omits `inference` keeps the existing connection.
Saving adds no credential and selects no default.
Source: `rustd/crates/protocol/af_wire/src/inference.rs`, `rustd/crates/daemon/afd_api_wire/src/admin_catalogue.rs`, `ui/packages/app/app/(dashboard)/admin/models/components/EditModelDialog.tsx`.

## Verify it works

1. Confirm the apply printed the `currently priced row(s)` line from `03_verify.sh`.
   It needs at least one identity with a current base price.
2. Rerun the step 1 diff.

   ```text
   0 added; 0 changed; 0 deleted
   ```

Unchanged applies keep identifiers, timestamps and the catalogue generation.

## Common problems

| Symptom | Cause | Action |
|---|---|---|
| Exit 2 before any step | Missing or invalid `ACTION`, `ENV` or write approval | Name one environment and get its approval |
| Confirmation refused | Typed name differs from the target | Rerun against the intended environment |
| `missing 1Password field` | The vault item lacks the connection string | Repair the vault item |
| `NOT SEEDED` | No exact provider context evidence | Read the provider models API; never infer from marketing shorthand |
| Refresh failed | Invalid data, unavailable database or refused write | Check input and database health, then rerun the diff |
| `0 currently priced rows` | Priming failed or no proved identity has a current price | Repair the snapshot and rerun the apply |

The seeder does not retry.
A failed transaction keeps the prior catalogue and generation.
Failure output carries no credentials, and the database URL stays out of process arguments.

## Remove or undo

Correct the snapshot and run a newly approved apply.
This playbook never drops a database.

## Related pages

- [Schema layout](../../../docs/architecture/postgres_schemas.md)
- [Catalogue implementation and open decisions](../../../docs/v2/done/M229_001_P1_API_DOCS_INFRA_POSTGRES_SCHEMA_CLEAN_SLATE.md)
- [Model catalogue issue](https://github.com/agentsfleet/agentsfleet/issues/771)
