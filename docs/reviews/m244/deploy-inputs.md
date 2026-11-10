---
type: how-to
audience: operator
verified: 2026-11-09
product_version: 0.75.0
executable: false
---

# M244 deploy: what Indy provides

## What it is

This page lists every input Indy supplies when M244 deploys, in order.
Everything else is in the Pull Request and its workflows.
It follows the decisions recorded in the [M244 spec](../../v2/done/M244_001_P1_DOCS_INFRA_OBS_CORE_NATS_REPLACES_DRAGONFLY.md#discovery-consult-log).

## Why it exists

M244 replaces Dragonfly with core NATS and moves durable state to Postgres.
The cutover stops the old daemons and resets dev data, so the order matters.

## When it can deploy

M244 does not deploy before three things land, in this order.

1. M243 (#819) merges, because M244 builds on its Bus.
2. M244's NATS verdict passes the signed-off bar on the laptop. A failure produces a rubric report for Indy instead of a deploy.
3. The M244 Pull Request is green, with Greptile resolved, and merged.

None of these is done yet, so a deploy this morning is not possible.

## Inputs, in order

| # | When | Input | Why |
|---|---|---|---|
| 1 | Before the deploy | Create 1Password item `nats-dev` with field `api-url`: `nats://agentsfleet:<password>@nats-dev.internal:4222`, in the dev runtime vault | The daemon and both NATS nodes read this one credential |
| 2 | Before the prod release | Create `nats-prod/api-url` the same way in the prod runtime vault, with its own password | Same, for prod |
| 3 | At the dev deploy | Run the deploy workflow; it creates `nats-dev` (two `shared-cpu-1x` 512 MB machines, `iad`, no volume) before the daemon | NATS must answer before the daemon boots, or boot refuses with `UZ-STARTUP-004` |
| 4 | Before you merge #820 | Follow the [database teardown playbook](../../../playbooks/operations/teardown/database/001_playbook.md) in full: (a) set the repository variable `DEV_RUNNER_READY` to `false`; (b) in a normal terminal, run `ALLOW_VAULT_READS=1 ALLOW_DATABASE_TEARDOWN=1 ENV=dev ./playbooks/operations/teardown/database/00_gate.sh` and type `DEVELOPMENT`; (c) merge #820, which deploys dev; (d) re-register the runner with [founding step 05](../../../playbooks/founding/05_runner_bootstrap_dev/001_playbook.md), which sets `DEV_RUNNER_READY` back to `true`; (e) prime the model catalogue the wipe emptied, as the gate prints | Dev applied `runner_runners.sql` and `fleet_obligations.sql` before M243 and M244 edited them, so the migrator refuses both (`UZ-STARTUP-005`). One reset covers both (Indy, Nov 10, 2026: "Reset once after #820 merges"). Every push to `main` deploys dev, so (a) and (b) come before the merge; a runner deployed onto the empty database keeps its old token, and the daemon refuses it with `UZ-RUN-001`. In-flight logins restart (accepted Nov 08, 2026) |
| 5 | After the dev deploy | Watch the drill: one `fly machine stop` on a `nats-dev` machine; two viewers get `catching_up` and resume within 10 s | Dimension 8.2 |
| 6 | At the prod release | Run the release; it creates `nats-prod` (two `shared-cpu-1x` 512 MB machines, `iad`, no volume) and migrates before the first new daemon starts | Dimension 8.1 |
| 7 | Once no old daemon runs | `fly apps destroy dragonfly-dev` and `fly apps destroy dragonfly-prod` | Dimension 8.3; you run it, as authorised |
| 8 | After the destroy | Delete the `DRAGONFLY_URL` items from both vaults | Nothing reads them after M244 |

## Machines

| App | Machines | Size | Volume |
|---|---|---|---|
| `dragonfly-dev` today | 1, four processes on it | `shared-cpu-1x`, 1 GB | 10 GB |
| `dragonfly-prod` today | 1, four processes on it | `performance-2x`, 4 GB | 10 GB |
| `nats-dev` | 2 | `shared-cpu-1x`, 512 MB | none |
| `nats-prod` | 2 | `shared-cpu-1x`, 512 MB | none |

Core NATS keeps no data, so a restarted node rejoins empty.
Sizes are re-checked from api-dev measurements after the dev deploy.

## Related pages

- [Dragonfly inventory](dragonfly-inventory.md)
- [Datastore scaling](../../architecture/datastore_scaling.md)
