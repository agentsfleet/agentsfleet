# Runner Fleet — splitting `zombied` into a mothership + host-resident `zombie-runner`

> Parent: [`README.md`](./README.md) · Sibling: [`data_flow.md`](./data_flow.md) (the **current** single-fleet runtime)

Date: May 22, 2026
Status: **Target architecture for the M80 milestone, being built S0 → S6.** This file describes where the runtime is going; [`data_flow.md`](./data_flow.md) describes what runs in production today. Until the M80 cutover lands, `data_flow.md` is the operational truth and this file is the destination every M80 workstream builds toward. When cutover completes, the two reconcile and this becomes canon.

Read this when a spec touches the `zombie-runner` binary, the `/v1/runner` control API, runner enrollment, the node fleet, or the scheduler.

---

## The problem

Today one `zombied` binary runs as `serve` (the Application Programming Interface, API) or `worker` (the orchestration loop), chosen by subcommand, plus a `zombied-executor` sidecar that already owns sandboxing. Two structural facts make it hard to run work on hosts the platform doesn't fully own:

1. **The worker is welded to the datastores.** It opens a Postgres pool and Redis connections directly, runs ~15 write patterns on the per-event hot path, and *discovers its own work* by querying `core.zombies` and racing peers on the `zombie:control` consumer group. A worker cannot run anywhere it can't reach Postgres and Redis.
2. **Connection budget grows with the fleet.** Each per-zombie thread holds a dedicated blocking Redis connection (see [`scaling.md`](./scaling.md)); zombie count is bounded by the Redis pool ceiling, not by compute.

The executor is **not** the problem. It is already a separate binary speaking a length-prefixed JSON Remote Procedure Call (RPC) over a Unix socket, already sandboxed (Landlock + cgroups v2 + bubblewrap), already has a lease and a `Heartbeat`. It does not change in this architecture.

## The split

`zombied` becomes the **mothership** (control plane: owns Postgres, Redis, the API, assignment). A new host-resident binary, **`zombie-runner`**, packages the worker's orchestration logic plus the unchanged executor, and reaches the mothership only over a network control channel. It holds **zero** datastore credentials.

```
            CURRENT (data_flow.md)                    TARGET (this doc)
  ┌──────── ONE TRUST ZONE ─────────┐     ┌─ PLATFORM ─┐      ┌─ HOST (baremetal / Mac / pod) ─┐
  │ zombied serve ─┐ PG, vault       │     │ zombied    │      │ zombie-runner                  │
  │                ▼                 │     │ MOTHERSHIP │◀────▶│  thin loop: register,          │
  │ PG ◀── 15 writes ── zombied      │     │ owns PG +  │ TLS  │  heartbeat, lease, report      │
  │ Redis ◀── XREADGROUP ─ worker    │     │ Redis +    │ pull │   │ Unix-socket RPC (local)    │
  │                │ Unix socket RPC │     │ assignment │      │   ▼                            │
  │                ▼                 │     └─────┬──────┘      │  zombied-executor (UNCHANGED)  │
  │           zombied-executor       │       PG, Redis         │  sandbox: NullClaw             │
  └──────────────────────────────────┘    (never leave zone)   └────────────────────────────────┘
```

**Control plane vs data plane.** The mothership owns all durable state and all queueing. The `zombie-runner` owns only ephemeral execution: a leased event, a warm executor session, the runner's own identity token. Two channels, two transports, deliberately kept apart:

| Channel | Transport | Scope | Auth |
|---|---|---|---|
| mothership ↔ runner | HTTPS + JSON, long-poll (pull) | network, NAT-friendly | bearer `runner_token` |
| runner ↔ executor | length-prefixed JSON-RPC, Unix socket | local, inside the runner | none (loopback, unchanged) |

Pull, not push: a host behind Network Address Translation (NAT) or a firewall needs only outbound HTTPS. This is the GitLab-runner / GitHub-Actions / Nomad-client topology.

## The `/v1/runner` control contract

Four verbs. The mothership translates them into the Postgres writes and Redis stream operations the worker does directly today, so the runner never sees a datastore.

| Verb | Direction | Purpose |
|---|---|---|
| `register` | runner → mothership | exchange a short-lived **enrollment token** for a durable per-runner `runner_token`; report `sandbox_tier` + labels |
| `heartbeat` | runner → mothership | liveness + capacity; reply carries a `status` (`ok` in S0; `drain`/`stop` reserved for the M80_006 failover seam) |
| `lease` | runner → mothership (long-poll) | claim the next event; response carries the event envelope, the zombie's resolved config, and (mode A) the `secrets_map` |
| `report` | runner → mothership | one idempotent batched transaction: `zombie_events` received→terminal, telemetry, debit, session checkpoint, then `XACK` |

The hot path collapses from the worker's 5-7 sequential Postgres roundtrips into **one `lease` + one `report`**. Idempotency that lives in the worker's `INSERT … ON CONFLICT` today moves into the `report` handler (replay-safe by `event_id`).

### Event-leasing + sticky routing

A `zombie-runner` is a **stateless event-leaser** (the GitHub-Actions model): it pulls the next event for any zombie it is eligible for, runs it, reports, repeats. A dead runner's in-flight lease expires and another runner reclaims it — the Redis Pending-Entries-List (PEL) reclaim already provides this.

The cost of pure event-leasing is a cold executor session per event. We recover most of the warmth with **best-effort sticky routing**: the mothership prefers to hand a zombie's next event to the runner that last ran it (hint stored as `last_runner_id`), falling back to any eligible runner. Warmth is an optimization; **correctness always derives from the durable `core.zombie_sessions` checkpoint**, never from runner-local state.

### Secret boundary — the real trust decision

"Agnostic of Postgres/Redis" is the easy part: those are platform credentials the runner never needed. The runner still needs the tenant `secrets_map` to make tool calls — that is the entire job. So `secret_delivery` is a contract field with a swappable mode:

```
  "anyone can run a zombie-runner"
    ├── anyone the OPERATOR enrolls (mode A, NOW)  → secrets ship in the lease over TLS,
    │     trusted fleet, same posture as the executor today
    └── anyone GLOBALLY incl. untrusted (LATER)    → mode C: per-tenant-scoped runners
          (a runner only receives work + secrets for tenants that enrolled it), or
          mode B: zero-trust, secrets never leave the mothership (egress proxy)
```

The runner topology (S0-S6) is **identical** under any mode. The open-fleet vision is a secret-delivery problem layered on top, not a topology problem, so it is deferred — S0 only commits the `secret_delivery` field and an optional tenant scope on enrollment so modes C/B don't require re-cutting the contract.

## Enrolling a runner — operator flow + trust gates

Adding a host is **pull-based enrollment**, not a push registration. The operator hands the host a short-lived enrollment token; the host trades it for a durable identity, then pulls work:

```
  Operator                 Mothership                       Host (zombie-runner)
   │ 1. request enrollment                          🔒 GATE 1 — WHO MAY ENROLL
   ├───────────────────────▶│ mint ENROLLMENT_TOKEN       short-lived · single-use ·
   │◀───────────────────────┤  (TTL, single-use, scoped)   scoped to tier/tenant
   │ 2. install runner, hand it the token
   ├───────────────────────────────────────────────────────────────▶│
   │            3. register {enrollment_token, host_id,               │  🔒 GATE 2 — ENROLLMENT
   │               sandbox_tier, labels}  ── over TLS ────────────────┤  validate token; reject if
   │               │ mint runner_token (256-bit random)               │  missing / expired / used
   │               │ store ONLY hash(runner_token) in fleet.runners   │  → a rogue without a valid
   │               ├─────────────────────────────────────────────────▶│    token is stopped here
   │               │  runner_token (returned once)                    │  persist locally
   │            4. steady loop — Authorization: Bearer runner_token       🔒 GATE 3 — per-call auth
   │               │◀── heartbeat · lease · report ───────────────────┤  hash(Bearer) == token_hash
   │               │  eligibility: sandbox_tier + tenant scope         │  (timing-safe)
   │               │  + secret_delivery bound what the runner receives │  🔒 GATE 4 — blast radius
```

Where the trust actually lives:

- **Gate 1 (issue) + Gate 2 (register)** are what stop a rogue runner — no valid enrollment token, no entry. This is the enrollment-token + TLS work in M80_005.
- **Gate 3 (`token_hash`)** only *authenticates* an already-enrolled runner per call. Necessary, not sufficient — it does not decide who may enroll.
- **Gate 4 (`sandbox_tier` + tenant scope + `secret_delivery`)** bounds a runner's blast radius. In mode A secrets ship inline to every enrolled runner, so mode A is for **operator-owned fleets only**; untrusted hosts wait for the per-tenant-scoped mode.

## Sandbox tiers (and how a Mac gets a real sandbox)

Landlock + cgroups + bubblewrap are Linux-only. A runner reports its isolation strength at enrollment as a label; assignment (and later the scheduler) refuses to place other-tenant or production work on a weak tier.

| `sandbox_tier` | Where | Eligible for |
|---|---|---|
| `landlock_full` | Linux host | any work |
| `container_nested` | runner inside a container on a Linux host (or a Linux Virtual Machine, VM) | any work — full sandbox, nested |
| `macos_seatbelt` | macOS, Apple Seatbelt profile (weaker) | own-tenant / dev work |
| `dev_none` | no real sandbox | own-tenant dev work |

Containerizing the runner is a first-class deployment mode. Landlock is a stackable Linux Security Module (LSM) and nests for free; bubblewrap needs user-namespace + mount permission; per-agent cgroup limits need cgroup-v2 delegation — `sysbox` grants those safely without full `--privileged`. On macOS, running `zombie-runner` inside a Linux VM (Docker Desktop / OrbStack / Lima) is exactly how a laptop earns `container_nested` instead of the degraded `macos_seatbelt`.

## Scaling

The split inverts the binding constraint:

```
        TODAY                                  TARGET
  N zombies → N Redis conns             N zombies → bounded mothership pool
  (pool ceiling is the wall)            runners hold 0 datastore conns
  bottleneck: Redis connections         bottleneck: mothership API replicas + PG writes
                                        both scale HORIZONTALLY
```

Runners scale out with zero coordination (add hosts, they enroll, they pull). The mothership serve tier replicates behind a load balancer; the `lease` endpoint shards by zombie/queue. The one piece needing care at multi-replica scale is **placement** (assignment/scheduler) — multiple replicas making placement decisions need leader-election or partitioning, which is the M80_006/007 concern. The hot path (lease/report) is already shardable.

## What does NOT change

- The `zombied-executor` binary, its RPC, its sandbox, its secret substitution at the tool bridge.
- Event ingress: steer / webhook / cron / continuation still `XADD zombie:{id}:events` (see `data_flow.md §B`).
- The user read path: `GET /events`, the Server-Sent-Events (SSE) live tail, `zombiectl status/events`.
- The three durable stores and their contracts (`data_flow.md` "three durable stores").

## Roadmap (M80 workstreams)

```
 S0  M80_001  KEYSTONE   freeze /v1/runner contract + ALL schema + route/build stubs;
                         prove ONE zombie register→lease→report over loopback, flag-gated  ← serial, first
 ── parallel fan-out against the frozen contract ───────────────────────────────────────
 S1  M80_002  API        mothership lease/report/assignment over real PG + Redis
 S2  M80_003  WORKER     thin the worker to call the contract; strip direct PG/Redis
 S3  M80_004  RUNNER     the `zombie-runner` binary + macOS Seatbelt backend + distribution
 S4  M80_005  IDENTITY   enrollment + per-runner token + TLS  (own security spec/PR)
 ── cutover: flip flag, delete old direct path (RULE NLR) ───────────────────────────────
 S5  M80_006  FLEET      node inventory + heartbeat + lease reassignment on death
 S6  M80_007  SCHEDULER  placement on labels/capacity/sandbox-tier; autoscale by queue depth
 ── later ────────────────────────────────────────────────────────────────────────────
     mode C    per-tenant-scoped runners — the open "run it on your laptop" vision
```

The heartbeat/failover gap this fills is the one [`data_flow.md`](./data_flow.md) already flags ("Recovery needs a heartbeat or XAUTOCLAIM sweep ... Multi-replica high availability remains a later concern").
