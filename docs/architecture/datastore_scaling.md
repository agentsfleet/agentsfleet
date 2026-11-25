---
type: explanation
audience: contributor
verified: 2026-11-25
product_version: 0.90.0
executable: false
---

# Datastores: Postgres facts and NATS signals

## What it is

`agentsfleetd` uses two datastores.
Postgres holds every durable fact and every compare-and-swap change.
Core NATS, a message bus with JetStream, its key-value store and its object store all off, carries signals and stores nothing.

| Fact | Store | Copies, and how they are repaired |
|---|---|---|
| A message accepted, in order | `thread.turns` | The runner's pending input, re-read before each model call |
| A run assigned, and its fence | `thread.threads`, `thread.runs` | The runner connection, rebuilt from Postgres on reconnect |
| A message folded into a live run | `thread.turns.input_run_id`, `input_fence` | None |
| A run deadline | `thread.runs.run_expires_at` | None |
| Usage and money | `billing.usage_lines`, written by the report | None |
| An outbound reply owed | `thread.deliveries` | None; a NATS ping only wakes a worker |
| A device login, an OAuth nonce | `identity.device_sessions`, `connector.oauth_nonces` | None |
| A question or access request awaiting a person | `fleet.approval_gates` | None |
| Live tail, wake-ups, runner commands | Core NATS subjects | Lossy; [When a signal is lost](#when-a-signal-is-lost) names each cost |

## Why it exists

A lost bus node must cost latency, never accepted work or an owed reply.
Keeping every fact in Postgres makes that true.
A viewer backfills from `thread.turns`, and a worker claims from `thread.deliveries`.

## How it behaves

### Connecting to NATS

Each `agentsfleetd` process opens one NATS connection at boot (`afd_bus/src/nats/mod.rs`).
It reads `NATS_URL`: comma-separated seeds sharing one user and password in the userinfo (`afd_bus/src/nats/config.rs`).
An unset or malformed `NATS_URL` refuses boot as `UZ-STARTUP-001`.

Boot refuses, as `UZ-STARTUP-004`, a seed that does not answer within five seconds.
It also refuses a server that reports JetStream, no cluster, no headers, or a small `max_payload` (`afd_bus/src/nats/preflight.rs`).

The largest frame is 128 KiB: the live tail's 64 KiB batch (`afd_bus::TAIL_BATCH_BYTES`) plus a 64 KiB envelope.
NATS's default 1 MiB `max_payload` holds it.

### Deployment

| Lane | Nodes | Where |
|---|---|---|
| Production | Two, in the Fly app `agentsfleet-nats-prod` | `iad`, no volume (`deploy/fly/nats-prod/fly.toml`; `NATS_MACHINES` in `.github/workflows/release.yml`) |
| Development | Two, in the Fly app `agentsfleet-nats-dev` | `iad`, no volume (`NATS_MACHINES` in `.github/workflows/deploy-dev-fly.yml`) |
| Local and test | Three compose nodes, `ant`, `bee` and `elk`, routed as one cluster | `docker-compose.yml` |

The deploy workflows bring NATS up before the daemon.
`scripts/ensure_fly_app.sh` waits until every NATS machine passes `/healthz`, so a daemon never deploys against a bus that refuses it.
The daemon reaches NATS in plaintext `nats://` over Fly's private network, which WireGuard already encrypts.

The Rust client is `async-nats` 0.53.0 without default features, so its JetStream module does not compile (`rustd/Cargo.toml`).

### The turns ledger is the queue

The queue is the unsettled rows of `thread.turns`; there is no separate queue table.
Intake writes a turn with one statement, `afd_turns/src/sql/admit_one.sql`.
That statement holds the fleet row `FOR NO KEY UPDATE` until commit, so one fleet's turns commit in sequence order.

The turn commits before any wake is published, so a failed publish loses no accepted work.
A wake, a detected bus gap or a runner reporting free capacity starts an assignment pass that reads Postgres.
No timer reads Postgres for waiting work; a silently lost wake leaves the turn queued until the next event.

No per-fleet cap exists.
A deployment ceiling, `[turns] max_queued_turns` in [the configuration reference](./config_reference.md), bounds the total.

The estimate refreshes every 1,000 turns or five seconds, and every 500 ms while it refuses (`afd_turns/src/budget/ceiling/mod.rs`).
Replicas can overshoot it between samples.
[Data flow](./data_flow.md) traces the assignment and report that follow.

### Outbound replies

The report writes a `thread.deliveries` row in its transaction, then publishes `af.delivery.wake`.
A worker claims the head row of each `(provider, workspace_id)` lane with `FOR UPDATE SKIP LOCKED` (`afd_outbound/src/delivery/sql/claim_heads.sql`).

A claim lasts 30 s, and the holder renews it every 10 s (`afd_outbound/src/delivery/mod.rs`).
A worker claims at start, on a ping, when a lane finishes, and once when another holder's earliest claim lapses.
Up to eight deliveries run at once, and each lane holds at most 32 jobs (`afd_outbound/src/lanes/mod.rs`).

### Sessions and nonces

A device login is an `identity.device_sessions` row.
By default it lives 300 s, replays for 60 s and survives five wrong codes (`afd_tenant/src/session/store.rs`).

An OAuth nonce is a `connector.oauth_nonces` row that lives 600 s and is spent once by `DELETE … RETURNING`.
Nothing sweeps expired rows; every read refuses them by `expires_at`.

### When a signal is lost

Core NATS delivers each message at most once and acknowledges nothing.
Each subject has a Postgres backstop, or none is needed.

| Subject | A lost message costs | Backstop |
|---|---|---|
| `af.thread.<id>.items` | Frames missing on screen | A gap, then a backfill from Postgres |
| `af.thread.<id>.wake` | Work waits for the next wake, a credit increase or a reconnect | The turn stays in `thread.turns` |
| `af.runner.<id>.cmd` | A cancel or folded input arrives late | The cancel mark on the run, and pending input read before each model call |
| `af.delivery.wake` | A reply waits for the next ping, a gap or a worker start | The row stays in `thread.deliveries` |
| `af.tenant.<id>.members`, `af.workspace.<id>.fleets` | An open stream keeps a stale viewer or fleet set | A gap makes every stream re-read; each booting daemon publishes one |

The bus adapter turns every loss it can see into a `Gap` for the subscriber, which then reads Postgres.

- **Reconnect.** The client reconnects to another seed, and every local subscription receives one `Gap`.
- **Slow consumer.** Every subscription on that client receives a `Gap`, because the client hides which one fell behind.
- **Failed or recent publish.** After a reconnect, each subject published in the last 2 s gets one gap marker (`afd_bus/src/nats/debt.rs`).

Until those markers are sent, every publish fails at once, so no frame overtakes its gap.
Past 4,096 such subjects, one message on `af.bus.gap` makes every reader in every process re-read Postgres.

### Readiness

After boot, `/readyz` reports the bus in its `queue` field and does not fail on it (`rustd/crates/daemon/main/src/probes.rs`).
Messages are still accepted during a bus outage, because acceptance is a Postgres commit.

A NATS restart loses nothing durable, because NATS stored nothing.
A deployment replaces application processes, never either datastore.

## Limits

- A node that dies silently, from a host crash or a network split, resets no connection.
  The client notices after two unanswered pings, 60 s apart, so in 2 to 3 minutes.
  Frames sent before the 2 s window in that span carry no gap; the viewer sees them at its next reload.
- The compose cluster runs three nodes on one host. It proves node loss, not host loss.
- External side effects keep their own retry limits; no exactly-once delivery to outside services is promised.
- One million stored fleets is a target with no measurement behind it.
  A report must state stored fleets, active fleets, concurrent runs and runner processes separately.

## Related

- [Scaling](./scaling.md): sizing replicas, Postgres and runners.
- [Data flow](./data_flow.md): assignment, report and the live tail end to end.
- [Thread model](./thread_model.md): the turn, run and delivery tables.
- [Development notes](../development.md#shared-development-worktrees): the shared local stack and `AGENTSFLEET_DEV_NAMESPACE`.
- [Testing](./testing.md): test isolation on the shared lane datastores.
- [async-nats](https://docs.rs/async-nats/0.53.0/async_nats/): the client's events and server information.
