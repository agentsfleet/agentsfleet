# Data-Plane IP Allowlisting

**Updated:** Jul 31, 2026
**Owners:** 🤠 Indy approves cost and edits the Upstash dashboard; 🦉 Orly
validates inventory, applies PlanetScale restrictions, and verifies both
providers.

Only Fly.io control-plane egress belongs in these allowlists. An
`agentsfleet-runner` holds no Postgres or Redis credential and reaches only the
agentsfleet API, so a runner-host address must not be added.

## Prerequisites

Fly.io outbound addresses are unstable by default. For each environment, 🤠
Indy first approves one app-scoped static egress allocation in region `iad`;
🦉 Orly then runs:

```bash
fly ips allocate-egress --app agentsfleetd-dev --region iad
fly ips allocate-egress --app agentsfleetd-prod --region iad
```

Store each returned IPv4 address as a `/32` JSON array:

| Vault | Item | Fields |
|---|---|---|
| `ZMB_CD_DEV` | `fly-egress-ips` | `cidrs`, `updated-at` |
| `ZMB_CD_PROD` | `fly-egress-ips` | `cidrs`, `updated-at` |

`updated-at` is a Coordinated Universal Time timestamp such as
`2026-07-31T10:00:00Z`. Inventory older than seven days fails the gate.

Provider management fields:

| Vault | Item | Fields |
|---|---|---|
| `ZMB_CD_DEV` | `planetscale-dev` | `organization`, `database`, `service-token` |
| `ZMB_CD_PROD` | `planetscale-prod` | `organization`, `database`, `service-token` |
| `ZMB_CD_DEV` | `upstash-dev` | `db-id`, `developer-api-email`, `developer-api-key`, `allowlist-cidrs`, `allowlist-verified-at` |
| `ZMB_CD_PROD` | `upstash-prod` | `db-id`, `developer-api-email`, `developer-api-key`, `allowlist-cidrs`, `allowlist-verified-at` |

The PlanetScale service token needs `read_database` and `write_database`.

## Handoff

| Order | Owner | Action |
|---|---|---|
| 1 | 🦉 Orly | Run the read-only inventory and target check. |
| 2 | 🤠 Indy | Review the exact development and production targets. |
| 3 | 🤠 Indy | Approve provider writes by setting `ALLOW_PROVIDER_WRITES=1`. |
| 4 | 🦉 Orly | Apply the idempotent PlanetScale restriction. |
| 5 | 🤠 Indy | In each Upstash database, enable **IP Allowlisting** and set the exact `fly-egress-ips/cidrs` values. |
| 6 | 🤠 Indy | Copy those exact ranges to `allowlist-cidrs` and record the current time in `allowlist-verified-at`. |
| 7 | 🦉 Orly | Run provider verification. |

Upstash’s documented Developer API exposes whether IP allowlisting is enabled,
but not the exact configured ranges. The vault fields are therefore the
explicit human attestation; verification requires an exact inventory match and
a timestamp no older than seven days.

## Run

```bash
export ALLOW_VAULT_READS=1

# Read-only inventory and target separation.
ACTION=check ./playbooks/operations/ip_allowlisting/00_gate.sh

# After 🤠 Indy reviews the targets and approves provider writes.
export ALLOW_PROVIDER_WRITES=1
ACTION=apply ./playbooks/operations/ip_allowlisting/00_gate.sh

# Re-check later without provider mutation.
ACTION=verify ./playbooks/operations/ip_allowlisting/00_gate.sh
```

Use `ENV=dev` or `ENV=prod` to scope a run; the default checks both.

## Required result

- Development and production database identifiers differ.
- PlanetScale has exactly one unrestricted role/schema entry whose ranges equal
  the current Fly.io IPv4 inventory.
- Upstash reports `securityAddons.ipWhitelisting=true`.
- The Upstash attestation exactly matches the current Fly.io inventory.
- No provider credential appears in process arguments or output.

Provider references:

- [Fly.io app-scoped egress addresses](https://fly.io/docs/networking/egress-ips/)
- [PlanetScale IP restriction API](https://planetscale.com/docs/api/reference/list_database_postgres_cidrs)
- [Upstash IP allowlisting](https://upstash.com/docs/redis/features/security#ip-allowlisting)
- [Upstash database inspection API](https://upstash.com/docs/devops/developer-api/redis/get_database)
