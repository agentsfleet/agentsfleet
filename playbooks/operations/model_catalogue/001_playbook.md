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
