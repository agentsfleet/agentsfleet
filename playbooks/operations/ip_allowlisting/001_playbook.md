# Data-Plane IP Allowlisting

**Updated:** Sep 17, 2026
**Owners:** 🤠 Indy approves cost and provider writes; 🦉 Orly validates
inventory, applies PlanetScale restrictions, and verifies them.

Only Fly.io control-plane egress belongs in these allowlists. An
`agentsfleet-runner` holds no Postgres or datastore credential and reaches only
the agentsfleet API, so a runner-host address must not be added.

**One provider, since M196.** The datastore used to be the second: a hosted
service with a public endpoint, whose egress ranges had to be allowlisted in
its dashboard and attested in the vault because its API would report whether
allowlisting was on but not which ranges were set. Self-hosted Dragonfly has
no public endpoint. It runs on a Fly machine reached over 6PN, a private
network that admits nothing from outside it, so there is no range to allowlist,
no dashboard to edit and no attestation to keep fresh. The control was not
dropped — it was answered by where the datastore now runs.

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

The PlanetScale service token needs `read_database` and `write_database`.

## Handoff

| Order | Owner | Action |
|---|---|---|
| 1 | 🦉 Orly | Run the read-only inventory and target check. |
| 2 | 🤠 Indy | Review the exact development and production targets. |
| 3 | 🤠 Indy | Approve provider writes by setting `ALLOW_PROVIDER_WRITES=1`. |
| 4 | 🦉 Orly | Apply the idempotent PlanetScale restriction. |
| 5 | 🦉 Orly | Run provider verification. |

Steps 5 through 7 of the previous revision were the datastore's half: enabling
allowlisting in a provider dashboard, copying the ranges into the vault as a
human attestation, and checking that attestation was no more than seven days
old. All three are gone with the hosted store.

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
- No provider credential appears in process arguments or output.

Provider references:

- [Fly.io app-scoped egress addresses](https://fly.io/docs/networking/egress-ips/)
- [PlanetScale IP restriction API](https://planetscale.com/docs/api/reference/list_database_postgres_cidrs)
- [Fly.io private networking (6PN)](https://fly.io/docs/networking/private-networking/)
