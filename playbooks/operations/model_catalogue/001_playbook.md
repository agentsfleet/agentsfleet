# Model Catalogue Priming

**Owners:** 🤠 Indy reads the diff, authorizes, and types the target; 🦉 Orly
executes and verifies.
**Scope:** exactly one of development or production per run.

`core.model_library` ships EMPTY by design and is populated from the curated
allowlist in `scripts/model-library-allowlist.json`. Every fleet needs a model,
so an environment with an empty catalogue is deployed but not usable — the
dashboard renders a model picker with nothing in it.

This wraps `scripts/seed-models.mjs`, which fetches published pricing from each
provider, diffs it against the live catalogue, and emits INSERTs for a fresh
catalogue or UPSERTs for drift. One code path serves both, so the monthly rate
refresh is exercised from day one rather than rotting as a rarely-run branch.

It is an operations playbook rather than a founding step on purpose: rate
refresh is recurring maintenance. The deployment steps
(`founding/04_deploy_dev`, `founding/07_deploy_prod`) reference it; they do not
own it.

**These rows are billing rates.** The write is therefore gated the same way the
destructive playbooks are — an approval variable, a vault-read approval, one
environment per invocation, and a typed confirmation — which is a higher bar
than the local `make seed-models APPLY=1` target it replaces for shared
environments.

## Before running

1. Confirm the target with 🤠 Indy. Production approval is separate from
   development approval.
2. Confirm authenticated 1Password Command-Line Interface (CLI) access.
3. Confirm `node` and `psql` are available.
4. Confirm the selected vault item has `migrator-connection-string`:
   - development: `planetscale-dev`
   - production: `planetscale-prod`

## Execute

Always read the diff first. The apply arm runs it again before writing, so
nothing is written that was not just displayed.

Development:

```bash
ACTION=diff ENV=dev ALLOW_VAULT_READS=1 \
  ./playbooks/operations/model_catalogue/00_gate.sh

ACTION=apply ENV=dev ALLOW_VAULT_READS=1 ALLOW_MODEL_CATALOGUE_WRITES=1 \
  ./playbooks/operations/model_catalogue/00_gate.sh
```

Production:

```bash
ACTION=diff ENV=prod ALLOW_VAULT_READS=1 \
  ./playbooks/operations/model_catalogue/00_gate.sh

ACTION=apply ENV=prod ALLOW_VAULT_READS=1 ALLOW_MODEL_CATALOGUE_WRITES=1 \
  ./playbooks/operations/model_catalogue/00_gate.sh
```

The apply arm prompts for the environment name and refuses any other answer.

## Configure inference after pricing

Freshly seeded rows contain rates and context metadata. A row also needs saved
inference settings before setup chat or runtime can use it. This is a separate
operator write: obtain approval for the target environment before saving.

1. Sign in to the target dashboard with `model:admin` and open `/admin/models`.
2. Edit the intended provider/model row. Provider and model identity stay locked.
3. Set **Inference API** to the provider's supported Chat completions, Responses
   or Messages API, and enter its **Inference base URL**. Enter the physical
   **Context window (tokens)** and **Model output limit (tokens)**. These are the
   model's actual limits; authored operating thresholds are separate.
4. Leave **SDK dialect (optional)** blank for the standard API, or enter a
   supported Software Development Kit (SDK) dialect for that provider. The server
   validates the route and dialect. Preserve the current rates unless a rate
   change was also approved.
5. Select **Save changes**, reload the page and reopen the row to check that the
   settings persisted. The underlying update is `PATCH /v1/admin/models/{id}`;
   its body includes the current rate/context fields and the `inference` object
   (`api`, `base_url`, optional `dialect`, `max_output_tokens`). Omitting
   `inference` preserves the existing settings.
6. Confirm the workspace has a credentialed registration for this model or an
   enabled platform model. New fleet uses those eligible models; setup chat
   additionally requires the selected SDK route to support tools. Saving
   catalogue metadata alone does not provision credentials or select a default.

Implementation references: `rustd/crates/afd_wire/src/inference.rs`,
`rustd/crates/afd_api_wire/src/admin_catalogue.rs`, and
`ui/packages/app/app/(dashboard)/admin/models/components/EditModelDialog.tsx`.

## Required result

- The diff arm prints the catalogue delta and writes nothing.
- The apply arm writes only after the typed confirmation matches.
- `core.model_library` reports a non-zero row count, which `03_verify.sh`
  asserts and names.
- Applying twice leaves the same row count — the generated statements upsert.

## Failure modes

| Symptom | Cause | Action |
|---|---|---|
| Exit 2 before anything runs | `ACTION`/`ENV` absent, unknown, or `ENV=all` | Re-invoke naming one environment explicitly |
| Exit 2 on apply | `ALLOW_MODEL_CATALOGUE_WRITES` unset | Read the diff first, then re-invoke with the approval |
| Confirmation refused | Typed name did not match the target | Nothing was written; re-invoke against the intended environment |
| `missing 1Password field` | Selected vault item lacks `migrator-connection-string` | Repair the vault item; do not fall back to a local URL |
| Verify reports 0 rows | Priming skipped or silently failed | Run the apply arm; the environment is not usable until this passes |
