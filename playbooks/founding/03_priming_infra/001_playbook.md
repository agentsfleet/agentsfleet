# Infrastructure Priming

**Updated:** Jul 31, 2026
**Owner:** Human
**Executors:** Human handles external consoles; Agent creates scriptable
resources and verifies handoffs
**Prerequisite:** The `bootstrap` stage of
`playbooks/founding/02_preflight/00_gate.sh` passes for both environments.

This step creates empty development and production infrastructure. It does not
deploy application code or claim that the public domains are ready.

## Resource map

| Surface | Development | Production |
|---|---|---|
| Fly.io API app | `agentsfleetd-dev` | `agentsfleetd-prod` |
| Fly.io tunnel app | `cloudflared-dev` | `cloudflared-prod` |
| API domain | `api-dev.agentsfleet.net` | `api.agentsfleet.net` |
| Dashboard domain | `app-dev.agentsfleet.net` | `app.agentsfleet.net` |
| PlanetScale item | `planetscale-dev` | `planetscale-prod` |
| Upstash item | `upstash-dev` | `upstash-prod` |
| Clerk item | `clerk-dev` | `clerk-prod` |
| Cloudflare R2 item | `cloudflare-r2` | `cloudflare-r2` |

## Execution and verification

| Order | Executor | Action | Verifier | Required evidence |
|---|---|---|---|---|
| 1 | Human | Confirm provider billing and account access. | Human | Named accounts and organizations recorded. |
| 2 | Agent | Create six Fly.io apps. | Fly command-line query | All six app names resolve in the intended organization. |
| 3 | Human | Create two PlanetScale and two Upstash resources; store role-separated values. | Deployment gate | Both environments pass and database roles differ. |
| 4 | Human | Create two Cloudflare tunnels, route API domains, and store tokens. | Agent | Tunnel identifiers and DNS routes recorded without token values. |
| 5 | Human | Configure Clerk session claims. | Pipeline | Authenticated development acceptance passes after deployment. |
| 6 | Human | Approve billed Fly.io static egress. | Human | Approval recorded before allocation. |
| 7 | Agent | Allocate egress and set repository variables with runners disabled. | Agent | Recent IPv4 inventories and repository variable values recorded. |
| 8 | Agent | Run the deployment credential gate. | `00_gate.sh` | Green output for development and production. |

## 1. Create Fly.io apps

```bash
fly apps create agentsfleetd-dev --org agentsfleet
fly apps create cloudflared-dev --org agentsfleet
fly apps create otelcol-dev --org agentsfleet
fly apps create agentsfleetd-prod --org agentsfleet
fly apps create cloudflared-prod --org agentsfleet
fly apps create otelcol-prod --org agentsfleet
```

The checked-in deployment definitions are canonical:

- `deploy/fly/agentsfleetd-dev/fly.toml`
- `deploy/fly/agentsfleetd-prod/fly.toml`
- `deploy/fly/cloudflared-dev/`
- `deploy/fly/cloudflared-prod/`
- `deploy/fly/otelcol-dev/`
- `deploy/fly/otelcol-prod/`

The two collector apps carry the OTLP hop the daemon exports through: the
daemon addresses `http://otelcol-<env>.internal:4318` and the collector holds
the Grafana Cloud credentials and forwards. They are listed here because the
deploy workflows DEPLOY them but never CREATE them — `flyctl secrets set --app
otelcol-dev` is the first thing that addresses the app, and it fails on an app
that does not exist. Omitting these two lines is what leaves the development
deploy red at "Ensure the OTLP collector is running".

The release workflow sets the production API app to exactly three machines and
fails unless all three are running before it checks the tunnel and public API.

Only `agentsfleetd` owns Postgres, Redis, and the vault. A host-resident
`agentsfleet-runner` reaches the API over HTTPS and is bootstrapped later.

## 2. Create data stores

The Human creates separate development and production resources, then stores:

```text
planetscale-{env}/api-connection-string
planetscale-{env}/migrator-connection-string
upstash-{env}/api-url
upstash-{env}/url
```

The two PlanetScale strings must differ. The migrator connection must be a
direct session connection on port `5432`; a transaction pooler cannot hold the
session advisory lock used by migrations.

The Upstash `api-url` is the restricted runtime connection. The root `url` is
reserved for the explicitly approved Redis teardown runbook.

Do not apply schema files manually. The checked-in migration runner applies
them in order during deployment and creates the current schemas:
`core`, `fleet`, `billing`, `audit`, `vault`, `ops_ro`, and `memory`.

## 3. Create Cloudflare tunnels

The Human creates `agentsfleetd-dev` and `agentsfleetd-prod` in Cloudflare Zero
Trust, copies each one-time tunnel token directly to the matching 1Password
item, and routes:

```text
api-dev.agentsfleet.net → agentsfleetd-dev tunnel
api.agentsfleet.net     → agentsfleetd-prod tunnel
```

Vault fields:

```text
ZMB_CD_DEV/cloudflare-tunnel-dev/credential
ZMB_CD_PROD/cloudflare-tunnel-prod/credential
```

The tunnel origins are already pinned in the checked-in configuration:

```text
agentsfleetd-dev.internal:3000
agentsfleetd-prod.internal:3000
```

No public `*.fly.dev` service is part of the supported topology.

## 4. Configure Clerk claims

In each Clerk instance, the Human sets the default session-token claims:

```json
{
  "aud": "<environment API URL>",
  "scopes": "{{user.public_metadata.scopes}}",
  "metadata": {
    "tenant_id": "{{user.public_metadata.tenant_id}}"
  }
}
```

Use `https://api-dev.agentsfleet.net` for development and
`https://api.agentsfleet.net` for production. The audience must match the
`OIDC_AUDIENCE` literal in the corresponding workflow. Sign out and back in
after saving so a new JSON Web Token (JWT) carries the claims.

## 5. Allocate stable Fly.io egress

After the Human approves the billed addresses, the Agent runs:

```bash
fly ips allocate-egress --app agentsfleetd-dev --region iad
fly ips allocate-egress --app agentsfleetd-prod --region iad
```

Store each returned IPv4 address as a `/32` JSON array in
`fly-egress-ips/cidrs`, with the current Coordinated Universal Time in
`fly-egress-ips/updated-at`. The allowlisting operation consumes these values.

## 6. Set repository variables

```bash
gh variable set VAULT_DEV --body ZMB_CD_DEV --repo agentsfleet/agentsfleet
gh variable set VAULT_PROD --body ZMB_CD_PROD --repo agentsfleet/agentsfleet
gh variable set FLY_APP_DEV --body agentsfleetd-dev --repo agentsfleet/agentsfleet
gh variable set FLY_APP_PROD --body agentsfleetd-prod --repo agentsfleet/agentsfleet
gh variable set DEV_RUNNER_READY --body false --repo agentsfleet/agentsfleet
gh variable set PROD_RUNNER_READY --body false --repo agentsfleet/agentsfleet
```

Keep both runner switches false until their bootstrap gates pass. This is what
allows the first control-plane deployment to run before either runner exists.

## Required result

- The six Fly.io apps exist in the `agentsfleet` organization.
- Development and production databases and Redis resources are distinct.
- Both Cloudflare tunnels exist and hold the intended API route.
- Clerk claim audiences match the workflow literals.
- Both environments have a recent static IPv4 egress inventory.
- `DEV_RUNNER_READY` and `PROD_RUNNER_READY` are false.
- `ENV=all STAGE=deployment ./playbooks/founding/02_preflight/00_gate.sh`
  exits zero.

Continue to `playbooks/founding/04_deploy_dev/001_playbook.md`.
