---
type: how-to
audience: operator
verified: 2026-11-08
product_version: 0.75.0
executable: false
---

# Tear down the Dragonfly cluster

## What this does

> **Warning:** this removes every stored key, including the `connector:outbound` stream and authentication session, approval and deduplication keys.

The gate sends `FLUSHALL` to each primary, then proves `DBSIZE` is 0 on every primary and replica.
Accepted inbound work and undelivered outbound obligations stay in Postgres.
Activity and wake channels are ephemeral, so their durable history survives.
Each run targets `ENV=dev` or `ENV=prod`; the gate refuses `all`.
🤠 Indy authorizes and types the target; 🦉 the agent runs and verifies.

Each cluster is one Fly app (`dragonfly-dev` or `dragonfly-prod`) on one machine.
It runs four members, two primaries with one replica each, from `scripts/dragonfly-cluster.sh`.
Members announce the private 6PN address, and the app has no public service, so the scripts run inside the machine over `flyctl ssh console`.
`cluster_ops.sh` reads members from `CLUSTER SHARDS` and refuses to act unless all four are listed, online, and answer `PING` within 5 seconds.
Nothing is read from the vault; the cluster password stays on the machine.
The fifth process is the integration lane's TLS test node; the teardown leaves it alone.

## Before you begin

1. Stop traffic and every `agentsfleetd` machine in the environment; live requests recreate keys and fail verification.
2. Authenticate with `flyctl auth login`, or export `FLY_API_TOKEN` from the environment's `fly-api-token` vault item.

## Steps

1. Run the gate; use `ENV=prod` for production.

   ```bash
   ALLOW_DRAGONFLY_TEARDOWN=1 ENV=dev ./playbooks/operations/teardown/dragonfly/00_gate.sh
   ```

2. Section 1 is read-only and lists the four members, proving the ssh route before anything is destroyed.
3. Section 2 asks you to type `DEVELOPMENT` or `PRODUCTION`, then flushes each primary.
4. Section 3 prints each member's key count and passes only when all are 0.

   ```text
     <IP>:<PORT> <ROLE> keys=0
   ✓ every member reports DBSIZE 0
     ✅ SUCCESS: every member is empty
   ✅ dragonfly-teardown complete (env: dev)
   ```

5. Restart or redeploy every `agentsfleetd` machine before restoring traffic.
   Startup recreates the `connector:outbound` group, and normal writes recreate session and deduplication keys.
   Lease polls keep reading the admissions ledger, and outbound rebuild retries undelivered obligations.
6. For a full platform rebuild, continue with founding step 04 (development) or 07 (production).

## Common problems

Section 1 refuses before section 2 flushes anything, so a refusal is never a partial teardown.

| Refusal | Fix |
|---|---|
| `the cluster names 3 member(s), not 4` | Repair the cluster, for example restart the Dragonfly machine, then rerun. |
| `member <IP>:<PORT> (replica) is fail, not online` | Same. |
| `... did not answer PING: no answer within 5s` | Same. |

`flyctl ssh console --command` parsing the quoted `sh -c '…'` has not run against a live Fly machine.
A wrong parse fails at section 1, before any flush.
On Oct 19, 2026, the full gate passed against a real four-member `dragonfly:v2.0.2` cluster, with ssh routed to `docker exec`.
`verify_test.sh`, run by `make check-playbooks`, covers the same refusals against a stubbed cluster.

## Remove or undo

To remove the cluster instead of emptying it, delete the environment's Dragonfly Fly app.
The next deploy recreates it from `deploy/fly/dragonfly-{dev,prod}/`; see [founding step 03](../../../founding/03_priming_infra/001_playbook.md).
