---
type: how-to
audience: operator
verified: 2026-11-11
product_version: 0.76.0
executable: false
---

# Register Upstash QStash

## What this does

Stores one environment's QStash credentials as the `qstash` bag, so schedules fire into `agentsfleetd`.
Each environment uses its own credentials; finish development acceptance before production.
Indy owns Upstash settings and 1Password; the agent syncs and verifies.

| Environment | Destination |
|---|---|
| Development | `https://api-dev.agentsfleet.net/v1/ingress/qstash/schedules` |
| Production | `https://api.agentsfleet.net/v1/ingress/qstash/schedules` |

## Before you begin

The environment's [admin bootstrap](../admin_bootstrap/001_playbook.md) is complete, `/readyz` passes and the public schedule ingress is reachable.

## Steps

1. From one Upstash account and region, Indy copies the token, both signing keys and the API base that account shows.
   Never infer a regional hostname or mix values from different accounts.
2. Indy stores the `qstash` fields: `token`, `current-signing-key`, `next-signing-key`, and `url` (the HTTPS API base, such as `https://<host>/v2`, with no query or fragment).
   `agentsfleetd` refuses any other `url` and names the field, because the bearer rides every call under it.
   Only a loopback host may use plain `http`, for the local emulator.
3. The agent syncs `qstash` with the [shared bag procedure](../admin_bootstrap/001_playbook.md#sync-a-provider-secret-bag).
   Every deploy re-syncs it too (`deploy-dev-verify.yml`, `release.yml`).
   `agentsfleetd` reads the bag each time it registers a schedule or verifies a fire, so no restart is needed.

Nothing reads `QSTASH_*` environment settings.
`agentsfleetd` logs a missing or incomplete bag as `scheduler_bag_unusable` and names the field at fault.

Each delivery carries a JSON Web Token (JWT) that `agentsfleetd` checks against the current and next keys.
See [QStash signing keys](https://upstash.com/docs/qstash/api-reference/signing-keys/get-signing-keys) and [QStash schedules](https://upstash.com/docs/qstash/features/schedules).

## Verify it works

1. Indy creates a disposable scheduled fleet in the dashboard.
2. The agent confirms the provider schedule's destination is the exact URL from the table.
3. The agent confirms `fleet.schedules` shows `desired_status=active` and `sync_status=synced` for the current `resource_version`.
4. The agent sees one signed delivery, and confirms a bad signature and a replay create no fleet event.
5. Indy removes the schedule; the agent confirms the provider schedule is gone.

## Rotate the signing keys

1. Rotate once in Upstash.
2. Copy the new current and next keys to 1Password and rerun the sync.

The daemon reads the new pair on the next fire. Never rotate twice before step 2: the bag must keep overlap with the provider's active key pair.
