# Credential Rotation

**Owners:** 🤠 Indy rotates values in provider consoles and 1Password; 🦉 Orly
propagates them and verifies the selected environment.
**Trigger:** a credential expires, is exposed, or is deliberately replaced.

## 1. Select and quiesce

Choose exactly one environment:

```bash
export ENV=dev
# or: export ENV=prod
```

🤠 Indy pauses affected writes when the provider cannot keep old and new values
valid at the same time.

## 2. Rotate and store

🤠 Indy replaces the provider value, then updates its existing 1Password field.
For the datastore, one field:

- `dragonfly-{env}/api-url` — the cluster's seed on Fly's private network

There is no second, root-level connection to keep in step. The hosted store
had one, reserved for destructive teardown; a self-hosted cluster has a single
credential and is torn down by deleting its Fly app.

There is no runner Redis credential.

For a PostHog key, update `posthog-{env}/credential`. For Vercel deployment
protection, update the matching `vercel-bypass-*` item in `ZMB_CD_PROD`.
Never paste an old or new value into chat, a shell argument, or a log.

### The Fly API token is org-scoped, and that is its whole failure mode

`fly-api-token/credential` is the credential the PIPELINE uses, not one the
daemon runs with — the workflows read it as
`op://$VAULT_{DEV,PROD}/fly-api-token/credential` and every `flyctl` call in a
deploy authenticates with it. It is bound to the Fly organisation it was minted
in, so it stops working when the apps move rather than when it expires:

| Vault | Item and field | Organisation |
|---|---|---|
| `ZMB_CD_DEV` | `fly-api-token/credential` | `agentsfleet-dev` |
| `ZMB_CD_PROD` | `fly-api-token/credential` | `agentsfleet-prod` |

**Rotate it whenever an app changes organisation, not only on the usual
triggers.** Billing linkage does not help: linked organisations share credits
and nothing else.

```bash
fly tokens create org --org agentsfleet-dev  --name agentsfleet-dev-ci  --expiry 8760h
fly tokens create org --org agentsfleet-prod --name agentsfleet-prod-ci --expiry 8760h
```

`--org` is a flag, not a positional argument; `fly tokens create org
agentsfleet-dev` silently mints against the default organisation instead.
The value is printed once — put it straight into the 1Password field and into
no log, shell history or file.

**Read the failure correctly.** A blind token reports
`Could not find App "<name>"` — it names the app and never the token, so it is
indistinguishable from an app that genuinely does not exist. If an app is
visible to `fly apps list` under your own login but CI still cannot find it,
the token is scoped to the wrong organisation. `fly tokens list --org <org>`
shows which tokens exist where.

Revoke the superseded token with `fly tokens revoke <id>` only after both
lanes are green — never before, or a rollback has no way back in.

## 3. Propagate

If a Vercel value changed, inspect and apply the complete environment matrix:

```bash
ALLOW_VAULT_READS=1 \
  ./playbooks/founding/01_bootstrap/02_vercel_env.sh --check

ALLOW_VAULT_READS=1 \
ALLOW_VERCEL_WRITES=1 \
  ./playbooks/founding/01_bootstrap/02_vercel_env.sh --apply
```

Redeploy the selected API so its Fly secret values are refreshed from
1Password:

- development: run `deploy-dev.yml`
- production: rerun the authorized Release workflow for the current tag, or
  use the next authorized release

## 4. Verify

```bash
ALLOW_VAULT_READS=1 \
ENV="$ENV" \
  ./playbooks/operations/credential_rotation/00_gate.sh
```

The gate verifies the current vault fields, `/healthz`, `/readyz`, and the
selected dashboard without exposing credential values in process arguments.
