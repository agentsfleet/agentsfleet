# Push scheduler

This page is the target design for how a message reaches a runner and how a run reports back, at 20,000 fleets. Milestones M242 to M247 build it, one Pull Request each. M241_001 recorded the decisions below and measured the starting point.

## Decisions

Indy made the initial decisions on Nov 06, 2026. The transport decision was revised on Nov 08, 2026.
The specs quote his words.

| Decision | What it means |
|---|---|
| Postgres accepts every message and fences every run | `fleet.admissions` is the only acceptance store. `runner.fleet_slots.fencing_token` is the only execution authority. |
| Messages fold into a running run | A person's message in the run's conversation joins that run before its next model call. A call in flight is never interrupted. A message no person sent, such as a webhook, waits for a run of its own and is never passed. Revised Nov 08, 2026: a plain run takes a durable task's rule (M242_002 Discovery). |
| Runners hold one stream | Work is pushed over one outbound connection. Each daemon replica retains one database recovery probe every five seconds. |
| Direct HTTP/3 with HTTP/2 fallback | The runner initiates a session to a reachable daemon endpoint. Fly-hosted services can use private networking; external operators supply connectivity and trusted certificates. Cloudflare and WebSocket are not required. See [the implementation design](runner_protocol.md), revised Nov 08, 2026. |
| Core NATS is the bus | NATS carries signals only: no JetStream, no key-value (KV) store. Every durable fact lives in Postgres. |
| Durable tasks are on by default | `durable_tasks_v1` is the default. A failed host's run restarts elsewhere from the current turn's last checkpoint. |
| Tenants run their own runners | Self-hosted runners point at the cloud daemon through its API. They never connect to NATS. |
| Placement uses labels; respawn across owners is a tenant setting | The setting has three options: only my runners; mine, then platform hosts; any runner with matching labels. |
| Order and fairness | Strict order within a fleet; roughly fair across fleets. A small budget overrun is accepted. |
| Build small | The smallest version first, measured, then improved. |

Targets: 2,000 fleets now and 20,000 in 12 months. At least 50 fleets may burst at once, each at 10 inbound messages a second.

## Rules this design retires

Each rule below holds until the milestone named beside it lands; that milestone's Pull Request edits the page that states it.

| Rule | Where it is written | Retired by |
|---|---|---|
| A cluster-only broker is the only datastore | `.orly/LOCAL.md`, `datastore_scaling.md` | M244, done: both pages now name core NATS and Postgres |
| A runner polls; there is no cancel channel | `runner_fleet.md` | M243 |
| The lease path is not a general scheduler | `runner_fleet.md` | M243 |
| Config is never cached | `runner_fleet.md` | M245 |
| Timers have no central deadline scheduler | `concurrency.md` | M245 |

## Where each fact lives

| Fact | Store | Copies, and how they are repaired |
|---|---|---|
| Message accepted, in order | `fleet.admissions` | The runner's pending input, re-read at each model call |
| Run assigned, and its fence | `runner.fleet_slots`, `runner.leases` | The runner's stream session, rebuilt from Postgres on reconnect |
| Message folded into a run | `fleet.admissions.input_claim_lease_id` and `input_claim_fence` (#803; `claim_inputs.sql` for any other run) | None |
| Lease deadline | `runner.leases.lease_expires_at`, one clock | None |
| Usage and money | `billing.usage_lines` and the billing relay | None |
| Outbound reply owed | `fleet.obligations` | A NATS ping wakes a worker; a 30 s scan is the backstop |
| Device-login session, OAuth nonce | Postgres rows (M244) | None |
| Live tail, wake-ups, runner commands | Core NATS subjects | Lossy; each has a Postgres backstop (below) |

## Proposed tables by milestone

Each milestone's spec fixes its own columns and slot numbers when it starts.

| Milestone | Tables |
|---|---|
| M242 | None. Intake holds the fleet row for one statement, and folding rides #803's input claims on `fleet.admissions`. |
| M243 | The connection a runner holds, on `runner.runners`. A cancel mark on `runner.leases`. |
| M244 | `identity.device_sessions`, `connector.oauth_nonces`, and claim columns on `fleet.obligations`. |
| M245 | None expected: caches are keyed by existing revision columns. |
| M246 | An owner tenant on `runner.runners`, a token overlap pair, and per-tenant runner settings. |
| M247 | Cordon reason and time on `runner.runners`, and a predecessor link on `runner.leases`. |

## Recovery and scheduling fairness

The Bus keeps one bounded wake queue shared by producers and the gateway.
A dropped notification or periodic database probe marks a recovery generation.
That mark survives later locality hints and hosts with no free capacity.

The five-second probe repairs a crash after admission commits but before its disposable wake is published.
Normal admission and steering publish a wake immediately and do not wait for that probe.

During recovery, admission selection orders by durable creation time and identifier across eligible fleets.
It ignores fleet-locality hints until a durable claim covers that recovery generation.
A claim for an older generation cannot clear a newer probe.

Normal hints resume after recovery, preserving locality without starving an older unhinted fleet.

## Sequences

### A message arrives

```
client          agentsfleetd                       Postgres                        runner
  │ POST message    │                                 │                              │
  │────────────────►│ one statement: fleet lock,      │                              │
  │                 │ repeat, status, policy, ceiling,│                              │
  │                 │ insert ────────────────────────►│ admission row (accepted)     │
  │                 │◄────────────────────────────────│ live-run facts               │
  │◄────────────────│ started | folded | queued       │                              │
  │                 │ ping af.wake.<shard> (lossy)    │                              │
  │                 │──── offer on the runner stream ─────────────────────────────►│ idle fleet: start
  │                 │                                 │◄─ claim pending input ───────│ durable-task run:
  │                 │                                 │   under the fence (#803)     │ next safe boundary
```

A live run claims a person's later messages in its conversation before its next model call, oldest first. A durable task does the same through #803. Each stops at a parked, task-bound or non-person message.

The report settles each message that a completed model call read (M242_002), in three statements however many there are. Each settled message gets the run's answer on its own event. It is in the run's conversation, so the run's own reply reaches it.

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
| `af.wake.<shard>` | Work offered up to 5 s late | Each replica's 5 s recovery probe (`OFFER_FALLBACK_TICK`) |
| `af.runner.<id>.cmd` | A cancel or folded input delayed | The cancel mark, and pending input read at each model call |
| Outbound ping | A reply sent late | The 30 s obligations scan |

## Milestones

| Milestone | Delivers | Proof of done |
|---|---|---|
| M241 | This page, honest counters, statements per chat by path, the burst bench | Baselines below |
| M242 | One-statement intake; folding with a disposition | One write statement per accepted message; a live run takes a person's mid-run message in its conversation |
| M243 | One runner stream; the Bus seam | Activity path near 0 statements; no poll; reconnect and revocation tests |
| M244 | Core NATS behind the Bus; durable jobs in Postgres | Tail p99 and a kill drill on both buses |
| M245 | Revision-keyed caches, a one-statement report, batched renewal | 15 to 20 statements per chat |
| M246 | Tenant-owned runners, a hard ownership filter, the respawn setting | Cross-tenant placement refused; no platform key leaves the daemon |
| M247 | Hold-aware capacity, auto-cordon, checkpoint respawn | A killed host's run restarts elsewhere, settled once |

A partitioned scheduler is not planned. It is built only if measured saturation falls below three times the 12-month peak.

## Baselines

Measured on Indy's laptop on Nov 06, 2026, with `make bench-lease PROFILE=rig`, each run on a reset rig. Each lane ran three times, and the run with the median headline value is committed in `bench/baselines/`. The lowest and highest values sit beside it.

The api-dev machines re-measure at deployment.

A simulated chat is one lease with a 10 s turn: two renewals and forty activity posts. The drain leases 200 fleets to 8 runners over a pool of 20.

The headline is drain leases per second: 82.51, between 80.43 and 84.08. The median run is committed as `lease.rig.json`.

| Path | Statements per chat (median run) | Lowest | Highest |
|---|---|---|---|
| activity | 40.0 | 40.0 | 40.0 |
| gates | 11.0 | 11.0 | 11.0 |
| shared | 11.0 | 11.0 | 11.0 |
| host | 9.0 | 9.0 | 9.0 |
| issue | 8.0 | 8.0 | 8.0 |
| meter | 8.0 | 8.0 | 8.0 |
| report | 8.0 | 8.0 | 8.0 |
| pricing | 7.0 | 7.0 | 7.0 |
| claim | 6.945 | 6.8 | 6.945 |
| task | 6.0 | 6.0 | 6.0 |
| other | 0.08 | 0.075 | 0.08 |
| total | 115.025 | 114.875 | 115.025 |

The `task` path holds the durable-task reads PR #803 added to issue, renewal and report. A chat also costs 77.2 commits. Lease p95 is 44.1 ms and report p95 is 21.5 ms.

A burst (`BENCH_SHAPE=burst`) sends 10 messages a second to each of 50 fleets for 60 s. Runners hold each lease for a 2 s turn. Today each message is a run of its own, so most wait.

The headline is pickup p95: 54.2 s, between 53.4 s and 54.5 s. The median run is committed as `lease-burst.rig.json`.

In it, 29,953 messages were accepted, none were refused, and 29,729 were still unleased at the close. Pickup p50 is 28.5 s and append p95 is 35.7 ms.

With M242's folding, a live run takes its fleet's messages at each model call. Pickup p95 is 491.5 ms, between 489.0 ms and 491.8 ms, and that median run replaces `lease-burst.rig.json`.

Of 30,000 messages, 4,564 folded and 25,212 still waited at the close, because 8 runners serve 50 fleets. The pickup percentiles count only the messages a run picked up.

### M242: one-statement intake

Measured on Indy's laptop on Nov 07, 2026, with `make bench-steer PROFILE=rig`'s lane. Base `6ce61641d` and branch `43d828970` ran interleaved, each on a reset rig. The median branch run is committed as `steer.rig.json`.

| Shape | Base, three runs | Branch, three runs |
|---|---|---|
| Default: Postgres statements per steer | 4.003, 4.005, 4.004 | 1.003, 1.003, 1.003 |
| Default: accepted per second | 1698, 1260, 1238 | 1956, 2390, 2191 |
| Default: p95 ms | 6.96, 13.6, 15.8 | 12.1, 10.1, 9.52 |
| One fleet, 16 senders, 3 s: accepted per second | 528, 330, 532 | 316, 837, 210 |
| One fleet, 16 senders, 10 s: accepted per second | 312, 342, 383 | 929, 933, 1031 |
| One fleet, 16 senders, 10 s: p95 ms | 6.46, 4.03, 3.76 | 1.42, 1.21, 1.13 |

The 3 s one-fleet window scattered from 210 to 837 on the branch, so the 10 s runs are the comparison. One fleet now admits about 2.7 times as many messages a second.

A burst on the branch measured pickup p95 at 54.7 s, unchanged from M241, because that branch did not fold messages yet.

M244 measured tail p99 and the broker-kill drill on both buses, against the same code (below). The wake ladder moved to M243.

### M244: previous broker against NATS

Measured on Indy's laptop on Nov 09, 2026, with `make bench-tail PROFILE=rig`: three interleaved pairs, one run on each bus, each on a reset rig. The tree was this branch at `514f37420` with the tail-lane method of `fb2f55801` and the publish-record fix of `202b08ffd`.

Each rung shows the median of the three runs. The median NATS run, by its 4,096-stream p99, is committed as `tail.rig.json`.

The viewer ladder keeps one frame in flight, so a frame's time is one trip through the bus and the fan-out. The stream ladder publishes to every stream at once and times each stream from the burst's start.

Indy chose this method after the first run. There, a 64-frame window let NATS's non-blocking publish queue ahead, so the run measured the queue.

### Broker pass bar

The bar, per rung: NATS p99 at most the greater of the previous broker's p99 plus 10% and its p99 plus 1 ms.

| Viewers | Frame | Previous broker p99 (ms) | NATS p99 (ms) | Bar (ms) | Verdict |
|---|---|---|---|---|---|
| 1 | 200 B | 0.35 | 0.79 | 1.35 | pass |
| 1 | 4 KiB | 0.56 | 0.50 | 1.56 | pass |
| 1 | 64 KiB | 0.46 | 2.76 | 1.46 | miss |
| 64 | 200 B | 0.32 | 0.40 | 1.31 | pass |
| 64 | 4 KiB | 0.29 | 1.13 | 1.29 | pass |
| 64 | 64 KiB | 0.46 | 1.22 | 1.46 | pass |
| 256 | 200 B | 0.86 | 0.86 | 1.86 | pass |
| 256 | 4 KiB | 0.88 | 0.82 | 1.88 | pass |
| 256 | 64 KiB | 0.87 | 1.36 | 1.87 | pass |
| 1,024 | 200 B | 3.28 | 3.71 | 4.28 | pass |
| 1,024 | 4 KiB | 3.18 | 3.09 | 4.18 | pass |
| 1,024 | 64 KiB | 3.39 | 3.35 | 4.39 | pass |

| Streams bursting at once | Previous broker p99 (ms) | NATS p99 (ms) | Bar (ms) | Verdict |
|---|---|---|---|---|
| 64 | 0.87 | 0.84 | 1.87 | pass |
| 256 | 1.53 | 1.78 | 2.53 | pass |
| 1,024 | 4.17 | 2.26 | 5.17 | pass |
| 4,096 | 20.83 | 9.83 | 22.91 | pass |

No frame went undelivered and no viewer lagged on either bus.

Verdict: NATS passes. Fifteen of sixteen rungs meet the bar. The miss is one viewer of 64 KiB frames, 1.3 ms over.

A size sweep ruled out the client's 65,535-byte read buffer, and the cause is open. Indy accepted it: "I would accept the 1 bench miss and signoff NATS."

#### Kill drills

Two runs per bus, on the local daemon (`make up`). Two viewers watched one fleet's stream with one steer every 10 s, while the bus stopped for 60 s and then started. For NATS the stopped node was the one the daemon held, `nats-3` then `nats-2`.

The two runs do not lose the same share of the bus. The previous broker ran all four cluster processes, two primaries and their replicas, in one container, so stopping it stopped the whole cluster. NATS lost one node of three.

| | Previous broker, runs 1 and 2 | NATS, runs 1 and 2 |
|---|---|---|
| During the outage | heartbeats only | every steer reached both viewers live |
| `catching_up` | 2.8 to 3.0 s after the container restarted | 0.25 to 0.3 s after the node stopped |
| Steers in `fleet.admissions` | 19 of 19 per run | 19 of 19 per run |
| Recovery | waited for the bus container to restart | the daemon moved to a surviving node |
