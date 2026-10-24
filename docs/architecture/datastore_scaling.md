---
type: explanation
audience: contributor
verified: 2026-10-21
product_version: 0.62.2
executable: false
---

# Datastore scaling requirements and target design

| Decision | Requirement |
|---|---|
| Required target | Four self-hosted Dragonfly processes, one region, must pass the application and recovery tests before this work is complete. Dragonfly Cloud Swarm is the later move, not the first one (Indy, 2026-09-14). |
| Existing deployment | Redis is not a supported backend. Dragonfly, cluster-only, is the datastore; the cutover is recorded in the M192_001 Discovery (Indy, 2026-09-12). |
| Rust client | redis-rs `cluster_async` over RESP3 (version 3 of the Redis Serialization Protocol) through the existing operation-specific boundaries is the only transport; primaries only; there is no standalone path and no topology selector. |
| Evidence | Compare the local cluster before and after each change on the same rig and workload; the Redis baseline (closed PR #681) is a historical reference point only. |
| Scale | Report measured capacity against an explicit workload; fleet population alone proves no throughput guarantee. |

## What it is

This page records the approved requirements for datastore scaling and the target design the implementation builds toward. Dragonfly has served development since Sep 17, 2026 (§"Upstash retirement status"); production and million-fleet capacity have not shipped. §"Target design" is built: M192_001 shipped it and M196_001 moved development onto it. The runtime pages (`data_flow.md`, `runner_fleet.md`) describe the same running system from the request's side.

## Why it exists

Dragonfly carries live activity, wake notifications, authentication state, and outbound delivery. PostgreSQL holds the admissions queue and durable delivery obligations.

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

PostgreSQL records accepted work, completion and delivery obligations. Each lease poll reads the admissions queue, even when Dragonfly sends no notification.

An expired slot makes its unsettled admission eligible again. Recovery needs no stream reconstruction or readiness marks.

`rebuild()` (`rustd/crates/afd_runner/src/sweep/rebuild/mod.rs`) repairs outbound delivery from `fleet.obligations`.

Single-use session approvals have no durable row. Losing those keys requires the user to request another approval.
The cluster recovery test checks this distinction: `test_cluster_restart_and_stale_snapshot_preserve_obligations`.

Move to Swarm when the cost of an unattended failover exceeds the difference
between the two bills.

### Two containers, cross-paired (Indy, 2026-09-15)

**Not built.** Both Fly configurations run ONE machine with all four processes (`deploy/fly/dragonfly-dev/fly.toml`, `deploy/fly/dragonfly-prod/fly.toml`), so a machine loss today costs every node.

Every requirement above says four *processes*, and until this decision nothing
said how many hosts or containers carry them — which is what decides what a node
loss costs. Asked and answered by Indy on 2026-09-15; recorded here because
deployment shape belongs to this page, and it seeds the cutover spec M192 defers
this to.

The local rig packs all four into ONE container on purpose, and that choice does
not carry: it exists so every node can announce `127.0.0.1` and have a `MOVED`
reply resolve from the host as well as from inside (see *What the local rig does
not prove* below).

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

A self-hosted deployment must restart a dead process. The outbound consumer still creates its group with `MKSTREAM`.

Keep non-stream values out of its stream key. The inbound queue uses PostgreSQL and creates no Dragonfly group.

## What the cluster probe showed (2026-09-12)

A four-node Dragonfly v1.40.2 cluster (two primaries, one replica each, one network namespace, `127.0.0.1:7001-7004`) was driven with the daemon's current key and script shapes. The results decided the target design:

| Shape probed in September 2026 | On the cluster | Consequence |
|---|---|---|
| `append_once`: one Lua over `webhook:dedup:<fleet>:<id>` and `fleet:<fleet>:events` | `CROSSSLOT Keys in request don't hash to the same slot` | Acceptance identity cannot live in a Redis claim beside the stream; it moves to PostgreSQL. |
| `fleet:ready`: one hash for the deployment | One slot, one node, every poll and every mark | The admissions ledger replaces this index. Wake notifications use sixteen independent sharded channels. |
| `PUBLISH`/`SUBSCRIBE` on `fleet:<id>:activity` | A cluster broadcasts every publish to every node | Sharded pub/sub (`SPUBLISH`/`SSUBSCRIBE`), routed by the channel's slot. |
| `SSUBSCRIBE` sent to a node that does not own the slot | Answered `OK`, not `MOVED` | Subscriber placement is proven under slot migration, not assumed from Redis behaviour. |
| `SCAN` for `auth:session:*` | Scans one node | Scans fan out over every primary from the live topology. |
| `DFLYCLUSTER CONFIG` with a `migrations` entry | 4096 slots moved and finalised; keys followed | Slot movement is a test fixture the owned local cluster can drive. |

## Target design

`afd_dragonfly` uses redis-rs `cluster_async` over version 3 of the Redis Serialization Protocol (RESP3). Boot refuses a non-cluster seed.

Ordinary commands share one logical cluster client per process. Blocking outbound consumers own separate clients, so parked reads cannot stall ordinary commands.
The [connection budget](scaling.md#connection-budget-after-the-cutover) distinguishes client owners from physical node sockets.

### The admissions ledger is the queue

```text
producer -> commit fleet.admissions -> publish fleet:wake:{shard}
                         |
runner poll -> PostgreSQL head probe -> slot + admission claim
                         |
                  issue fenced lease
                         |
report -> usage + terminal event + settlement + delivery obligation
```

A producer commits its identity and payload before reporting acceptance. A failed wake publish leaves that acceptance intact.

Every poll probes PostgreSQL. The notification only prioritizes a recent fleet.

The candidate query first checks whether any unsettled admission exists. An empty ledger skips fleet scans.
When work exists, it reads each eligible fleet's oldest unsettled admission through `idx_admissions_unsettled` on `(fleet_id, sequence)`.
Admission holds the fleet row lock before allocating its sequence and through commit. A transaction that inserts later therefore stays behind unfinished work.

A parked head blocks later rows while standing access awaits an answer or an unresolved event approval has a future deadline.
Both discovery and claim probe these two scopes through `idx_approval_gates_pending_scope_deadline`.
One `NOT EXISTS` over `UNION ALL` keeps the probes correlated to the chosen head. Each branch limits its search to one blocker, including when many approvals match.
Expired event approvals remain answerable; the resumed gate evaluation decides whether to proceed or refuse.
A missing cached gate reference falls back to the durable event association, so a long polling outage cannot erase that decision.

The claim locks the slot and exact head with `SKIP LOCKED`, then checks whether that head can run and advances both fences together.
A blocked or unavailable head returns no work, and the caller rolls back the slot claim.

Claims retain held-sandbox preference and spread other candidates randomly. They provide order within each fleet, with no global waiting-time guarantee.

`delivered_at` records admission into runtime history. `settled_at` removes a row from the queue.
A report settles the admission in the transaction that records usage, terminal state, checkpoint, slot release and any outbound obligation.

An expired slot exposes the same unsettled head to another runner. The original billing identity survives redelivery.

The per-fleet budget serializes admission with a `NO KEY UPDATE` fleet lock and refuses at 10,000 unsettled admissions.
That lock permits the foreign-key checks needed to record delivery concurrently.

The deployment ceiling estimates 100,000 unsettled admissions. It refreshes every 1,000 admissions or five seconds, and every 500 milliseconds while refusing.

Each process adds its own accepted admissions between samples. Concurrent replicas can overshoot this estimate; the per-fleet limit remains serialized.
The census publishes `agentsfleet_admission_backlog` and `agentsfleet_admission_backlog_oldest_age_seconds` from PostgreSQL, withdrawing them when its query fails.

### Dragonfly notifications and outbound delivery

The live tail uses sharded publish and subscribe on `fleet:<id>:activity`. A restored subscription reports a gap so viewers can reload durable history.

The doorbell uses sixteen `fleet:wake:{shard}` channels and a bounded local hint buffer. Every replica subscribes through its existing hub.

Outbound work stays on its stream. PostgreSQL records the obligation before append, and the rebuild pass repairs missing entries.

Destination lanes preserve delivery order within a workspace. Up to eight deliveries run across independent lanes; each lane holds at most 32 jobs.

Acknowledgements trim completed history while retaining pending and undelivered entries. Those stream rules apply only to outbound delivery.

Boot refuses a primary in cache mode or with an eviction policy other than `noeviction`.
The local rig has four cluster processes. A fifth process serves the Transport Layer Security (TLS) trust tests.

See [Shared local development](#shared-local-development) for ownership and reset rules.

## How it behaves

The evidence sequence is: prototype on the local cluster, integrate the proven shape behind the boundary, prove cluster behaviour in the lane, then the four self-hosted processes, with Dragonfly Cloud Swarm after them (§"Destination: self-hosted first, Swarm later (Indy, 2026-09-14)"). Redis parity is not a gate, because Redis is not a supported backend.

Dragonfly supports disk snapshots without an append-only log. A crash loses writes after the last recoverable snapshot. A restart can restore stale data.

Recovery handles both missing and stale state. Any obligation still held in PostgreSQL is recovered regardless of Dragonfly's restored state. A stale outbound entry for completed delivery is acknowledged without another delivery.

Accepted work remains recoverable after Dragonfly is lost, provided the durable database survives. This includes pending inbound work and queued outbound answers. Third-party side effects retain their documented retry and idempotency limits; this page makes no exactly-once promise for external services.

Deploy implementation increments to the same application services while retaining their Dragonfly endpoint. Record an immutable revision and configuration for every comparison. Changes in instance count, database capacity, region, payload distribution, or background traffic must be disclosed and separated from code improvements.

The cutover to the cluster is a fresh start: PostgreSQL is rebuilt and Dragonfly provisioned empty, and nothing is carried across. There is therefore no import tool and no import receipt, and a daemon boots against a datastore that passes the three suitability refusals and nothing more. What the design owes instead is everything accepted after that point, through every ordinary restart and deployment: PostgreSQL records what was accepted, what completed and what delivery is still owed; Dragonfly is rebuildable from it, so losing Dragonfly's contents erases no obligation and a snapshot restoring stale entries re-runs nothing already terminal; a deployment replaces application processes and neither datastore; and a retry reuses the operation's identity, because a timeout does not prove an operation failed. A failed connection must not silently fall back to a standalone path or an empty queue.

## Limits

One million stored fleets, active fleets, concurrent runs, and runner processes are separate workload dimensions. Reports must state all four, together with offered load, completion rate, queue age, memory, and database cost. Small deployment probes prove behavior at their tested load; they do not prove the million-fleet target.

Fault injection and saturation testing use isolated datastores and synthetic destinations. Shared deployments use bounded fixture traffic and cleanup that only removes the run's own records. No shared Dragonfly flush, database reset, failover exercise, or provider switch follows from spec approval alone.

The historical Redis baseline (closed PR #681) is a reference for the local rig only. Dragonfly Cloud Swarm evidence belongs to the later move to Swarm; M192_001 claims no Swarm rows.

## Upstash retirement status

The datastore cutover ran in M196_001. Its runbook and probe runner were
deleted once it had served its purpose; this section is what survives it,
because one half of the move is finished and the other has not started.

**Development — retired.** `agentsfleetd-dev` has served on the `dragonfly-dev`
cluster since Sep 17, 2026. The `upstash-dev` vault item was archived out of
`ZMB_CD_DEV` the same day on Indy's named approval, and Indy deleted the hosted
Upstash database itself from the provider console. The daemon proves the
cluster on every boot regardless. `rustd/crates/afd_dragonfly/src/preflight/mod.rs`
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

## Shared local development

`make up` shares one infrastructure project across a repository's linked Git worktrees.
`compose.dev-infra.yml` contains Postgres, the local Dragonfly cluster, Mailpit and Upstash QStash.
The first caller saves a canonical Compose snapshot; later callers reuse healthy services without replacing that snapshot.

Each worktree starts its own native `agentsfleetd`, compiled from that checkout.
A repository build lock protects the shared Cargo cache and the copy to each worktree's separate binary path.
The daemon port, database name and development namespace come from the worktree's absolute path checksum.

| Resource | Development boundary |
|---|---|
| Postgres | One database per worktree; existing schemas and tenant checks remain intact |
| Dragonfly | `AGENTSFLEET_DEV_NAMESPACE` adds `dev:<namespace>:` to keys, streams, scan patterns and subscription channels |
| QStash | Each daemon supplies its own callback port through the existing `API_URL` setting |
| Mailpit | Shared inbox; use distinct recipient addresses to identify each worktree's mail |
| Linux runner | Separate binary, registered token, `AGENTSFLEET_API_URL` and `RUNNER_STORAGE_HOME` per worktree |

The namespace accepts at most 64 ASCII letters, digits, underscores or hyphens.
Unset or blank values retain existing datastore names; production and deployed `api-dev` keep their deployment configuration.
Replicas in one deployment use the same database and namespace so they can process the same deployment's work.

`afd_dragonfly` selects one validated, immutable namespace policy during configuration.
Stores resolve fixed outbound keys at construction and dynamic fleet/session keys through private key builders.
Arbitrary-key operations, dedicated blocking readers and the subscription hub apply the same policy.
Scans and delivered subscription messages return logical names to callers.
Raw command methods accept physical keys; `Dragonfly::physical_key` and `Dedicated::physical_key` map logical names for these callers.
Application code uses the scoped stores. The default policy preserves names and exposes every matching key to raw scans, including development prefixes on a shared cluster.

`make down` stops only the verified daemon belonging to the current worktree.
Shared snapshots and registered roots live under `${XDG_STATE_HOME:-$HOME/.local/state}/agentsfleet`, outside Git metadata.
`make status` lists saved stacks, live or gone worktree roots, and container states.
`make down DEV_STACK=<COMPOSE_PROJECT>` refuses existing registered roots and removes an orphan stack's containers and volumes using its saved port settings.
Teardown deletes that stack's databases, Dragonfly state and pending QStash deliveries.

The disposable stack used by `make test-integration-rustd` has separate project names, ports and volumes.
Its schema resets and cluster fault tests must never use the shared development project.
The [development notes](../development.md#shared-development-worktrees) explain startup, runner setup and failure recovery.

## Related pages

- [Scaling](./scaling.md) describes existing capacity assumptions that measurements must verify.
- [Data flow](./data_flow.md) and [runner fleet](./runner_fleet.md) describe the running topology; each Section updates them with its code.
- [Test isolation on a shared datastore (rules ISO-1 to ISO-3)](./testing.md#test-isolation-on-a-shared-datastore-rules-iso-1-to-iso-3) governs isolation on the shared lane datastores.
- [Dragonfly cluster mode](https://github.com/dragonflydb/dragonfly/blob/main/docs/cluster-mode.md), [compatibility](https://www.dragonflydb.io/docs/command-reference/compatibility), and [AOF](https://www.dragonflydb.io/docs/managing-dragonfly/aof) are checked against the pinned image.
