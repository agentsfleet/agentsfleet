---
type: explanation
audience: contributor
verified: 2026-09-13
product_version: 0.31.0
executable: false
---

# Datastore scaling requirements and target design

| Decision | Requirement |
|---|---|
| Required target | Dragonfly Cloud Swarm must pass the application and recovery tests before this work is complete. |
| Existing deployment | Redis is not a supported backend. Dragonfly, cluster-only, is the datastore; the cutover is recorded in the M192_001 Discovery (Indy, 2026-09-12). |
| Rust client | redis-rs `cluster_async` over RESP3 through the existing operation-specific boundaries is the only transport; primaries only; there is no standalone path and no topology selector. |
| Evidence | Compare the local cluster before and after each change on the same rig and workload; the Redis baseline (closed PR #681) is a historical reference point only. |
| Scale | Report measured capacity against an explicit workload; fleet population alone proves no throughput guarantee. |

## What it is

This page records the approved requirements for datastore scaling and the target design the implementation builds toward. It does not claim that Dragonfly support or million-fleet capacity has shipped; the sections below marked *target* describe what the active spec builds, and the runtime pages (`data_flow.md`, `runner_fleet.md`) describe what runs today until a Section lands and updates them in the same commit.

The [datastore scaling roadmap](./roadmap.md#datastore-scaling) links the implementation spec.

## Why it exists

Redis carries event streams, readiness hints, authentication state, and outbound delivery. Those uses require different retention and failure rules. Treating all of them as disposable cached values would weaken accepted-work recovery.

Dragonfly Cloud Swarm is the required destination. There is no Dragonfly single-shard migration stage. Changing that destination requires an explicit user decision. Redis support remains part of the acceptance requirements.

## What the cluster probe showed (2026-09-12)

A four-node Dragonfly v1.40.2 cluster (two primaries, one replica each, one network namespace, `127.0.0.1:7001-7004`) was driven with the daemon's current key and script shapes. The results decided the target design:

| Current shape | On the cluster | Consequence |
|---|---|---|
| `append_once`: one Lua over `webhook:dedup:<fleet>:<id>` and `fleet:<fleet>:events` | `CROSSSLOT Keys in request don't hash to the same slot` | Acceptance identity cannot live in a Redis claim beside the stream; it moves to PostgreSQL. |
| `fleet:ready`: one hash for the deployment | One slot, one node, every poll and every mark | Readiness is partitioned by fleet hash. |
| `PUBLISH`/`SUBSCRIBE` on `fleet:<id>:activity` | A cluster broadcasts every publish to every node | Sharded pub/sub (`SPUBLISH`/`SSUBSCRIBE`), routed by the channel's slot. |
| `SSUBSCRIBE` sent to a node that does not own the slot | Answered `OK`, not `MOVED` | Subscriber placement is proven under slot migration, not assumed from Redis behaviour. |
| `SCAN` for `auth:session:*` | Scans one node | Scans fan out over every primary from the live topology. |
| `DFLYCLUSTER CONFIG` with a `migrations` entry | 4096 slots moved and finalised; keys followed | Slot movement is a test fixture the owned local cluster can drive. |

## Target design

**One boundary, one transport.** `afd_datastore` (renamed from `afd_redis`; the `REDIS_*` environment names are unchanged) keeps its surfaces — `Redis`, `Dedicated`, `FleetStreams`, `ReadyIndex`, `SessionStore`, `SubscriptionHub` — over one `ClusterConnection` per process for ordinary commands. There is no `REDIS_TOPOLOGY` and no standalone backend: a seed that is not a cluster refuses boot. A blocking consumer (`Dedicated`) holds its OWN `ClusterConnection`, never a borrowed one, because the driver keeps exactly one socket per node and applies one reply deadline to every command on a connection — a parked read on a shared handle would stall the owning node's only socket and impose the park-sized deadline on every other caller. Business crates never learn the topology, and no business code branches on it. Correctness-sensitive commands go to primaries; `read_from_replicas` is never enabled.

**Acceptance is a PostgreSQL row, the queue is a receipt.** Every producer (steer, webhook, schedule fire, gate continuation, install, outbound answer) commits an admission row carrying its producer identity before any success response. The single-key `XADD` follows and is recorded back as the physical receipt; a replay dispatcher re-appends admitted rows that never got one. Duplicate producer identity is a unique-index conflict, which is what the Redis dedup claim used to be. Settlement and billing key on the admission row, so a replayed receipt cannot debit twice. Losing the in-memory datastore loses no accepted work while PostgreSQL survives, and that takes two recovery passes rather than one: the two paragraphs below are the second.

**Delivery is the ledger's second timestamp, and it is what makes the recovery claim hold.** A receipt alone cannot decide whether accepted work still needs running. `core.fleet_admissions.delivered_at` records that a runner was handed the event, stamped by the same lease write that opens the narrative log in `core.fleet_events`. With `receipt` it is the table's whole status vocabulary, and both are NULL tests rather than a `status` text column, which would be a free-text restatement of them with nothing validating the spelling. `receipt IS NULL` is admitted and never queued, which the replay sweeper re-appends. `receipt IS NOT NULL AND delivered_at IS NULL` is accepted work the queue holds that nothing has run — the set a flush destroys, and the set that sweeper's scan cannot see, because those rows have their receipts. Both readers ride one partial index, `idx_fleet_admissions_undelivered (fleet_id, created_at, seq) WHERE receipt IS NOT NULL AND delivered_at IS NULL`, so a healthy deployment's index holds only the work currently in flight, and asking the same question across `core.fleet_events` instead would be a join no index can bound. The stamp keys on the logical event id and NOT on the receipt: a replayed admission puts one logical event on two stream entries while the ledger records only the first receipt, so a receipt-keyed stamp would miss the second entry's delivery and leave the row unstamped forever. It runs on both arms of the lease path's insert, because a first delivery that commits the narrative row and then fails to stamp takes the conflict arm on redelivery and would never be stamped otherwise; the statement's own `delivered_at IS NULL` guard is what makes running it twice free.

**Reconciliation forgets a lost receipt instead of appending a second time.** `afd_admission::Admissions::reconcile` asks one question per fleet holding undelivered work — does the stream still hold that fleet's OLDEST undelivered receipt (`afd_datastore::FleetStreams::holds_entry`, an `XRANGE` existence probe) — and walks a fleet row by row only where the answer is no, because a rebuilt stream can already hold new entries that are perfectly alive. A receipt the stream cannot produce is data loss and never housekeeping: the trim floor is bounded below by unfinished work, an undelivered entry sits above the group's last delivered id, a stream with no group is not trimmed at all, and nothing else in this daemon deletes an entry. The repair sets `receipt` back to NULL, which returns the row to the state the replay sweeper already scans, so the crate keeps ONE append path rather than a second that would have to get fencing, receipts, `replay_count` and the readiness mark right all over again. A probe the datastore will not answer changes nothing, because voiding on an unreadable stream would re-append work the stream may still hold. Voided rows re-enter the deployment's replay backlog and count against it, which is honest — the work is genuinely owed again — and is why the pass takes a row cap per fleet: recovery arrives over several passes, and the backlog rises in steps the sweeper can drain. The daemon runs the pass as its own supervised sweeper (`sweeper:admission-reconcile`) rather than as a step in the replay dispatcher's, because it costs a datastore round trip per fleet where replay costs none: it waits five minutes after a pass that voided nothing, and thirty seconds after one that did.

**No multi-key operation exists.** The at-most-once append was the crate's only two-key script, and it is GONE: when acceptance became a PostgreSQL row the marker key it hash-tagged onto the stream — `{fleet:<id>:events}:<scope-prefix><once-id>` — had nothing left to claim, so the script, its `OnceScope`, and the sweep that expired its keys were deleted rather than ported. Every script the crate still owns touches exactly one key: token-checked readiness clear, increment-in-window, and session verify/approve/abort. `grep -o 'KEYS\[[0-9]\]' | sort -u` over `afd_datastore/src` answers `KEYS[1]` and nothing else.

That deletion also freed the stream keys. `fleet_stream_key` returns a plain `fleet:<id>:events` with no hash tag, because nothing has to co-locate with it any more — so fleets distribute across primaries by their own hash instead of being pinned into one slot by a tag that existed to serve a script that no longer runs. A new multi-key script is refused by the cluster suite.

**Sharded pub/sub.** The hub subscribes with `SSUBSCRIBE` and the tail publishes with `SPUBLISH`. Channel names (`fleet:<id>:activity`) and the SSE parser are unchanged; the channel routes by its own slot. Measured on the local cluster: a slot migration strands the subscription — the server pushes `sunsubscribe` and the new owner counts zero subscribers — so the hub re-issues `SSUBSCRIBE` on that push and after a reconnect, with no duplicate local frame.

**Readiness is partitioned.** The index is sixteen hashes, `fleet:ready:{p}` for `p` in `0..16`, the number as a hash tag so each partition is one slot. A fleet's partition is CRC16-XMODEM of its id modulo sixteen (`afd_datastore::ready::Partition::of`), the checksum the cluster keys slots by, computed identically by the ingress that marks, the poll that clears and the sweeper that re-marks. Sixteen is the count the prototype measured: with one partition holding 4,000 ready fleets and the other fifteen three each, a rotation of sixteen polls at a budget of eight found 45 of 45 cold fleets and no poll read past its budget, where the single hash under the same budget found one or two. Each lease poll reads ONE partition — the next in a rotation kept by one `ReadyCursor` per process, shared by every clone of the lease store, so every poll made through the process advances the same rotation whichever runner makes it — under the existing 64-candidate ceiling; an idle poll is still one bounded round trip. The token-checked clear stays a single-key script on the fleet's partition. The capacity report counts partitions and marks separately.

**Outbound delivery is fair per destination.** The outbound stream stays one stream read in order, but the worker no longer delivers what it reads: each job goes to the lane for its destination (provider plus workspace, `afd_outbound::Destination`) and the reader moves on. A lane delivers serially, so answers to one workspace leave in the order they were queued; lanes are independent, so a workspace whose vendor is slow holds only its own; and at most eight deliveries (`IN_FLIGHT_DELIVERIES`) are in flight across every lane. A lane exists only while it holds work and retires under the same lock a dispatch takes to find it, which is what keeps order across the hand-over to a fresh lane. A lane holds at most thirty-two jobs (`LANE_DEPTH`); past that the dispatch waits and the stream buffers, so a destination that will not take a job stalls unrelated answers only after that many of its own have queued. Cancellation stops a lane between jobs: the delivery in hand finishes and is acknowledged, everything still queued stays pending for the next process's pending-first read.

**Retention is bounded below by unfinished work.** An append carries no `MAXLEN`. On every acknowledgement `afd_datastore::streams::retain` computes a floor — the least of the group's last delivered id, its oldest pending id, and the id 1,000 entries from the tail — and issues `XTRIM MINID` there, so a pending or undelivered entry is never removed and acknowledged history keeps at most 1,000 entries; every race between the reads and the trim leaves the floor lower, never higher. The outbound stream is trimmed the same way. A consumer group lost on the cluster is reported by the read (`is_group_missing`) and recreated by the lease path at the newest receipt the admission ledger and `core.fleet_events` jointly prove was delivered (`afd_admission::Admissions::delivered_cursor`) — never at `$`, which loses accepted work, and never at `0`, which re-runs delivered entries because a redelivery only skips its receive debit. Admission budgets refuse before the row is committed: a fleet with 10,000 outstanding entries (pending plus undelivered, from `XINFO GROUPS`) and a deployment with 100,000 unreceipted rows (a count on the partial index, folded into the admission `INSERT`). An `OOM` reply and a SQLSTATE class-53 refusal are their own class (`is_full`, `is_over_capacity`), counted as `over_budget`/`full` and answered with the 503 producers already retry on. Boot refuses a primary reporting `cache_mode:true` or a `maxmemory_policy` other than `noeviction`. The replay sweeper publishes `agentsfleet_admission_backlog` and its oldest age each pass, and the cardinality lane reports streams, retained and pending entries, readiness partitions and marks, primaries, replicas and the ledger backlog as separate figures.

**Local rig.** The compose service `dragonfly` runs four cluster processes in one network namespace announced on `127.0.0.1` with host ports equal to announced ports, bootstraps replication and `DFLYCLUSTER CONFIG` on every start, and is reset only when ownership is proven. A fifth process in the same container, `--cluster_mode=emulated --tls`, serves the TLS trust proof alone; ordinary suites take the plaintext cluster so hundreds of connects do not each pay a handshake. There is no Redis service.

## How it behaves

The evidence sequence is: prototype on the local cluster, integrate the proven shape behind the boundary, prove cluster behaviour in the lane, then Dragonfly Cloud Swarm. Redis parity is not a gate, because Redis is not a supported backend.

Work accepted by the target design remains recoverable after the in-memory datastore is lost, provided the durable database survives. This includes pending inbound work and queued outbound answers. Third-party side effects retain their documented retry and idempotency limits; this page makes no exactly-once promise for external services.

Deploy implementation increments to the same application services while retaining their Dragonfly endpoint. Record an immutable revision and configuration for every comparison. Changes in instance count, database capacity, region, payload distribution, or background traffic must be disclosed and separated from code improvements.

Changing the selected datastore requires an explicit rollout action after evidence passes, and an import receipt the daemon checks before accepting work. A failed connection must not silently fall back to a standalone path or an empty queue.

## Limits

One million stored fleets, active fleets, concurrent runs, and runner processes are separate workload dimensions. Reports must state all four, together with offered load, completion rate, queue age, memory, and database cost. Small deployment probes prove behavior at their tested load; they do not prove the million-fleet target.

Fault injection and saturation testing use isolated datastores and synthetic destinations. Shared deployments use bounded fixture traffic and cleanup that only removes the run's own records. No shared Redis flush, database reset, failover exercise, or provider switch follows from spec approval alone.

The historical Redis baseline (closed PR #681) is a reference for the local rig only. Cloud evidence is required before the Dragonfly acceptance rows are complete; missing Cloud evidence leaves them incomplete.

## Related pages

- [Scaling](./scaling.md) describes existing capacity assumptions that measurements must verify.
- [Data flow](./data_flow.md) and [runner fleet](./runner_fleet.md) describe the running topology; each Section updates them with its code.
- [Test isolation on a shared datastore (rules ISO-1 to ISO-3)](./testing.md#test-isolation-on-a-shared-datastore-rules-iso-1-to-iso-3) governs isolation on the shared lane datastores.
- [Dragonfly cluster mode](https://github.com/dragonflydb/dragonfly/blob/main/docs/cluster-mode.md), [compatibility](https://www.dragonflydb.io/docs/command-reference/compatibility), and [AOF](https://www.dragonflydb.io/docs/managing-dragonfly/aof) are checked against the pinned image.
