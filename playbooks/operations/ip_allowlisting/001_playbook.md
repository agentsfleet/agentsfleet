---
type: how-to
audience: operator
verified: 2026-11-08
product_version: 0.75.0
executable: false
---

# Restrict PlanetScale ingress to Fly.io egress

## What this does

Restricts each PlanetScale database to the `agentsfleetd` Fly.io static egress addresses.
Add only control-plane egress; never add a runner-host address, because `agentsfleet-runner` holds no datastore credential.
Dragonfly needs no allowlist: it runs on Fly [private networking (6PN)](https://fly.io/docs/networking/private-networking/) with no public endpoint.

## Before you begin

1. 🤠 Indy approves one app-scoped static [egress allocation](https://fly.io/docs/networking/egress-ips/) per environment.
2. 🦉 The agent allocates it.

   ```bash
   fly ips allocate-egress --app agentsfleetd-dev --region iad
   fly ips allocate-egress --app agentsfleetd-prod --region iad
   ```

   ```text
   <varies: one IPv4 address per app>
   ```

3. Store each address as a `/32` JSON array in the vault `VAULT_DEV` or `VAULT_PROD` names.

   | Item | Fields |
   |---|---|
   | `fly-egress-ips` | `cidrs`, `updated-at` (UTC, such as `2026-07-31T10:00:00Z`) |
   | `planetscale-dev` / `planetscale-prod` | `organization`, `database`, `service-token` |

   The gate fails on inventory older than seven days (`MAX_EGRESS_AGE_DAYS`, default 7).
   The service token needs `read_database` and `write_database`.

## Steps

`ENV=dev` or `ENV=prod` scopes a run; unset checks both.

1. 🦉 The agent runs the read-only inventory and target check.

   ```bash
   export ALLOW_VAULT_READS=1
   ACTION=check ./playbooks/operations/ip_allowlisting/00_gate.sh
   ```

   ```text
   PASS: egress inventory
   PASS: provider targets
   PASS: IP allowlisting check completed for all
   ```

2. 🤠 Indy reviews the development and production targets and approves with `ALLOW_PROVIDER_WRITES=1`.
3. 🦉 The agent applies the idempotent [restriction](https://planetscale.com/docs/api/reference/list_database_postgres_cidrs), which then verifies.

   ```bash
   ALLOW_PROVIDER_WRITES=1 ACTION=apply ./playbooks/operations/ip_allowlisting/00_gate.sh
   ```

   ```text
   PASS: PlanetScale IP restrictions applied
   PASS: provider IP restrictions verified
   ```

## Verify it works

Re-check at any time without writing.

```bash
ACTION=verify ./playbooks/operations/ip_allowlisting/00_gate.sh
```

```text
PASS: development PlanetScale IP restrictions
PASS: production PlanetScale IP restrictions
PASS: provider IP restrictions verified
PASS: IP allowlisting verify completed for all
```

A pass means the two database identifiers differ and each has exactly one unrestricted role and schema entry equal to the Fly.io inventory.
No provider credential appears in process arguments or output.
