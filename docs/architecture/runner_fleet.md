# Runner Fleet — splitting `zombied` into a control plane + a host-resident `zombie-runner`

> Parent: [`README.md`](./README.md) · Sibling: [`data_flow.md`](./data_flow.md) (the **current** single-process runtime).

Date: May 24, 2026
Status: **Target architecture for the M80 milestone, built S0 → S6.** [`data_flow.md`](./data_flow.md) describes the single-process model the codebase implements now; this file is the target the M80 workstreams build toward. The two reconcile at the M80_003 cutover.

Read this when a spec touches the `zombie-runner` binary, the `/v1/runners` control protocol, runner registration, the node fleet, or the scheduler.

---

## Why split

Today one `zombied` binary runs as `serve` (the HTTP API) or `worker` (the orchestration loop), plus a `zombied-executor` sidecar that owns sandboxing. Two facts make it hard to run work on hosts the platform does not fully own:

1. **The worker is welded to the datastores.** Each per-zombie worker thread opens its own Postgres pool and Redis connections, runs ~15 write patterns on the per-event hot path, and discovers its own work by `XREADGROUP` on `zombie:{id}:events`. It cannot run anywhere it can't reach Postgres and Redis directly.
2. **The connection budget grows with the fleet.** Every per-zombie thread holds a dedicated blocking Redis connection (see [`scaling.md`](./scaling.md)); the zombie count is capped by the Redis pool ceiling, not by compute.

The goal: move execution onto arbitrary hosts (bare metal, a Mac, a pod) that hold **no datastore credentials**, reaching the platform only over an authenticated HTTP control protocol.

## The split — two binaries, no sidecar

- **`zombied`** — the control plane. Owns Postgres, Redis, the Vault API, the HTTP API, and work assignment. Unchanged in role; it gains the `/v1/runners` endpoints.
- **`zombie-runner`** — a new host-resident binary. It is the worker's orchestration loop **plus the NullClaw execution logic linked in directly** (the old `zombied-executor` sidecar is gone as a separate process). It holds zero datastore credentials and talks to `zombied` only over HTTPS, carrying a `runner_token`.

```
        CURRENT (data_flow.md)                      TARGET (this doc)
 ┌──────── ONE TRUST ZONE ─────────┐    ┌─ PLATFORM ──┐      ┌─ HOST (bare metal / Mac / pod) ─┐
 │ zombied serve ─┐  PG, Vault      │    │ zombied     │      │ zombie-runner  (one binary)     │
 │                ▼                 │    │ control     │◀────▶│  parent loop: register,         │
 │ PG ◀─ 15 writes ─ zombied worker │    │ plane:      │HTTPS │  heartbeat, lease, report,      │
 │ Redis ◀─ XREADGROUP ─ worker     │    │ owns PG +   │ pull │  activity                       │
 │                │ Unix-socket RPC │    │ Redis +     │ zrn_ │    │ fork + sandbox per event    │
 │                ▼                 │    │ Vault API + │      │    ▼                            │
 │           zombied-executor       │    │ assignment  │      │  sandboxed child: NullClaw      │
 └──────────────────────────────────┘   └──────┬──────┘      └─────────────────────────────────┘
                                          PG · Redis · Vault
                                          (never leave the platform)
```

**Why the executor folds in but still forks.** NullClaw runs the agent: language-model calls plus tool calls, with tenant secrets substituted at the tool bridge. It needs a sandbox — Landlock (filesystem) + cgroups (memory/CPU) + a network namespace. Landlock is one-way and irreversible for a process, and the `zombie-runner` parent loop needs un-sandboxed network to reach `zombied`. So the runner **forks a sandboxed child per event** and talks to it over a local pipe. One binary, two process roles: an un-sandboxed parent that speaks the control protocol, and a sandboxed child that runs NullClaw. There is no separate daemon to deploy.

### Where the code lives

The directory layout mirrors this split so the "runner holds zero datastore credentials" invariant is **structural and grep-visible**, not merely enforced by `build_runner.zig`'s import list. The two planes never share a tree; the only surface they both reach is the frozen wire contract, consumed as a named Zig module (`@import("contract")`) so neither build graph reaches into the other's source.

| Layer | Path | Build graph | Links | Role |
|---|---|---|---|---|
| `contract` | `src/lib/contract/` | both (named module) | none | frozen `/v1/runners` wire types — `protocol`, `event_envelope`, `execution_policy` |
| `control_plane` | `src/runner/control_plane/` | `zombied` (`build.zig`) | `pg`, `redis` | lease / fence / reclaim / assignment engine |
| `daemon` | `src/runner/daemon/` | `zombie-runner` (`build_runner.zig`) | none | runner-side process; imports nothing from `control_plane` |
| `common` | `src/runner/common/` | both | none | single-source knobs both layers key off |

Consumers import the `src/runner.zig` root and reach a layer (`runner.control_plane.service`) rather than a deep file path.

## The control protocol — `/v1/runners`

Five verbs. `zombied` translates them into the Postgres writes and Redis stream operations the worker does directly today, so the runner never sees a datastore.

| Verb | Path | Auth | Purpose |
|---|---|---|---|
| `register` | `POST /v1/runners` | `Bearer` Clerk JWT **or** `zmb_t_` api_key | exchange a human/service credential for a durable `runner_token` (`zrn_`); record `host_id`, `sandbox_tier`, `labels` |
| `heartbeat` | `POST /v1/runners/me/heartbeats` | `Bearer zrn_` | liveness; reply carries `status` (`ok` / `drain` / `stop`) and any revoked lease IDs |
| `lease` | `POST /v1/runners/me/leases` | `Bearer zrn_` | long-poll for the next event; reply carries the event, resolved config, secrets, `lease_id`, `fencing_token` — or `null` + `retry_after_ms` |
| `report` | `POST /v1/runners/me/reports` | `Bearer zrn_` | terminal result for a lease; `zombied` persists + `XACK`s |
| `activity` | `POST /v1/runners/me/leases/{lease_id}/activity` | `Bearer zrn_` | write-only progress stream for the live tail; best-effort, no ack |

`me` resolves from the token — no `runner_id` in any path or body, so there is nothing to spoof or reconcile. `register` is the one verb authed by a *human/service* credential; everything else is authed by the machine credential it returns. Identity and auth are covered in [`../AUTH.md`](../AUTH.md) (the runner is the first machine principal).

## Registering a runner

A runner needs a `zrn_` token before it can pull work. It gets one by calling `register` with an existing credential — a Clerk JWT (an operator from the dashboard or CLI) or a `zmb_t_` api_key (an automated provisioner). There is no separate admin endpoint and no enrollment token; the open-fleet, self-enrolling case is mode C, later.

```
 caller (operator w/ Clerk JWT  OR  host w/ zmb_t_)         zombied                  host: zombie-runner
   │ POST /v1/runners                                🔒 GATE 1 — who may register:
   │   Authorization: Bearer <Clerk-JWT | zmb_t_>    a valid Clerk JWT or api_key
   │   { host_id, sandbox_tier, labels[] }           is required
   ├────────────────────────────────────────────────►│ mint zrn_ (256-bit random)
   │                                                  │ store ONLY sha256(zrn_) in fleet.runners
   │◀──────────────────────────────────────────────────┤ 201 { runner_id, runner_token: zrn_ }  (shown once)
   │ install zrn_ on the host (env ZOMBIE_RUNNER_TOKEN)
   ├──────────────────────────────────────────────────────────────────────────────►│ persist locally
   │ steady loop — Authorization: Bearer zrn_         🔒 GATE 2 — per-call auth:
   │      ◀── heartbeat · lease · report · activity ─┤ sha256(Bearer) == token_hash (timing-safe)
   │      eligibility: sandbox_tier + scope + secret_delivery   🔒 GATE 3 — blast radius
```

`zombied` owns the Postgres pool, the Redis pool, and the Vault API; `zombie-runner` owns none of them and holds only the `zrn_` token. Rotating a token swaps `token_hash`; revoking sets `status='revoked'` so the next call gets a 401.

## Running one event (NullClaw)

A `lease` reply is the runner's entire input for an event. The runner forks a sandboxed child, the child runs NullClaw, and the result goes back via `report`.

```
lease → { event, ExecutionPolicy(config + secrets_map + network_policy + tool_allowlist),
          lease_id, fencing_token, checkpoint? }
   │
zombie-runner: fork a sandboxed child (Landlock + cgroups + network namespace)
   │
   └─ child runs NullClaw: build config from the policy, build the tool set from the
      allowlist, run the agent turn — language-model calls + tool calls, secrets
      substituted at the tool bridge — and return content + tokens + timing
   │
report → zombied: persist terminal state + telemetry + checkpoint, then XACK
```

The executor RPC (`createExecution` → `startStage` → `getUsage` → `destroyExecution`) and its TOCTOU guards (lease re-check before a stage, orphan reaping, idempotent destroy) move inside the runner as parent↔child supervision. The durable lease guard moves to `zombied` via `lease_expires_at` + `fencing_token` (see **Reclaim** below).

### Multi-stage events

A *stage* is one NullClaw run inside one language-model context window. When a single event needs more reasoning than one window holds, NullClaw stops at `stage_chunk_threshold` (0.75 of the context cap), checkpoints, and signals "resume me." `zombied` enqueues a **continuation event** chained by `resumes_event_id`, and the next lease resumes from the checkpoint in a fresh window. One lease = one stage.

```
trigger event E0 ─► STAGE 1 (lease, checkpoint=∅) ─► NullClaw hits 0.75 cap ─► report{continue, C1}
                                                            │ zombied persists checkpoint C1,
                                                            │ enqueues continuation (resumes_event_id=E0)
                ─► STAGE 2 (lease, checkpoint=C1) ─► … ─► report{continue, C2}
                ─► STAGE 3 (lease, checkpoint=C2) ─► NullClaw finishes ─► report{processed}
```

Durable state across stages is the checkpoint in `zombied`, never runner-local — which is why a different runner can pick up stage 2. A chain hard-stops at 10 continuations (escalates to a human). Sticky routing (below) prefers the runner that ran the previous stage, but correctness never depends on it.

## Live activity (the SSE tail)

NullClaw emits progress frames mid-run (tool started, response chunk, tool completed). Today the worker `PUBLISH`es them to `zombie:{id}:activity` for the Server-Sent-Events (SSE) live tail. A runner holds no Redis, so it forwards frames to `zombied` over the `activity` verb, and `zombied` does the `PUBLISH`. Downstream SSE is unchanged.

```
NullClaw child ─pipe─► runner parent ─POST .../activity (chunked, no ack)─► zombied ─PUBLISH─► SSE
```

Two planes, kept apart on purpose: **activity** is ephemeral and best-effort (a dropped frame is cosmetic); **report** is the durable system of record. The live tail is never the source of truth.

## Steer, kill, pause

All three are decided by `zombied`, which owns both `core.zombies.status` and lease issuance. A runner learns of an in-flight change on its next `heartbeat`, so cancel latency is bounded by the heartbeat interval.

- **Steer** — a human message. `zombied` enqueues a `steer` event; it is leased like any other. The current stage finishes first; the steer runs next. Not an interrupt.
- **Pause** — `zombied` sets `status=paused` and stops issuing leases for the zombie. Any in-flight lease runs to completion.
- **Kill** — `zombied` sets `status=killed` and marks the in-flight lease revoked. The runner sees the revocation in its next heartbeat reply, kills the sandboxed child, and reports `cancelled`. A late report from a killed runner is rejected by the fencing token.

A dedicated low-latency cancel channel can come later; heartbeat-carried revocation is the S0 mechanism.

## Cold and warm execution

Default is **cold**: every lease forks a fresh sandbox, runs, and tears it down. No pinning, no stale state, no idle cost.

A later, opt-in **warm** mode keeps the sandbox shell alive across leases for the same zombie to skip cold setup. Warm reuses only the sandbox shell — never agent state or config. Two guards make it safe: the lease always carries fresh config + secrets (config is never cached, see below) and the checkpoint is the only carried state; and sticky routing is a *hint*, not ownership — if the warm runner is busy or dead, any eligible runner takes the event, and idle warm children self-evict. A zombie is never stuck waiting for one runner.

## Config

A zombie's config (model, tool allowlist, network policy, context budget, gate rules, trigger settings, secret references) is parsed from `TRIGGER.md` frontmatter into `core.zombies.config_json`. A `PATCH /v1/workspaces/{ws}/zombies/{id}` updates it — including reparsing `trigger_markdown` to add a tool.

In the split, `zombied` resolves config fresh from Postgres on every `lease`, so config changes take effect on the **next command** (the next lease) with no signaling — the same boundary at which the worker reloads config today, just without the in-memory cache and the `zombie_config_changed` signal. A config change never alters a language-model turn already in flight; the next stage picks it up.

## Money gates

The credit-pool billing model debits twice per event, and both debits move to `zombied`'s lease/report path — the runner never touches billing.

- At **lease issue**, before handing work to a runner: the balance gate (does the tenant cover the receive + stage estimate?), then the `receive` debit (flat, posture-based), then the approval gate, then the `stage` debit (a conservative estimate at floor tokens). Any gate failure means no lease is issued.
- At **report**: reconcile the stage telemetry row to the actual token counts. The charged amount stays at the pre-execution estimate — report updates telemetry, it does not re-charge.

Receive credits are not refunded if the stage later exhausts. This mirrors today's `metering.zig` exactly; only the caller moves from the worker to `zombied`.

## Redis topology — what changes

Today there are three Redis surfaces. The split keeps two (shifting their producer/consumer to `zombied`) and retires one.

| Surface | Today | Split |
|---|---|---|
| `zombie:{id}:events` (work stream, group `zombie_workers`) | the per-zombie worker thread is the consumer (`worker-{host}-{ts}`); `XREADGROUP`, `XAUTOCLAIM`, `XACK` | **`zombied` is the consumer.** `XREADGROUP` on `lease`, `XACK` on `report`. The runner is not a Redis consumer. |
| reclaim of a dead processor | `XAUTOCLAIM` by consumer idle (5 min) — a dead worker is a dead consumer | **lease expiry + `fencing_token`.** A dead runner is *not* a dead Redis consumer (`zombied` is), so consumer-idle can't see it. The lease layer is the reclaim mechanism. |
| `zombie:control` (control stream) | the watcher consumes `zombie_created` / `zombie_status_changed` / `zombie_config_changed` / `worker_drain_request` to spawn/cancel/reload per-zombie threads | **retired.** No per-zombie threads to orchestrate: created is moot, status/config live in Postgres + heartbeat/lease, drain is the heartbeat reply. Kept only if serve replicas need their own coordination. |
| `zombie:{id}:activity` (pub/sub) | the worker `PUBLISH`es; SSE handlers subscribe | same channel + SSE; `zombied` `PUBLISH`es, fed by the runner's `activity` stream. |

The reclaim shift is the load-bearing one: moving the processor off-platform means Redis can no longer observe its death, so the durable lease (`lease_expires_at` + `fencing_token`, frozen in M80_001) replaces `XAUTOCLAIM`.

## Sandbox tiers

A runner reports its isolation strength at registration. Assignment (and later the scheduler) refuses to place other-tenant or production work on a weak tier. The reported tier is telemetry; trust for placement is operator-assigned, not self-claimed.

| `sandbox_tier` | Where | Eligible for |
|---|---|---|
| `landlock_full` | Linux host | any work |
| `container_nested` | runner inside a container on a Linux host or VM | any work — full sandbox, nested |
| `macos_seatbelt` | macOS, Seatbelt profile (weaker) | own-tenant / dev work |
| `dev_none` | no real sandbox | own-tenant dev work |

On a Mac, running `zombie-runner` inside a Linux VM (Docker Desktop / OrbStack / Lima) is how a laptop earns `container_nested` instead of the degraded `macos_seatbelt`.

## Scaling

The split inverts the binding constraint. Today N zombies need N Redis connections and the pool ceiling is the wall. After the split, runners hold zero datastore connections; the bottleneck becomes `zombied` API replicas + Postgres writes, both of which scale horizontally. Runners scale out with no coordination — add hosts, they register, they pull. The one piece needing care at multi-replica scale is placement (assignment / scheduler), which is the M80_006/007 concern; the hot path (lease / report) is shardable.

## What does not change

- NullClaw's agent loop, its tool inventory, and secret substitution at the tool bridge. It moves into the runner as a linked module and a sandboxed child, but its behaviour is identical.
- Event ingress: steer / webhook / cron / continuation still `XADD zombie:{id}:events`.
- The user read path: `GET /events`, the SSE live tail, `zombiectl status/events`.
- The three durable stores and their contracts (see `data_flow.md`).

## Roadmap (M80 workstreams)

```
 S0  M80_001  KEYSTONE   freeze /v1/runners protocol + fleet schema + route/build stubs;
                         prove ONE zombie register→lease→report over loopback, flag-gated   ← serial, first
 ── parallel fan-out against the frozen protocol ────────────────────────────────────────────
 S1  M80_002  API        zombied lease/report/assignment + fencing + reclaim over PG + Redis
 S2  M80_003  WORKER     thin the worker to call the protocol; strip the direct PG/Redis path;
                         reconcile data_flow.md / capabilities.md / scaling.md to the split (Dimension)
 S3  M80_004  RUNNER     the zombie-runner binary, NullClaw fold-in, macOS backend, distribution
 S4  M80_005  IDENTITY   TLS hardening + trust_class/allowed_workspace_ids authz (register + runnerBearer shipped in S0/M80_001)
 ── cutover: flip the flag, delete the old direct path ───────────────────────────────────────
 S5  M80_006  FLEET      node inventory + heartbeat + lease reassignment on death; operator plane
 S6  M80_007  SCHEDULER  placement on labels / capacity / sandbox-tier; autoscale by queue depth
 ── later ─────────────────────────────────────────────────────────────────────────────────────
     mode C    self-enrolling runners — the open "run it on your own host" case
```
