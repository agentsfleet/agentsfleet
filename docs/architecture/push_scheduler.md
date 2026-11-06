# Push scheduler

This page is the target design for how a message reaches a runner and how a run reports back, at 20,000 fleets. Milestones M242 to M247 build it, one Pull Request each. M241_001 recorded the decisions below and measured the starting point.

## Decisions

Indy made each decision below on Nov 06, 2026. The specs quote his words.

| Decision | What it means |
|---|---|
| Postgres accepts every message and fences every run | `fleet.admissions` is the only acceptance store. `runner.fleet_slots.fencing_token` is the only execution authority. |
| Messages fold into a running run | A chat message that arrives mid-run joins that run at its next model call. During compaction it waits, and the user sees "compacting". |
| Runners hold one stream | A runner host keeps one outbound connection to `agentsfleetd`. Work is pushed down it; nothing polls. |
| Core NATS replaces Dragonfly | NATS carries signals only: no JetStream, no KV. Every durable fact moves to Postgres first. |
| Durable tasks are on by default | `durable_tasks_v1` is the default. A failed host's run restarts elsewhere from the current turn's last checkpoint. |
| Tenants run their own runners | Self-hosted runners point at the cloud daemon through its API. They never connect to the broker. |
| Placement uses labels; respawn across owners is a tenant setting | The setting has three options: only my runners; mine, then platform hosts; any runner with matching labels. |
| Order and fairness | Strict order within a fleet; roughly fair across fleets. A small budget overrun is accepted. |
| Build small | The smallest version first, measured, then improved. |

Targets: 2,000 fleets now and 20,000 in 12 months. At least 50 fleets may burst at once, each at 10 inbound messages a second.

## Rules this design retires

Each rule below holds until the milestone named beside it lands; that milestone's Pull Request edits the page that states it.

| Rule | Where it is written | Retired by |
|---|---|---|
| Dragonfly is the only datastore, cluster-only | `.orly/LOCAL.md`, `datastore_scaling.md` | M244 |
| A runner polls; there is no cancel channel | `runner_fleet.md` | M243 |
| The lease path is not a general scheduler | `runner_fleet.md` | M243 |
| Config is never cached | `runner_fleet.md` | M245 |
| Timers have no central deadline scheduler | `concurrency.md` | M245 |

## Where each fact lives

| Fact | Store | Copies, and how they are repaired |
|---|---|---|
| Message accepted, in order | `fleet.admissions` | The runner's pending input, re-read at each model call |
| Run assigned, and its fence | `runner.fleet_slots`, `runner.leases` | The runner's stream session, rebuilt from Postgres on reconnect |
| Message folded into a run | `fleet.admissions.folded_into`, `fleet.events.folded_into` | None |
| Lease deadline | `runner.leases.lease_expires_at`, one clock | None |
| Usage and money | `billing.usage_lines` and the billing relay | None |
| Outbound reply owed | `fleet.obligations` | A NATS ping wakes a worker; a 30 s scan is the backstop |
| Device-login session, OAuth nonce | Postgres rows (M244) | None |
| Live tail, wake-ups, runner commands | Core NATS subjects | Lossy; each has a Postgres backstop (below) |

## Proposed tables by milestone

Each milestone's spec fixes its own columns and slot numbers when it starts.

| Milestone | Tables |
|---|---|
| M242 | A per-fleet unsettled-count guard row for one-statement intake. `folded_into` on `fleet.admissions` and `fleet.events`. `compacting_since` on `runner.fleet_slots`. |
| M243 | The connection a runner holds, on `runner.runners`. A cancel mark on `runner.leases`. |
| M244 | `identity.device_sessions`, `connector.oauth_nonces`, and claim columns on `fleet.obligations`. |
| M245 | None expected: caches are keyed by existing revision columns. |
| M246 | An owner tenant on `runner.runners`, a token overlap pair, and per-tenant runner settings. |
| M247 | Cordon reason and time on `runner.runners`, and a predecessor link on `runner.leases`. |

## Sequences

### A message arrives

```
client          agentsfleetd                       Postgres                        runner
  │ POST message    │                                 │                              │
  │────────────────►│ one statement: repeat check,    │                              │
  │                 │ policy, budget guard, insert ──►│ admission row (accepted)     │
  │                 │◄────────────────────────────────│ disposition                  │
  │◄────────────────│ started | folded | queued |     │                              │
  │                 │ waiting_compaction              │                              │
  │                 │ ping af.wake.<shard> (lossy)    │                              │
  │                 │──── offer on the runner stream ─────────────────────────────►│ idle fleet: start
  │                 │                                 │◄─ read pending input ────────│ busy fleet: next
  │                 │                                 │   under the fence            │ model call folds it
```

During compaction the runner reads no input. The message waits, its disposition is `waiting_compaction`, and it folds at the first model call after compaction.

### A runner connects, and reconnects

```
runner                         agentsfleetd replica                 Postgres
  │ open stream, token once ──────►│ authenticate                       │
  │                                │ read this runner's active and      │
  │                                │ cancel-marked leases ─────────────►│
  │◄─ Hello: policy, holds, leases │◄───────────────────────────────────│
  │ Ready{credits} ───────────────►│ offers follow credits              │
  │        … connection drops …    │                                    │
  │ reconnect (jitter) ──────────►│ any replica: the same rebuild      │
```

A second connection for the same runner supersedes the first. A revoked token closes the stream, and every lease grant re-checks the runner's status.

### Renewal, batched per host

```
runner                         agentsfleetd                         Postgres
  │ RenewBatch{lease ids, slices} ►│ one host lock, one fenced update   │
  │                                │ for every lease, money checked     │
  │                                │ every Nth tick ───────────────────►│
  │◄─ Renewed{per lease: deadline or lost}                              │
```

A lost lease in the batch never blocks the others. Each answer is fenced.

### The live tail

```
runner ─ Activity frames on its stream ─► replica A ─ publish af.fleet.<id>.tail ─► NATS
                                                                                     │
browser ◄─ SSE ◄─ replica B ◄────────────────────── subscribe ───────────────────────┘
          backfill once from fleet.events after subscribing
          reconnect or slow consumer ─► gap ─► catching_up ─► backfill again
```

The lease target is cached per stream session, so a frame costs no Postgres read. A tail never freezes silently. A failing publish or subscription sends `catching_up`.

### An outbound reply

```
report commits ─► fleet.obligations row ─► ping (lossy) ─► worker claims the lane's head row
                                                        └─ 30 s scan claims it if the ping is lost
```

Only the head of each `(provider, workspace)` lane is claimable, so claims never reorder a lane.

### A host dies mid-turn

```
host stops ─► heartbeats stop ─► host cordoned with its offline record
lease deadline passes ─► another runner claims the same event under a higher fence
             └─► loads the current turn's last checkpoint ─► run_restarted on the tail
```

Lease expiry is the only respawn trigger. The new run settles the admission once, and no input is dropped or replayed.

## When a signal is lost

Every NATS subject is at most once. Each one names its Postgres backstop.

| Subject | A lost message costs | Backstop |
|---|---|---|
| `af.fleet.<id>.tail`, `af.draft.<id>.events` | Frames missing on screen | Gap signal, then a backfill from Postgres |
| `af.wake.<shard>` | Work offered late | The gateway's periodic offer tick |
| `af.runner.<id>.cmd` | A cancel or folded input delayed | The cancel mark, and pending input read at each model call |
| Outbound ping | A reply sent late | The 30 s obligations scan |

## Milestones

| Milestone | Delivers | Proof of done |
|---|---|---|
| M241 | This page, honest counters, statements per chat by path, the burst bench | Baselines below |
| M242 | One-statement intake; folding with a disposition | Burst pickup p95 under 1 s; one statement per accepted message |
| M243 | One runner stream; the Bus seam with a Dragonfly implementation | Activity path near 0 statements; no poll; reconnect and revocation tests |
| M244 | Core NATS behind the Bus; durable jobs in Postgres; Dragonfly deleted | Tail and wake p99 and a kill drill, Dragonfly then NATS |
| M245 | Revision-keyed caches, a one-statement report, batched renewal | 15 to 20 statements per chat |
| M246 | Tenant-owned runners, a hard ownership filter, the respawn setting | Cross-tenant placement refused; no platform key leaves the daemon |
| M247 | Hold-aware capacity, auto-cordon, checkpoint respawn | A killed host's run restarts elsewhere, settled once |

A partitioned scheduler is not planned. It is built only if measured saturation falls below three times the 12-month peak.

## Baselines

Measured on Indy's laptop with `make bench-lease PROFILE=rig`. Each figure is the median of three interleaved runs, with the lowest and highest beside it. The api-dev machines re-measure at deployment.

A simulated chat is one lease with a 10 s turn: two renewals and forty activity posts.

| Path | Statements per chat |
|---|---|
| activity | 40.0 |
| gates | 11.0 |
| report | 10.0 |
| host | 9.0 |
| issue | 8.0 |
| meter | 8.0 |
| pricing | 7.0 |
| shared | 7.0 |
| claim | 4.9 |
| total | 105.0 |

These are a first single run. The committed median and its spread replace them before M241 closes.

A burst (`BENCH_SHAPE=burst`) sends 10 messages a second to each of 50 fleets while runners hold each lease for a 2 s turn. Today each message is a run of its own, so most wait. A short first run (5 fleets, 10 s) left 480 of 500 messages unleased at the close, with pickup p95 at 8.56 s.

Tail and wake p99 and the broker-kill drill are measured in M244, on Dragonfly and then on NATS, against the same code.
