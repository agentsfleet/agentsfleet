---
type: explanation
audience: contributor
verified: 2026-11-09
product_version: 0.75.0
executable: false
---

# Datastore scaling requirements and target design

## Decision: core NATS carries signals, Postgres holds every fact

Indy chose core NATS for the bus on 2026-11-06.
He said, "Yes i agree on NATS."
The M244_001 spec's Discovery records that approval and the measurements it depended on.

Postgres owns every durable fact and every compare-and-swap transition.
Core NATS carries live activity, draft progress, wake notifications, runner commands and the outbound ping.
NATS runs without JetStream and without a key-value store, so it holds no state.

Each signal is disposable because its durable authority and its recovery path live in Postgres.
The [scheduler decisions](push_scheduler.md#decisions) list each subject and its backstop.

M244 measured both buses on the same laptop and code ([baselines](push_scheduler.md#m244-previous-broker-against-nats)).
NATS met the bar on 15 of 16 tail rungs, and Indy accepted the one miss on 2026-11-09.

In the kill drills, NATS viewers received `catching_up` within 0.3 s of a node stop.

| Decision | Requirement |
|---|---|
| Bus | Core NATS, routed as one cluster in one region, with JetStream off. Boot refuses a server that fails any of these. |
| Durable state | Postgres: admissions, outbound obligations, device-login sessions, OAuth nonces and approval gates. |
| Deployment | The Fly apps `agentsfleet-nats-prod` and `agentsfleet-nats-dev` each run two nodes, both in `iad`, with no volume (`.github/workflows/release.yml`, `deploy-dev-fly.yml`). |
| Rust client | `async-nats` 0.52.0 without default features, so its JetStream module does not compile (`rustd/Cargo.toml`). |
| Evidence | Compare before and after each change on the same rig and workload. |
| Scale | Report measured capacity against an explicit workload; fleet population alone proves no throughput guarantee. |

Changing the bus or moving a durable fact out of Postgres requires an explicit decision from Indy, recorded on this page.

## What it is

This page records where `agentsfleetd` keeps each kind of state, and the target design for scaling it.
Postgres is the only datastore. Core NATS is a message bus that stores nothing.

The runtime pages ([`data_flow.md`](./data_flow.md), [`runner_fleet.md`](./runner_fleet.md)) describe the same running system from the request's side.

## Why it exists

A lost bus node must cost latency, never accepted work or an owed reply.
Keeping every fact in Postgres makes that true: a viewer backfills from `fleet.events`, and a worker claims from `fleet.obligations`.

Before M244, the bus also held durable jobs, and all its nodes ran on one machine per environment.
One machine loss froze every viewer's tail with no signal.

## Target design

Each `agentsfleetd` process opens one NATS connection at boot (`rustd/crates/daemon/afd_bus/src/nats/mod.rs`).
It reads `NATS_URL`, comma-separated seeds that share one user and password in the userinfo (`afd_bus/src/nats/config.rs`).
An unset or malformed `NATS_URL` refuses boot as `UZ-STARTUP-001`.

Boot refuses, as `UZ-STARTUP-004`, a seed that does not answer within five seconds.
It also refuses a server reporting JetStream, no cluster, no headers, or a `max_payload` under the largest frame (`afd_bus/src/nats/preflight.rs`).
The largest frame is 128 KiB: the tail's 64 KiB batch plus 64 KiB of envelope.

The deploy workflows bring the NATS app up before the daemon.
`scripts/ensure_fly_app.sh` waits until every NATS machine passes its `/healthz` check, so a daemon never deploys against a bus that refuses it.
The daemon reaches NATS over Fly's private network in plaintext `nats://`.

### The admissions ledger is the queue

```text
producer -> commit fleet.admissions -> publish af.wake.<shard>
                         |
wake ring or 5 s recovery probe -> offer pass -> Postgres head probe
                         |
              slot + admission claim
                         |
        fenced lease offered down the runner stream
                         |
report -> usage + terminal event + settlement + delivery obligation
```

A producer commits its identity and payload before reporting acceptance. A failed wake publish leaves that acceptance intact.

A wake ring starts an offer pass, and the pass reads Postgres (`afd_gateway/src/scheduler/mod.rs`).

Each replica also asks Postgres for pending work every 5 s (`OFFER_FALLBACK_TICK`, `afd_gateway/src/scheduler/recovery/mod.rs`).
A lost ring therefore costs up to 5 s, never work.

The candidate query first checks whether any unsettled admission exists. An empty ledger skips fleet scans.
When work exists, it reads each eligible fleet's oldest unsettled admission through `idx_admissions_unsettled` on `(fleet_id, sequence)`.

Admission holds the fleet row lock within its one statement, before allocating its sequence and through commit. A transaction that inserts later therefore stays behind unfinished work.

A parked head blocks later rows while standing access awaits an answer or an unresolved event approval has a future deadline.
Both discovery and claim probe these two scopes through `idx_approval_gates_pending_scope_deadline`.

One `NOT EXISTS` over `UNION ALL` keeps the probes correlated to the chosen head. Each branch limits its search to one blocker, including when many approvals match.

Expired event approvals remain answerable; the resumed gate evaluation decides whether to proceed or refuse.
A parked event finds its gate in `fleet.approval_gates` alone, so a long bus outage cannot erase that decision.

The claim locks the slot and exact head with `SKIP LOCKED`, then checks whether that head can run and advances both fences together.
A blocked or unavailable head returns no work, and the caller rolls back the slot claim.

Claims retain held-sandbox preference and spread other candidates randomly. They provide order within each fleet, with no global waiting-time guarantee.

`delivered_at` records admission into runtime history. `settled_at` removes a row from the queue.
A report settles the admission in the transaction that records usage, terminal state, checkpoint, slot release and any outbound obligation.

An expired slot exposes the same unsettled head to another runner. The original billing identity survives redelivery.

Admission is one statement, `admit_one.sql`, that holds the fleet row `NO KEY UPDATE` until it commits, so one fleet's messages commit in sequence order.
That lock permits the foreign-key checks needed to record delivery concurrently. No per-fleet cap exists; the deployment ceiling bounds the total.

The deployment ceiling estimates 100,000 unsettled admissions. It refreshes every 1,000 admissions or five seconds, and every 500 milliseconds while refusing.

Each process adds its own accepted admissions between samples. Concurrent replicas can overshoot this estimate; one fleet's sequence stays serialized.
The census publishes `agentsfleet_admission_backlog` and `agentsfleet_admission_backlog_oldest_age_seconds` from PostgreSQL, withdrawing them when its query fails.

### Signals on core NATS, facts in Postgres

The live tail publishes on `af.fleet.<fleet_id>.tail`. After a reconnect, a slow consumer or a gap marker, viewers receive `catching_up` and reload durable history.

Wake notifications use sixteen `af.wake.<shard>` subjects, the shard a CRC-16/XMODEM of the fleet id (`afd_bus/src/wake/publish.rs`).
Each replica keeps a local queue of at most 64 fleets by default (`afd_bus/src/wake/config.rs`).
A full queue evicts its oldest fleet, and the 5 s recovery probe still finds that fleet's work.

The report path writes the `fleet.obligations` row in its own transaction, then publishes `af.outbound.wake`.
A worker claims the head row of each `(provider, workspace_id)` lane with `FOR UPDATE SKIP LOCKED` (`afd_outbound/src/obligation/sql/claim_heads.sql`).
A claim lasts 30 s, the holder renews it every 10 s, and every replica also claims every 30 s.

Up to eight deliveries run at once across lanes; each lane holds at most 32 jobs (`afd_outbound/src/lanes/mod.rs`).
Every write after a claim matches the holder's token, so a lapsed holder's late write changes nothing.

Device-login sessions are `identity.device_sessions` rows: 300 s of life, a 60 s replay window, and five verify attempts (`afd_tenant/src/session/store.rs`).
OAuth nonces are `connector.oauth_nonces` rows that live 600 s and are spent once by `DELETE … RETURNING`.

Nothing sweeps expired sessions or nonces. Every read refuses them by `expires_at`, and spending an expired nonce removes it.

Gate references come from `fleet.approval_gates` (`afd_gate/src/gate/store/mod.rs`).
The anomaly brake counts in each replica's memory, one window per rule, bounded at 10,000 keys, and fails open (`afd_gate/src/gate/anomaly.rs`).

See [Shared local development](#shared-local-development) for ownership and reset rules.

## How it behaves

Core NATS delivers each message at most once and acknowledges nothing.
The adapter turns every loss it can see into a `Gap` for the subscriber, which backfills from Postgres.

- **Reconnect.** The client reconnects to another seed by itself, and every local subscription receives one `Gap`.
- **Slow consumer.** Every subscription on that client receives `Gap`, because the client keeps the subscription id private.
- **Failed or recent publish.** A failed publish is owed an `Af-Gap: 1` marker on reconnect. So is every subject published in the 2 s before a disconnect.

From a disconnect until those markers are paid, every publish fails at once, so no frame reaches a subscriber ahead of its gap.
Past 4,096 owed subjects, one marker on `af.bus.gap` gaps every subscription in every process (`afd_bus/src/nats/debt.rs`).

After boot, `/readyz` reports the bus in its `queue` field and does not gate on it (`rustd/crates/daemon/main/src/probes.rs`).
A bus outage costs latency, and pulling every replica out of service would turn it into a full outage.
Steers are still accepted, because acceptance is a Postgres commit.

A NATS restart loses nothing durable, because NATS stored nothing.
Accepted work stays recoverable while Postgres survives, including pending inbound work and owed outbound answers.
Third-party side effects keep their documented retry and idempotency limits; this page promises no exactly-once delivery to external services.

A deployment replaces application processes and neither datastore. A retry reuses the operation's identity, because a timeout does not prove an operation failed.

## Limits

A node that dies silently, from a host crash or a network split, resets no connection.
The client notices only when its pings go unanswered: one ping every 60 s, stale after two, so about 2 to 3 minutes.
Frames published before the debt window in that span reach no reader and carry no gap; a viewer sees their rows at its next `catching_up`.

The compose cluster runs three nodes on one host. It proves node loss, but not host loss or real DNS between nodes.

One million stored fleets, active fleets, concurrent runs, and runner processes are separate workload dimensions. Reports must state all four, together with offered load, completion rate, queue age, memory, and database cost. Small deployment probes prove behavior at their tested load; they do not prove the million-fleet target.

Fault injection and saturation testing use isolated datastores and synthetic destinations. Shared deployments use bounded fixture traffic and cleanup that only removes the run's own records. No shared database reset, node stop, failover exercise, or provider switch follows from spec approval alone.

## Shared local development

`make up` shares one infrastructure project across a repository's linked Git worktrees.
`compose.dev-infra.yml` contains Postgres, three NATS nodes (`nats-1` to `nats-3`), Mailpit and Upstash QStash.
The first caller saves a canonical Compose snapshot; later callers reuse healthy services without replacing that snapshot.

Each worktree starts its own native `agentsfleetd`, compiled from that checkout.
A repository build lock protects the shared Cargo cache and the copy to each worktree's separate binary path.
The daemon port, database name and development namespace come from the worktree's absolute path checksum.

| Resource | Development boundary |
|---|---|
| Postgres | One database per worktree; existing schemas and tenant checks remain intact |
| NATS | `AGENTSFLEET_DEV_NAMESPACE` adds `dev.<namespace>.` to every subject, the `af.bus.gap` broadcast included |
| QStash | Each daemon supplies its own callback port through the existing `API_URL` setting |
| Mailpit | Shared inbox; use distinct recipient addresses to identify each worktree's mail |
| Linux runner | Separate binary, registered token, `AGENTSFLEET_API_URL` and `RUNNER_STORAGE_HOME` per worktree |

The namespace accepts at most 64 ASCII letters, digits, underscores or hyphens.
An unset namespace keeps the deployed subject spelling; production and deployed `api-dev` set none.
Replicas in one deployment use the same database and namespace so they can process the same deployment's work.

`afd_bus::nats::Config` reads the namespace once at boot and refuses one outside its charset as `UZ-STARTUP-001` (`afd_bus/src/nats/config.rs`).
The adapter applies the prefix to every publish and subscribe, so callers name only logical subjects.

`make down` stops only the verified daemon belonging to the current worktree.
Shared snapshots and registered roots live under `${XDG_STATE_HOME:-$HOME/.local/state}/agentsfleet`, outside Git metadata.
`make status` lists saved stacks, live or gone worktree roots, and container states.

`make down DEV_STACK=<COMPOSE_PROJECT>` refuses existing registered roots and removes an orphan stack's containers and volumes using its saved port settings.
Teardown deletes that stack's databases and pending QStash deliveries; the NATS nodes keep nothing to delete.

The disposable stack used by `make test-integration-rustd` has separate project names, ports and volumes.
Its schema resets and node-stop tests (`TEST_NATS_CONTROL`) must never use the shared development project.
The [development notes](../development.md#shared-development-worktrees) explain startup, runner setup and failure recovery.

## Upstash QStash

QStash is the cron trigger and stays in service. Mentions of Upstash in the cron sections of these pages are correct.

## Related pages

- [Scaling](./scaling.md) describes existing capacity assumptions that measurements must verify.

- [Data flow](./data_flow.md) and [runner fleet](./runner_fleet.md) describe the running topology; each Section updates them with its code.

- [Push scheduler](./push_scheduler.md) holds the subjects, their backstops and the M244 measurements.

- [Test isolation on a shared datastore (rules ISO-1 to ISO-3)](./testing.md#test-isolation-on-a-shared-datastore-rules-iso-1-to-iso-3) governs isolation on the shared lane datastores.

- [async-nats](https://docs.rs/async-nats/0.52.0/async_nats/) documents the client's events and server information, checked against the pinned version.
