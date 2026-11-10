---
type: how-to
audience: operator
verified: 2026-11-08
product_version: 0.75.0
executable: false
---

# Infrastructure Priming

## What this does

The Human and Agent create empty development and production infrastructure.
No application code deploys here, and no public domain is proven live.

| Surface | Development | Production |
|---|---|---|
| Fly.io organization | `agentsfleet-dev` | `agentsfleet-prod` |
| Fly.io API app | `agentsfleetd-dev` | `agentsfleetd-prod` |
| Fly.io tunnel app | `cloudflared-dev` | `cloudflared-prod` |
| API domain | `api-dev.agentsfleet.net` | `api.agentsfleet.net` |
| Dashboard domain | `app-dev.agentsfleet.net` | `app.agentsfleet.net` |
| 1Password items | `planetscale-dev`, `nats-dev`, `clerk-dev` | `planetscale-prod`, `nats-prod`, `clerk-prod` |
| Cloudflare R2 item | `cloudflare-r2` | `cloudflare-r2` |

## Before you begin

The `bootstrap` gate in [step 02](../02_preflight/001_playbook.md) passes for `ENV=all`.

## Steps

### 1. Create Fly.io organizations, apps and tokens

The Agent creates one organization per environment, so access to one grants nothing in the other:

```bash
fly orgs create agentsfleet-dev
fly orgs create agentsfleet-prod
fly apps create agentsfleetd-dev --org agentsfleet-dev
fly apps create cloudflared-dev --org agentsfleet-dev
fly apps create agentsfleetd-prod --org agentsfleet-prod
fly apps create cloudflared-prod --org agentsfleet-prod
```

The deploy workflows create `otelcol-<env>` and `nats-<env>` themselves with `scripts/ensure_fly_app.sh --create-only`.

A new organization refuses to deploy until it can pay.
Both organizations are linked to the Billing Organization `personal`, which holds the prepaid credits.
The Human links each one at `https://fly.io/dashboard/<ORG>/billing` → **Link billing to another Organization**.

> **Warning:** Never delete `personal`. It holds no apps, and it pays for both environments.
> Linking is irreversible, so a deleted billing organization cannot be relinked.

Fly organizations cannot be renamed. To move apps, create the organization, then run `fly apps move`.

A deploy token works only in the organization that minted it; billing links grant no access.
Mint one token per organization:

```bash
fly tokens create org --org agentsfleet-dev  --name agentsfleet-dev-ci  --expiry 8760h
fly tokens create org --org agentsfleet-prod --name agentsfleet-prod-ci --expiry 8760h
```

`--org` must be a flag.
`fly tokens create org agentsfleet-dev` silently mints against the default organization.
A token from the wrong organization fails every deploy with `Could not find App "<NAME>"`.

Write each token straight from the command output into 1Password, never into a log, history or file:

| Vault | Item and field | Organization |
|---|---|---|
| `ZMB_CD_DEV` | `fly-api-token/credential` | `agentsfleet-dev` |
| `ZMB_CD_PROD` | `fly-api-token/credential` | `agentsfleet-prod` |

Once both pipelines are green, revoke a superseded token.
`fly tokens list --org personal --scope org` names it; `fly tokens revoke <ID>` removes it.

The checked-in app definitions under `deploy/fly/` are canonical: `agentsfleetd-<env>/fly.toml`, `cloudflared-<env>/`, `otelcol-<env>/` and `nats-<env>/`.
The release workflow scales `agentsfleetd-prod` to exactly three machines and fails unless all three run.

### 2. Create datastores

The Human creates separate development and production PlanetScale databases, then stores:

```text
planetscale-<env>/api-connection-string
planetscale-<env>/migrator-connection-string
nats-<env>/api-url
```

Create each PlanetScale Postgres database in `us-east` (AWS us-east-1), the metro of Fly `iad`.
A region cannot change once a branch exists; moving one is [`database_region_move`](../../operations/database_region_move/001_playbook.md).

| Field | Port | Target |
|---|---|---|
| `api-connection-string` | `6432` | PlanetScale PgBouncer: same host and credentials, transaction pooling |
| `migrator-connection-string` | `5432` | Postgres directly; the migrator holds a session advisory lock |

Read the cluster's `max_connections` under **Clusters → Parameters** before sizing.
Every replica's `DATABASE_POOL_SIZE_API` (set in `fly.toml`), the migrator during a release, operator sessions and PgBouncer's `default_pool_size` (default 20) must fit inside it.
Past that ceiling Postgres refuses with `53300 too_many_connections`.

The daemon then reports a stall, `UZ-INTERNAL-001` (`the pool held <N> of <M>`), not an outage.

The NATS `api-url` is the cluster seed on Fly's private network: `nats://agentsfleet:<PASSWORD>@nats-<env>.internal:4222`.
The daemon and every NATS node read this one credential; there is no second, root-level NATS credential.

Never apply schema files by hand.
The migration runner applies `schema/*.sql` in order during deployment, starting with `schema/20261108120000_schemas.sql`.

### 3. Create Cloudflare tunnels

In Cloudflare Zero Trust, the Human creates tunnels `agentsfleetd-dev` and `agentsfleetd-prod`.
Copy each one-time tunnel token straight into `ZMB_CD_DEV/cloudflare-tunnel-dev/credential` or `ZMB_CD_PROD/cloudflare-tunnel-prod/credential`.
Route `api-dev.agentsfleet.net` to the `agentsfleetd-dev` tunnel and `api.agentsfleet.net` to `agentsfleetd-prod`.

The tunnel origins are pinned in [`ARCHITECTURE.md`](../../ARCHITECTURE.md).

### 4. Configure Clerk claims

In each Clerk instance, the Human sets the default session-token claims:

```json
{
  "aud": "<API_URL>",
  "scopes": "{{user.public_metadata.scopes}}",
  "metadata": {
    "tenant_id": "{{user.public_metadata.tenant_id}}"
  }
}
```

`<API_URL>` is `https://api-dev.agentsfleet.net` or `https://api.agentsfleet.net`.
It must equal the `OIDC_AUDIENCE` literal in `deploy-dev-fly.yml` or `release.yml`.
Sign out and back in so a new JSON Web Token (JWT) carries the claims.

### 5. Allocate static Fly.io egress

The Human approves the billed addresses first. Then the Agent runs:

```bash
fly ips allocate-egress --app agentsfleetd-dev --region iad
fly ips allocate-egress --app agentsfleetd-prod --region iad
```

Store each address as a `/32` JSON array in that vault's `fly-egress-ips/cidrs`.
Store the current Coordinated Universal Time (UTC) in `fly-egress-ips/updated-at`.

### 6. Set repository variables

```bash
gh variable set VAULT_DEV --body ZMB_CD_DEV --repo agentsfleet/agentsfleet
gh variable set VAULT_PROD --body ZMB_CD_PROD --repo agentsfleet/agentsfleet
gh variable set FLY_APP_DEV --body agentsfleetd-dev --repo agentsfleet/agentsfleet
gh variable set FLY_APP_PROD --body agentsfleetd-prod --repo agentsfleet/agentsfleet
gh variable set DEV_RUNNER_READY --body false --repo agentsfleet/agentsfleet
gh variable set PROD_RUNNER_READY --body false --repo agentsfleet/agentsfleet
```

Both runner switches stay `false` until their bootstrap gates pass.
That lets the first control-plane deployment run before any runner exists.

## Verify it works

- `fly apps list --org agentsfleet-dev` and `--org agentsfleet-prod` each show the API and tunnel apps.
- Development and production databases are distinct.
- Both tunnels exist and route their API domain.
- Clerk audiences match the workflow `OIDC_AUDIENCE` literals.
- Both vaults hold a recent `fly-egress-ips` inventory.
- `DEV_RUNNER_READY` and `PROD_RUNNER_READY` are `false`.
- The `deployment` gate in [step 02](../02_preflight/001_playbook.md#steps) exits 0 for `ENV=all`.

Continue to [step 04](../04_deploy_dev/001_playbook.md).
