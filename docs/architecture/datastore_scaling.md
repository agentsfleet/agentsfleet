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
| Required target | Four self-hosted Dragonfly processes, one region, must pass the application and recovery tests before this work is complete. Dragonfly Cloud Swarm is the later move, not the first one (Indy, 2026-09-14). |
| Existing deployment | Redis is not a supported backend. Dragonfly, cluster-only, is the datastore; the cutover is recorded in the M192_001 Discovery (Indy, 2026-09-12). |
| Rust client | redis-rs `cluster_async` over RESP3 through the existing operation-specific boundaries is the only transport; primaries only; there is no standalone path and no topology selector. |
| Evidence | Compare the local cluster before and after each change on the same rig and workload; the Redis baseline (closed PR #681) is a historical reference point only. |
| Scale | Report measured capacity against an explicit workload; fleet population alone proves no throughput guarantee. |

## What it is

This page records the approved requirements for datastore scaling and the target design the implementation builds toward. It does not claim that Dragonfly support or million-fleet capacity has shipped; the sections below marked *target* describe what the active spec builds, and the runtime pages (`data_flow.md`, `runner_fleet.md`) describe what runs today until a Section lands and updates them in the same commit.

The [datastore scaling roadmap](./roadmap.md#datastore-scaling) links the implementation spec.

## Why it exists

The datastore carries event streams, readiness hints, authentication state, and outbound delivery. Those uses require different retention and failure rules. Treating all of them as disposable cached values would weaken accepted-work recovery.

Four self-hosted Dragonfly processes in one region are the near-term destination; Dragonfly Cloud Swarm is where this moves when operational risk justifies its control plane. There is no Dragonfly single-shard migration stage. Changing that destination requires an explicit user decision — this one was made on 2026-09-14 and is recorded below.

## Destination: self-hosted first, Swarm later (Indy, 2026-09-14)

Swarm was the recorded destination until this decision. What changed is not the
data plane — it is the price of the control plane, and when it is worth paying.

Four processes are not a cheap Swarm. They are four processes that need their
own cluster controller. Dragonfly's own cluster-mode documentation says
multi-shard server mode does not manage failover, health, rebalancing or
consistent topology configuration; Swarm is what supplies those. So a
self-hosted deployment buys the data plane and owes the operations, and that
trade is only sound while an unattended node loss is survivable.

It is survivable here for one reason: PostgreSQL is authoritative for what was
accepted, what completed and what delivery is still owed, and Dragonfly
accelerates discovery and delivery without holding an obligation of its own.
Losing a node costs throughput and erases no work — but it can erase
*discoverability*, and that is the gap Dimension 7.1 closes. The readiness
index has two writers: ingress marks a fleet as it admits, and the reclaim
sweeper re-marks one when the *stream* says it still holds work. A fleet whose
lease expired while its stream was lost is marked by neither, and nothing
re-leases it until a runner polls it, which nothing prompts. 7.1 adds the first
PostgreSQL-driven writer: a question the sweeper asks the ledger — does this
fleet owe work? — answered with a mark and never with a lease flip, since
`reclaim_prior_active` re-leases from PostgreSQL alone and needs the lease still
`active`. With that writer in place, every key the daemon holds in Dragonfly is
rebuildable from the ledger: the streams from `core.fleet_admissions` (replay
and reconcile), the outbound stream from `core.fleet_obligations` (the
producer's scan), readiness from admissions and leases (7.1), while `once`
markers may be lost because the lease path's conflict arm absorbs a
double-append and the hub holds nothing. `rebuild()` composes those scans into
one entry point, each looped to zero changed — the operator's tool for tearing
the cluster down and repopulating it, and the routine 7.8's flush test runs.
One class is a decision rather than a rebuild: `session` holds single-use
approval codes with no ledger row, so a flush ends a pending approval and the
user re-requests; 7.8 asserts that outcome rather than hiding it. **Dimension
7.8 is not yet proven**, and it is the proof this whole decision rests on. Until it is green,
this decision rests on a designed property rather than a measured one, and
nothing should carry production traffic on it.

Move to Swarm when the cost of an unattended failover exceeds the difference
between the two bills.

### Two containers, cross-paired (Indy, 2026-09-15)

Every requirement above says four *processes*, and until this decision nothing
said how many hosts or containers carry them — which is what decides what a node
loss costs. Asked and answered by Indy on 2026-09-15; recorded here because
deployment shape belongs to this page, and it seeds the cutover spec M192 defers
this to.

The local rig packs all four into ONE container on purpose, and that choice does
not carry: it exists so every node can announce `127.0.0.1` and have a `MOVED`
reply resolve from the host as well as from inside (see *What the local rig does
not prove* above).

| option | shape | failure behaviour |
|---|---|---|
| A | four containers, one process each, across hosts or zones in one region | each failure domain holds one node; a host loss costs one node |
| B | two containers, CROSS-paired — `dfly-a` with `b-replica`, `dfly-b` with `a-replica` | halves the count and no primary shares a domain with its OWN replica |
| C | two containers, naively paired — each primary beside its own replica | reads as four nodes and fails as two: a replica dies with the primary it covers, so replication buys nothing |

**B is chosen.** Two containers, `dfly-a` beside `b-replica` and `dfly-b` beside
`a-replica`, so a container loss costs one primary and one unrelated replica and
never a primary together with the replica that covers it. C is the shape to
refuse: it is the one that looks like the others on a diagram and fails as two
nodes. Budget roughly 512 MiB per node plus overhead — the same per-node floor
the rig enforces.

The shard count is unchanged by this: two shards from the first deployment, not
one. A single shard would leave `MOVED`, cross-node sharded pub/sub and the
hub's `SUNSUBSCRIBE` reconcile dormant in production until the day they all
arrive at once during a scale-out, and that reconcile is a path §0 measured as
required rather than optional.

### What the local rig does not prove

`scripts/dragonfly-cluster.sh` runs all four nodes in ONE container's network
namespace, every node announcing `127.0.0.1`, because a multi-container layout
makes a `MOVED` reply name an address that does not resolve from the host. That
is the right trade for deterministic local addressability and it is why the lane
is reliable. It also means the rig proves neither host failure nor real DNS
between nodes. Proving those needs four actual machines with announced internal
hostnames, on the platform that will run them — a staging lane this repository
does not have yet, and a separate milestone from M192.

### A node can abort on its own (2026-09-14)

Dragonfly v1.40.2 took a `SIGABRT` mid-run on the local cluster:
`db_slice.cc:1176 Check failed: res.is_new`, through
`CreateGroup → OpCreate → DbSlice::AddNew`. The node died; the other three kept
running and every suite then failed with `Connection refused`. A 64-way
concurrent `XGROUP CREATE … MKSTREAM` does NOT reproduce it — Redis 7.4.11 and
Dragonfly v1.40.2 both answer that correctly with one `OK` and 63 `BUSYGROUP` —
so the trigger is not group creation alone. The same window carried slot
migration and `DFLYCLUSTER FLUSHSLOTS`, which is the leading unproven candidate.

Recorded because it bears on the decision above: a node here can die from an
internal assertion on a routine command path, not only from infrastructure. A
self-hosted deployment must therefore restart a dead process automatically and
must not assume node loss is rare. Second observed occurrence; the first is in
the M192_001 session record.

#### Root cause found: `XGROUP CREATE … MKSTREAM` over an occupied key (2026-09-15)

The entry above names slot migration and `DFLYCLUSTER FLUSHSLOTS` as the leading
candidate. **That candidate is wrong.** Two commands abort Dragonfly v1.40.2, on
one node, with no cluster involvement, no migration and no concurrency:

    SET occupied notastream
    XGROUP CREATE occupied g $ MKSTREAM
    -> F db_slice.cc:1176 Check failed: res.is_new   -> SIGABRT -> Exited (134)

`MKSTREAM` is the whole differential. The same key answers correctly for every
neighbouring shape, node healthy each time:

| command over a key already holding a string | answer |
|---|---|
| `XGROUP CREATE occupied g $` (no `MKSTREAM`) | `WRONGTYPE`, node survives |
| `XADD occupied * f v` | `WRONGTYPE`, node survives |
| `XGROUP CREATE occupied g $ MKSTREAM` | **`SIGABRT`** |

So `MKSTREAM` reaches `DbSlice::AddNew` to create the key, `AddNew` finds one
already there, and the fatal `CHECK` fires where the type check should have
returned `WRONGTYPE`. The occupying key's TYPE does not matter — a list aborts
the node exactly as a string does — so no variant of the test keeps its
assertion and leaves the node alive. Redis answers `WRONGTYPE` here, which is
why the test is correct and this server cannot honour it. That is a server assertion reachable from ordinary client
input, on the newest release — there is no version to upgrade into.

How it was found, recorded so the path is not re-walked: `datastore_suite` run
serially (`--test-threads=1`) against a freshly recreated container names the
offender directly —
`integration_streams::test_a_group_create_that_is_not_a_race_is_reported` is the
last test to start and every test after it fails `Connection refused`. That test
occupies the stream key with a string on purpose, to assert the `WRONGTYPE` is
reported. It is correct and the server is not.

**Three earlier explanations are dead, and none should be re-derived:** that a
consumer group carried across a migration breaks a later idempotent
`XGROUP CREATE` (it answers `BUSYGROUP`, probed); that `MKSTREAM` on a new key in
a freshly received slot is the trigger (it answers `OK`, probed twice); and that
a node stays fragile for seconds after a migration settles (an abort was observed
14 minutes after the last migration, and then on a container that had never
migrated at all). The `Flushing newly unowned slots` line that made migration
look implicated **also fires at bootstrap**, when the first cluster configuration
is pushed, which is why it appeared before every abort.

The operational consequence above is unchanged: a self-hosted deployment must
restart a dead process automatically. What changes is the exposure. `create_group`
in `afd_dragonfly` (`streams.rs`, `outbound.rs`) always passes `MKSTREAM`, so any
path that leaves a non-stream value at a fleet stream key or at the outbound
stream key turns a routine group create into a node kill. Nothing in the daemon
writes those keys with another type today; the risk is that nothing prevents it
either.

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

**One boundary, one transport.** `afd_dragonfly` (renamed from `afd_redis`; the `REDIS_*` environment names are unchanged) keeps its surfaces — `Redis`, `Dedicated`, `FleetStreams`, `ReadyIndex`, `SessionStore`, `SubscriptionHub` — over one `ClusterConnection` per process for ordinary commands. There is no `REDIS_TOPOLOGY` and no standalone backend: a seed that is not a cluster refuses boot. A blocking consumer (`Dedicated`) holds its OWN `ClusterConnection`, never a borrowed one, because the driver keeps exactly one socket per node and applies one reply deadline to every command on a connection — a parked read on a shared handle would stall the owning node's only socket and impose the park-sized deadline on every other caller. Business crates never learn the topology, and no business code branches on it. Correctness-sensitive commands go to primaries; `read_from_replicas` is never enabled.

**Acceptance is a PostgreSQL row, the queue is a receipt.** Every producer (steer, webhook, schedule fire, gate continuation, install, outbound answer) commits an admission row carrying its producer identity before any success response. The single-key `XADD` follows and is recorded back as the physical receipt; a replay dispatcher re-appends admitted rows that never got one. Duplicate producer identity is a unique-index conflict, which is what the Redis dedup claim used to be. Settlement and billing key on the admission row, so a replayed receipt cannot debit twice. Losing the in-memory datastore loses no accepted work while PostgreSQL survives, and that takes two recovery passes rather than one: the two paragraphs below are the second.

**Delivery is the ledger's second timestamp, and it is what makes the recovery claim hold.** A receipt alone cannot decide whether accepted work still needs running. `core.fleet_admissions.delivered_at` records that a runner was handed the event, stamped by the same lease write that opens the narrative log in `core.fleet_events`. With `receipt` it is the table's whole status vocabulary, and both are NULL tests rather than a `status` text column, which would be a free-text restatement of them with nothing validating the spelling. `receipt IS NULL` is admitted and never queued, which the replay sweeper re-appends. `receipt IS NOT NULL AND delivered_at IS NULL` is accepted work the queue holds that nothing has run — the set a flush destroys, and the set that sweeper's scan cannot see, because those rows have their receipts. Both readers ride one partial index, `idx_fleet_admissions_undelivered (fleet_id, created_at, seq) WHERE receipt IS NOT NULL AND delivered_at IS NULL`, so a healthy deployment's index holds only the work currently in flight, and asking the same question across `core.fleet_events` instead would be a join no index can bound. The stamp keys on the logical event id and NOT on the receipt: a replayed admission puts one logical event on two stream entries while the ledger records only the first receipt, so a receipt-keyed stamp would miss the second entry's delivery and leave the row unstamped forever. It runs on both arms of the lease path's insert, because a first delivery that commits the narrative row and then fails to stamp takes the conflict arm on redelivery and would never be stamped otherwise; the statement's own `delivered_at IS NULL` guard is what makes running it twice free.

**Reconciliation forgets a lost receipt instead of appending a second time.** `afd_admission::Admissions::reconcile` asks one question per fleet holding undelivered work — does the stream still hold that fleet's OLDEST undelivered receipt (`afd_dragonfly::FleetStreams::holds_entry`, an `XRANGE` existence probe) — and walks a fleet row by row only where the answer is no, because a rebuilt stream can already hold new entries that are perfectly alive. A receipt the stream cannot produce is data loss and never housekeeping: the trim floor is bounded below by unfinished work, an undelivered entry sits above the group's last delivered id, a stream with no group is not trimmed at all, and nothing else in this daemon deletes an entry. The repair sets `receipt` back to NULL, which returns the row to the state the replay sweeper already scans, so the crate keeps ONE append path rather than a second that would have to get fencing, receipts, `replay_count` and the readiness mark right all over again. A probe the datastore will not answer changes nothing, because voiding on an unreadable stream would re-append work the stream may still hold. Voided rows re-enter the deployment's replay backlog and count against it, which is honest — the work is genuinely owed again — and is why the pass takes a row cap per fleet: recovery arrives over several passes, and the backlog rises in steps the sweeper can drain. The daemon runs the pass as its own supervised sweeper (`sweeper:admission-reconcile`) rather than as a step in the replay dispatcher's, because it costs a datastore round trip per fleet where replay costs none: it waits five minutes after a pass that voided nothing, and thirty seconds after one that did.

**No multi-key operation exists.** The at-most-once append was the crate's only two-key script, and it is GONE: when acceptance became a PostgreSQL row the marker key it hash-tagged onto the stream — `{fleet:<id>:events}:<scope-prefix><once-id>` — had nothing left to claim, so the script, its `OnceScope`, and the sweep that expired its keys were deleted rather than ported. Every script the crate still owns touches exactly one key: token-checked readiness clear, increment-in-window, and session verify/approve/abort. `grep -o 'KEYS\[[0-9]\]' | sort -u` over `afd_dragonfly/src` answers `KEYS[1]` and nothing else.

That deletion also freed the stream keys. `fleet_stream_key` returns a plain `fleet:<id>:events` with no hash tag, because nothing has to co-locate with it any more — so fleets distribute across primaries by their own hash instead of being pinned into one slot by a tag that existed to serve a script that no longer runs. A new multi-key script is refused by the cluster suite.

**Sharded pub/sub.** The hub subscribes with `SSUBSCRIBE` and the tail publishes with `SPUBLISH`. Channel names (`fleet:<id>:activity`) and the SSE parser are unchanged; the channel routes by its own slot. Measured on the local cluster: a slot migration strands the subscription — the server pushes `sunsubscribe` and the new owner counts zero subscribers — so the hub re-issues `SSUBSCRIBE` on that push and after a reconnect, with no duplicate local frame.

**Readiness is partitioned.** The index is sixteen hashes, `fleet:ready:{p}` for `p` in `0..16`, the number as a hash tag so each partition is one slot. A fleet's partition is CRC16-XMODEM of its id modulo sixteen (`afd_dragonfly::ready::Partition::of`), the checksum the cluster keys slots by, computed identically by the ingress that marks, the poll that clears and the sweeper that re-marks. Sixteen is the count the prototype measured: with one partition holding 4,000 ready fleets and the other fifteen three each, a rotation of sixteen polls at a budget of eight found 45 of 45 cold fleets and no poll read past its budget, where the single hash under the same budget found one or two. Each lease poll reads ONE partition — the next in a rotation kept by one `ReadyCursor` per process, shared by every clone of the lease store, so every poll made through the process advances the same rotation whichever runner makes it — under the existing 64-candidate ceiling; an idle poll is still one bounded round trip. The token-checked clear stays a single-key script on the fleet's partition. The capacity report counts partitions and marks separately. The key family is named rather than hard-coded — `afd_dragonfly::ready::ReadyPrefix` — and `production()` returns `fleet:ready` unchanged and is the only constructor compiled into the daemon. The `test-util` `private(name)` mints `fleet:ready:private:<name>` so a suite can own an index no other writer touches; it exists because the empty-poll property is unobservable on the shared one, where a lane that has seeded fleets across all sixteen partitions leaves no partition empty to measure. The partition number stays the hash tag whatever the prefix, so a private index spreads over the cluster exactly as the production one does and a test proves the real routing.

**Outbound delivery is fair per destination.** The outbound stream stays one stream read in order, but the worker no longer delivers what it reads: each job goes to the lane for its destination (provider plus workspace, `afd_outbound::Destination`) and the reader moves on. A lane delivers serially, so answers to one workspace leave in the order they were queued; lanes are independent, so a workspace whose vendor is slow holds only its own; and at most eight deliveries (`IN_FLIGHT_DELIVERIES`) are in flight across every lane. A lane exists only while it holds work and retires under the same lock a dispatch takes to find it, which is what keeps order across the hand-over to a fresh lane. A lane holds at most thirty-two jobs (`LANE_DEPTH`); past that the dispatch waits and the stream buffers, so a destination that will not take a job stalls unrelated answers only after that many of its own have queued. Cancellation stops a lane between jobs: the delivery in hand finishes and is acknowledged, everything still queued stays pending for the next process's pending-first read.

**Retention is bounded below by unfinished work.** An append carries no `MAXLEN`. On every acknowledgement `afd_dragonfly::streams::retain` computes a floor — the least of the group's last delivered id, its oldest pending id, and the id 1,000 entries from the tail — and issues `XTRIM MINID` there, so a pending or undelivered entry is never removed and acknowledged history keeps at most 1,000 entries; every race between the reads and the trim leaves the floor lower, never higher. The outbound stream is trimmed the same way. A consumer group lost on the cluster is reported by the read (`is_group_missing`) and recreated by the lease path at the newest receipt the admission ledger and `core.fleet_events` jointly prove was delivered (`afd_admission::Admissions::delivered_cursor`) — never at `$`, which loses accepted work, and never at `0`, which re-runs delivered entries because a redelivery only skips its receive debit. Admission budgets refuse before the row is committed: a fleet with 10,000 outstanding entries (pending plus undelivered, from `XINFO GROUPS`) and a deployment with 100,000 unreceipted rows. The deployment figure is an ESTIMATE and not a count. It was a `count(*)` over the partial index folded into the admission `INSERT`, which read well and walked every waiting row on every accepted event: the index keeps that walk off the table but not off the rows, so the check cost most exactly when the deployment was furthest behind, and a valve that grows heavier the harder it is pressed is the wrong shape for the path every producer takes. `afd_admission::budget::ceiling` now reads the figure occasionally — on the first admission of a process, every 1,000 admissions, every five seconds, and every 500 milliseconds while the estimate is refusing so a drained backlog clears — holds it behind an `Arc` every clone of the ledger shares, and lets the hot path add the admissions that process has committed since. A row leaves the counted state only when a receipt is recorded, so the sum runs HIGH and never low on that process's own traffic: the refusal arrives early rather than late, which is the direction a safety valve errs in. Two gaps stay, both bounded and both closed by the next read — receipts recorded since the read are not yet subtracted, and a sibling replica's admissions are in no local counter — and the ledger's own `receipt IS NULL` count stays the authority the replay sweeper and the cardinality lane report. An `OOM` reply and a SQLSTATE class-53 refusal are their own class (`is_full`, `is_over_capacity`), counted as `over_budget`/`full` and answered with the 503 producers already retry on. Boot refuses a primary reporting `cache_mode:true` or a `maxmemory_policy` other than `noeviction`. The replay sweeper publishes `agentsfleet_admission_backlog` and its oldest age each pass, and the cardinality lane reports streams, retained and pending entries, readiness partitions and marks, primaries, replicas and the ledger backlog as separate figures.

**Local rig.** The compose service `dragonfly` runs four cluster processes in one network namespace announced on `127.0.0.1` with host ports equal to announced ports, bootstraps replication and `DFLYCLUSTER CONFIG` on every start, and is reset only when ownership is proven. A fifth process in the same container, `--cluster_mode=emulated --tls`, serves the TLS trust proof alone; ordinary suites take the plaintext cluster so hundreds of connects do not each pay a handshake. There is no Redis service.

## How it behaves

The evidence sequence is: prototype on the local cluster, integrate the proven shape behind the boundary, prove cluster behaviour in the lane, then Dragonfly Cloud Swarm. Redis parity is not a gate, because Redis is not a supported backend.

Dragonfly documents disk snapshots and no append-only log, so a crash loses whatever followed the last recoverable snapshot and a restart may come back with data that is stale rather than absent. The design has to survive both: an obligation PostgreSQL still holds is recovered whatever the datastore came back with, and a stale entry for work already terminal is acknowledged rather than re-run. Work accepted by the target design remains recoverable after the in-memory datastore is lost, provided the durable database survives. This includes pending inbound work and queued outbound answers. Third-party side effects retain their documented retry and idempotency limits; this page makes no exactly-once promise for external services.

Deploy implementation increments to the same application services while retaining their Dragonfly endpoint. Record an immutable revision and configuration for every comparison. Changes in instance count, database capacity, region, payload distribution, or background traffic must be disclosed and separated from code improvements.

The cutover to the cluster is a fresh start: PostgreSQL is rebuilt and Dragonfly provisioned empty, and nothing is carried across. There is therefore no import tool and no import receipt, and a daemon boots against a datastore that passes the three suitability refusals and nothing more. What the design owes instead is everything accepted after that point, through every ordinary restart and deployment: PostgreSQL records what was accepted, what completed and what delivery is still owed; Dragonfly is rebuildable from it, so losing Dragonfly's contents erases no obligation and a snapshot restoring stale entries re-runs nothing already terminal; a deployment replaces application processes and neither datastore; and a retry reuses the operation's identity, because a timeout does not prove an operation failed. A failed connection must not silently fall back to a standalone path or an empty queue.

## Limits

One million stored fleets, active fleets, concurrent runs, and runner processes are separate workload dimensions. Reports must state all four, together with offered load, completion rate, queue age, memory, and database cost. Small deployment probes prove behavior at their tested load; they do not prove the million-fleet target.

Fault injection and saturation testing use isolated datastores and synthetic destinations. Shared deployments use bounded fixture traffic and cleanup that only removes the run's own records. No shared Dragonfly flush, database reset, failover exercise, or provider switch follows from spec approval alone.

The historical Redis baseline (closed PR #681) is a reference for the local rig only. Cloud evidence is required before the Dragonfly acceptance rows are complete; missing Cloud evidence leaves them incomplete.

## Upstash retirement status

The datastore cutover ran in M196_001. Its runbook and probe runner were
deleted once it had served its purpose; this section is what survives it,
because one half of the move is finished and the other has not started.

**Development — retired.** `agentsfleetd-dev` has served on the `dragonfly-dev`
cluster since Sep 17, 2026. The `upstash-dev` vault item was archived out of
`ZMB_CD_DEV` the same day on Indy's named approval, and Indy deleted the hosted
Upstash database itself from the provider console. The daemon proves the
cluster on every boot regardless. `rustd/crates/afd_dragonfly/src/preflight.rs`
issues `INFO CLUSTER` and refuses a seed that answers as a single server;
`rustd/crates/agentsfleetd/src/serve/runtime.rs` calls it before the daemon
serves anything, so a machine that reaches healthy has passed that check.

**Production — not started, and `upstash-prod` stays until it is.** There is no
`dragonfly-prod` app, and `agentsfleetd-prod` has never deployed. Deleting
`upstash-prod` now would remove the only rollback for a move that has not
happened. It becomes eligible when `dragonfly-prod` exists and is bootstrapped,
`agentsfleetd-prod` deploys and serves against it, and `release.yml` resolves no
Upstash seed — then on a fresh named approval, not before.

Both deploy pipelines still gate on `scripts/dragonfly_cluster_ready.sh`, which
asks the cluster whether it is bootstrapped rather than trusting Fly's TCP
check. That is not cutover scaffolding and did not retire with it.

**QStash is a different product and is NOT retired.** It remains the cron
trigger. Mentions of Upstash in the cron sections of these pages are correct and
`scripts/check_architecture_doc.sh` requires them.

## Related pages

- [Scaling](./scaling.md) describes existing capacity assumptions that measurements must verify.
- [Data flow](./data_flow.md) and [runner fleet](./runner_fleet.md) describe the running topology; each Section updates them with its code.
- [Test isolation on a shared datastore (rules ISO-1 to ISO-3)](./testing.md#test-isolation-on-a-shared-datastore-rules-iso-1-to-iso-3) governs isolation on the shared lane datastores.
- [Dragonfly cluster mode](https://github.com/dragonflydb/dragonfly/blob/main/docs/cluster-mode.md), [compatibility](https://www.dragonflydb.io/docs/command-reference/compatibility), and [AOF](https://www.dragonflydb.io/docs/managing-dragonfly/aof) are checked against the pinned image.
