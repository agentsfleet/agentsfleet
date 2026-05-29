# Runner Fleet — `zombied` control plane + host-resident `zombie-runner` execution plane

> Parent: [`README.md`](./README.md) · Sibling: [`data_flow.md`](./data_flow.md) (how one event flows through this split).

Date: May 24, 2026 · reconciled to the cutover May 27, 2026
Status: **Implemented (M80_002 cutover).** This is the runtime the codebase runs now: `zombied` is the control plane, the host-resident `zombie-runner` daemon is the execution plane, and the old single-process `zombied worker` + `zombied-executor` sidecar are deleted. [`data_flow.md`](./data_flow.md) traces an event through it; this file is the structural picture.

Read this when a spec touches the `zombie-runner` binary, the `/v1/runners` control protocol, runner registration, the node fleet, or assignment / fencing / reclaim.

---

## System guarantees (read this first)

The runner fleet is an **execution plane**: stateless runners lease work, run it in a sandbox, and report back. The control plane (`zombied`) owns all durable state. Everything below is a consequence of that one decision — read the guarantees before the mechanics, because the mechanics only exist to hold these.

| Guarantee | What the platform promises | How it holds |
|---|---|---|
| **No event loss on runner death** | A runner that crashes, partitions, or is killed mid-event never drops the event. | The lease has a `lease_expires_at`; the reclaim sweep re-leases an expired lease to another runner. Durability is at-least-once via `core.zombie_events` + `INSERT … ON CONFLICT DO NOTHING`. |
| **At-most-once durable effect** | A reclaimed or duplicate runner cannot double-write state. | Every lease carries a monotonic `fencing_token`; `report` verifies it in the same atomic statement that flips the lease to `reported`. A stale holder's report is rejected (`UZ-RUN-005`). |
| **Secrets never leave the trust boundary** | Tenant credentials are never written to a runner's disk, logs, or cache. | `secrets_map` rides the lease inline over Transport Layer Security (TLS), is used only at the tool bridge inside the sandboxed child, and is never persisted runner-side. |
| **Execution is always sandboxed** | No leased event ever runs un-isolated. | Each lease forks a child under Landlock + cgroups + a network namespace; a sandbox-setup failure fails **closed** — the child does not start, the runner reports `UZ-RUN-007`, and the lease is redeliverable. |
| **The runner holds no datastore credentials** | A compromised or untrusted host cannot reach Postgres, Redis, or the Vault. | `build_runner.zig` links no `pg` / `httpz` / `redis`; the only platform surface the runner reaches is the authenticated `/v1/runners` protocol carrying a `zrn_` token. |

### Runners are cattle, not pets

A runner has no durable identity that the system depends on. It is enrolled once by the operator, then leases, runs, reports, and may vanish at any moment; the control plane notices via lease expiry and hands the work to whichever runner leases next. There is no runner the fleet cannot lose. Sticky routing (below) is a *performance hint*, never ownership — correctness never blocks on one runner being alive.

## Failure recovery model

Recovery latency is **emergent from fleet polling density**, not a hard bound — a dead runner's work is picked up when its lease expires and another runner next leases. The current Service Level Agreement (SLA) is the S0 floor; tightening it is the M80_006 mandate, not optional polish.

| Failure | SLA today (S0) | Mechanism | Tradeoff | M80_006 path |
|---|---|---|---|---|
| Runner dies mid-lease | work resumes within ~`LEASE_TTL_MS` (30 s) + next lease latency | lease expiry + reclaim sweep re-leases with a higher fencing token | recovery latency is lazy (tied to the TTL), not push-driven | heartbeat-detected death → proactive reassignment; sub-10 s recovery |
| Stale report after reclaim | immediate | `report` CAS verifies `fencing_token`; stale holder rejected (`UZ-RUN-005`) | the redone work by the new holder is the authority; the slow holder's compute is wasted | unchanged — fencing is the durable guard |
| **Agent outruns the lease TTL** | **broken for agents > 30 s** | there is **no per-lease heartbeat renewal yet**: a child running past `lease_expires_at` is killed at its deadline and the event is reclaimed + re-run | state stays correct (the late report is fenced, no double-write) but the work is **redone and capped at 30 s** | **per-lease renewal driven by the activity stream** (`tool_call_progress` is the long-tool heartbeat) + decoupled liveness/execution TTL + a separate hard max-runtime cap |
| Sandbox setup fails | immediate | child never starts; runner reports `agent_error` (`UZ-RUN-007`); lease redeliverable | a host with a broken sandbox burns one lease attempt before the operator cordons it | cordon / reaping of hosts that repeatedly fail to establish a sandbox |
| Control plane unreachable | bounded by runner backoff | runner retries with backoff; the un-acked lease redelivers | a runner that can't reach `zombied` does no work until the link returns | unchanged — the runner is the reconnect handler |

> **The renewal gap is live operational debt, not a bug.** `LEASE_TTL_MS = 30_000` (single-sourced in `src/lib/common/constants.zig`) with no renewal means the cutover is safe to default to the runner **only for agents that finish inside the TTL**. Flipping the runner to default for **> 30 s** agents must wait for M80_006's per-lease renewal, or ship with `LEASE_TTL_MS` raised to cover the max expected runtime plus a separate hard cap.

## Scope — an execution plane, deliberately not a control plane

The fleet borrows Kubernetes / Nomad / Temporal **semantics** — leases, fencing, node heartbeats, drain, sticky scheduling, checkpointed workloads — but it is **not** a general orchestrator and must not drift into one. The non-goals are load-bearing; each rejected feature is one we deliberately do not build until a spec changes this direction:

- **Not a general scheduler.** Placement is capped at *sticky + any-eligible*. Label / capacity / sandbox-tier placement is M80_007, full stop.
- **No autoscale.** Runners scale by operators adding hosts, not by the platform reacting to queue depth.
- **No fairness engine.** No per-tenant weighting, no priority lanes, no preemption.
- **No arbitrary workload types.** One workload: a NullClaw stage from a leased `ExecutionPolicy`.

Without this fence the design rediscovers three control planes at once (Nomad-lite + Temporal-lite + Kubernetes-lite), each demanding its own observability, reconciliation, and high-availability story. The distributed-systems core here is sound; the risk is scope, not correctness. If the platform ever needs a true control plane, that is a larger upfront conversation (inventory / reconciliation / high-availability / placement fairness) — surface it, don't drift into it.

---

## Why split

The pre-cutover runtime ran one `zombied` binary as `serve` (the HTTP API) or `worker` (the orchestration loop), plus a `zombied-executor` sidecar that owned sandboxing. Two facts made it impossible to run work on hosts the platform does not fully own:

1. **The worker was welded to the datastores.** Each per-zombie worker thread opened its own Postgres pool and Redis connections, ran ~15 write patterns on the per-event hot path, and discovered its own work by `XREADGROUP` on `zombie:{id}:events`. It could not run anywhere it could not reach Postgres and Redis directly.
2. **The connection budget grew with the fleet.** Every per-zombie thread held a dedicated blocking Redis connection; the zombie count was capped by the Redis pool ceiling, not by compute.

The cutover moved execution onto arbitrary hosts (bare metal, a Mac, a pod) that hold **no datastore credentials**, reaching the platform only over the authenticated `/v1/runners` protocol.

## The split — two binaries, no sidecar

- **`zombied`** — the control plane. Owns Postgres, Redis, the Vault API, the HTTP API, and work assignment / fencing / reclaim. It gained the `/v1/runners` endpoints and does the `XREADGROUP` / `XACK` the worker used to do.
- **`zombie-runner`** — the host-resident execution plane. It is the parent control loop **plus the NullClaw execution engine linked in directly** (the old `zombied-executor` sidecar is gone). It holds zero datastore credentials and talks to `zombied` only over Hypertext Transfer Protocol Secure (HTTPS), carrying a `runner_token`.

```
        BEFORE (deleted)                            NOW (this doc + data_flow.md)
 ┌──────── ONE TRUST ZONE ─────────┐    ┌─ PLATFORM ──┐      ┌─ HOST (bare metal / Mac / pod) ─┐
 │ zombied serve ─┐  PG, Vault      │    │ zombied     │      │ zombie-runner  (one binary)     │
 │                ▼                 │    │ control     │◀────▶│  parent loop: heartbeat,        │
 │ PG ◀─ 15 writes ─ zombied worker │    │ plane:      │HTTPS │  lease, report, activity        │
 │ Redis ◀─ XREADGROUP ─ worker     │    │ owns PG +   │ pull │  (boots from pre-minted zrn_)   │
 │                │ Unix-socket RPC │    │ Redis +     │ zrn_ │    │ fork + sandbox per event    │
 │                ▼                 │    │ Vault API + │      │    ▼                            │
 │           zombied-executor       │    │ assignment  │      │  sandboxed child: NullClaw      │
 └──────────────────────────────────┘   └──────┬──────┘      └─────────────────────────────────┘
                                          PG · Redis · Vault
                                          (never leave the platform)
```

**Why the executor folds in but still forks.** NullClaw runs the agent: language-model calls plus tool calls, with tenant secrets substituted at the tool bridge. It needs a sandbox — Landlock (filesystem) + cgroups (memory/CPU) + a network namespace. Landlock is one-way and irreversible for a process, and the `zombie-runner` parent loop needs un-sandboxed network to reach `zombied`. So the runner **forks a sandboxed child per event** and talks to it over a local pipe. One binary, two process roles: an un-sandboxed parent that speaks the control protocol, and a sandboxed child that runs NullClaw. There is no separate daemon to deploy.

### Where the code lives

The directory layout makes the "runner holds zero datastore credentials" guarantee **structural and grep-visible**, not merely enforced by `build_runner.zig`'s import list. The control plane and the execution plane never share a source tree; the only surface both reach is the frozen wire protocol, consumed as a named Zig module (`@import("contract")`) so neither build graph reaches into the other's source.

| Layer | Path | Build graph | Links | Role |
|---|---|---|---|---|
| `contract` | `src/lib/contract/` | both (named module) | none | frozen `/v1/runners` wire types — `protocol`, `event_envelope`, `execution_policy`, `execution_result`, `activity` |
| `common` | `src/lib/common/` | both (named module) | none | single-source knobs both planes key off (`LEASE_TTL_MS`, …) |
| `logging` | `src/lib/logging/` | both (named module) | none | shared logfmt scope helpers |
| control plane | `src/zombied/fleet/` | `zombied` (`build.zig`) | `pg`, `redis` | `assign` / `affinity` / `reclaim` / `service` / `service_report` / `service_activity` — lease / fence / reclaim / assignment |
| runner daemon | `src/runner/daemon/`, `src/runner/{main,child_supervisor,child_exec,sandbox_args,pipe_proto}.zig` | `zombie-runner` (`build_runner.zig`) | none | runner-side process; imports nothing from `src/zombied` |
| runner engine | `src/runner/engine/` | `zombie-runner` | none (NullClaw base) | the folded-in NullClaw engine + sandbox glue (`cgroup`, `landlock`, `network`) |

The control-plane handlers under `src/zombied/fleet/` are faithful mirrors of the deleted worker's `event_loop_writepath` steps — the comments there name their origin so the row-equivalence guarantee (below) is auditable.

## The control protocol — `/v1/runners`

Five verbs. `zombied` translates them into the Postgres writes and Redis stream operations the worker did directly, so the runner never sees a datastore.

| Verb | Path | Auth | Handler | Purpose |
|---|---|---|---|---|
| `register` | `POST /v1/runners` | `Bearer` Clerk JWT carrying `platform_admin` | `runner/register.zig` | platform operator mints a durable `runner_token` (`zrn_`) for a host; record `host_id`, `sandbox_tier`, `labels`. Tenant `admin` JWT / `zmb_t_` api_key → `403`. The host does not call this — the operator does, then installs the `zrn_` |
| `heartbeat` | `POST /v1/runners/me/heartbeats` | `Bearer zrn_` | `runner/heartbeat.zig` | liveness; reply carries `status` (`ok` / `drain` / `stop`) and any revoked lease IDs |
| `lease` | `POST /v1/runners/me/leases` | `Bearer zrn_` | `runner/lease.zig` | long-poll for the next event; reply carries the event, resolved config, secrets, `lease_id`, `fencing_token` — or `null` + `retry_after_ms` |
| `report` | `POST /v1/runners/me/reports` | `Bearer zrn_` | `runner/report.zig` | terminal result for a lease; `zombied` persists + `XACK`s after a fencing check |
| `activity` | `POST /v1/runners/me/leases/{lease_id}/activity` | `Bearer zrn_` | `runner/activity.zig` | write-only progress stream for the live tail; best-effort, no ack |

`me` resolves from the token — no `runner_id` in any path or body, so there is nothing to spoof or reconcile. `register` is the one verb authed by a *human operator* credential; everything else is authed by the machine credential it mints. Identity and auth are covered in [`../AUTH.md`](../AUTH.md) (the runner is the first machine principal). `register` is gated by the `platform_admin` claim — only usezombie's platform operator may enroll a host into the shared fleet — so a tenant `admin` JWT or a `zmb_t_` api_key is rejected `403`.

## Registering a runner

A runner needs a `zrn_` token before it can pull work. The **platform operator pre-mints it** and installs it on the host — the host never self-registers (Option B, the GitLab-16 "create runner → authentication token" model). The operator calls `register` once with a Clerk JWT carrying `platform_admin`; `zombied` mints the `zrn_` and the operator drops it into the host's `ZOMBIE_RUNNER_TOKEN` env var. On boot the daemon validates the `zrn_` prefix (fail-loud, not a silent 401 loop) and goes straight to the heartbeat/lease loop — no register call, so no host ever holds an enrollment-grade credential. There is no enrollment token; the operator-as-minter must hold `platform_admin`. The open-fleet, self-enrolling case is mode C, later.

```
 platform operator                                       zombied
 (Clerk JWT, metadata.platform_admin=true)
   │ POST /v1/runners                                🔒 GATE 1 — who may enroll:
   │   Authorization: Bearer <Clerk-JWT>             platform_admin claim required
   │   { host_id, sandbox_tier, labels[] }           (tenant admin / zmb_t_ → 403)
   ├────────────────────────────────────────────────►│ mint zrn_ (256-bit random)
   │                                                  │ store ONLY sha256(zrn_) in fleet.runners
   │◀──────────────────────────────────────────────────┤ 201 { runner_id, runner_token: zrn_ }  (shown once)
   │ operator installs zrn_ on the host (env ZOMBIE_RUNNER_TOKEN)
   ▼
 host: zombie-runner
 (env ZOMBIE_API_URL + ZOMBIE_RUNNER_TOKEN=zrn_…)
   │ boot: validate zrn_ prefix, NO register call
   │ steady loop — Authorization: Bearer zrn_         🔒 GATE 2 — per-call auth:
   │      ◀── heartbeat · lease · report · activity ─┤ sha256(Bearer) == token_hash (timing-safe)
   │      eligibility: sandbox_tier + scope + secret_delivery   🔒 GATE 3 — blast radius
```

`zombied` owns the Postgres pool, the Redis pool, and the Vault API; `zombie-runner` owns none of them and holds only the `zrn_` token. Rotating a token swaps `token_hash`; revoking sets `status='revoked'` so the next call gets a 401. The runner's env is `ZOMBIE_API_URL` + `ZOMBIE_RUNNER_TOKEN` (matching the `zombied` / `zombiectl` convention), and `ZOMBIE_RUNNER_TOKEN` holds the operator-minted `zrn_` directly — there is no bootstrap credential on the host and no datastore secret.

## Running one event (NullClaw)

A `lease` reply is the runner's entire input for an event. The runner forks a sandboxed child, the child runs NullClaw, and the result goes back via `report`.

```
lease → { event, ExecutionPolicy(config + secrets_map + network_policy + tool_allowlist),
          lease_id, fencing_token, checkpoint? }
   │
zombie-runner parent (child_supervisor.zig): establish the cgroup, fork, exec self as
   `zombie-runner __execute` under bwrap (unshare-all + ro-system + rw-workspace), feed the
   lease over the child's stdin, read framed frames off its stdout under the lease deadline
   │
   └─ sandboxed child (child_exec.zig): apply mandatory Landlock, build config + tool set from
      the policy, run the NullClaw turn — language-model calls + tool calls, secrets substituted
      at the tool bridge — emit activity frames + the final result over stdout
   │
report → zombied: persist terminal state + telemetry + checkpoint, then XACK
```

The executor's TOCTOU (Time-Of-Check-To-Time-Of-Use) guards — lease re-check before a stage, orphan reaping, idempotent destroy — moved inside the runner as parent↔child supervision: the parent reaps orphan-safe, kills the cgroup tree on a deadline overrun, and `destroy()`s idempotently. The durable lease guard lives in `zombied` via `lease_expires_at` + `fencing_token` (see **Reclaim** below). The fork model is **fork-then-exec-self under bwrap**: bwrap owns the unprivileged user/network-namespace dance (raw `unshare` needs privilege) and gives the child a clean address space.

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

NullClaw emits progress frames mid-run (tool started, response chunk, tool completed). The runner holds no Redis, so the child emits frames over its stdout pipe (`src/runner/pipe_proto.zig`, length-prefixed typed frames: `A` = activity, `R` = result, multiplexed because stdout crosses bwrap cleanly); the parent forwards each `A` frame to `zombied` over the `activity` verb, and `zombied`'s `fleet/service_activity.zig` translates it to the `PUBLISH` on `zombie:{id}:activity`. Downstream Server-Sent Events (SSE) is unchanged.

```
NullClaw child ─pipe(A frames)─► runner parent ─POST .../activity (no ack)─► zombied ─PUBLISH─► SSE
```

Two planes, kept apart on purpose: **activity** is ephemeral and best-effort (a dropped frame is cosmetic); **report** is the durable system of record. The live tail is never the source of truth. The bracket frames (`event_received` at lease, `event_complete` at report) are published by `zombied` itself, so the tail has open/close markers even before the runner forwards a single mid-run frame.

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

`zombied` resolves config fresh from Postgres on every `lease`, so config changes take effect on the **next command** (the next lease) with no signaling. There is no in-memory config cache and no `zombie_config_changed` consumer to wait on — the deleted worker's watcher-reload path is gone. A config change never alters a language-model turn already in flight; the next stage picks it up.

## Money gates

The credit-pool billing model debits twice per event, and both debits live on `zombied`'s lease path — the runner never touches billing.

- At **lease issue**, before handing work to a runner: the balance gate (does the tenant cover the receive + stage estimate?), then the `receive` debit (flat, posture-based), then the approval gate, then the `stage` debit (a conservative estimate at floor tokens). Any gate failure means no lease is issued.
- At **report**: reconcile the stage telemetry row to the actual token counts. The charged amount stays at the pre-execution estimate — report updates telemetry, it does not re-charge.

Receive credits are not refunded if the stage later exhausts. This mirrors the deleted `metering.zig` exactly; only the caller moved from the worker to `zombied`'s lease/report path.

## Redis topology — what changed

The pre-cutover runtime had three Redis surfaces. The split keeps two (shifting their producer/consumer to `zombied`) and retires one.

| Surface | Before | Now |
|---|---|---|
| `zombie:{id}:events` (work stream, group `zombie_workers`) | the per-zombie worker thread was the consumer (`worker-{host}-{ts}`); blocking `XREADGROUP`, `XAUTOCLAIM`, `XACK` | **`zombied` is the consumer.** `lease` does a non-blocking `XREADGROUP` on the request thread; `report` does the `XACK`. The runner is not a Redis consumer. |
| reclaim of a dead processor | `XAUTOCLAIM` by consumer idle (5 min) — a dead worker was a dead consumer | **lease expiry + `fencing_token`.** A dead runner is *not* a dead Redis consumer (`zombied` is), so consumer-idle can't see it. The lease layer is the reclaim mechanism. |
| `zombie:control` (control stream) | the watcher consumed `zombie_created` / `zombie_status_changed` / `zombie_config_changed` / `worker_drain_request` to spawn / cancel / reload per-zombie threads | **removed.** There are no per-zombie threads to orchestrate: created is moot, status/config live in Postgres + are read fresh per `lease`, drain is the heartbeat reply. The producer (`control_stream.publish`) and the dead `control_stream` module were deleted; install keeps only `redis_zombie.ensureZombieConsumerGroup` (the lease `XREADGROUP` needs the events group present). |
| `zombie:{id}:activity` (pub/sub) | the worker `PUBLISH`ed; SSE handlers subscribed | same channel + SSE; **`zombied` `PUBLISH`es** — bracket frames directly, mid-run frames fed by the runner's `activity` stream. |

The reclaim shift is the load-bearing one: moving the processor off-platform means Redis can no longer observe its death, so the durable lease (`lease_expires_at` + `fencing_token`, frozen in M80_001) replaces `XAUTOCLAIM`.

## Sandbox tiers

A runner reports its isolation strength at registration. Assignment (and later the scheduler) refuses to place other-tenant or production work on a weak tier. The reported tier is telemetry; trust for placement is operator-assigned, not self-claimed. A production startup guard refuses `dev_none` (or an unknown tier) in a release build, so the weakest tier cannot become the production default.

| `sandbox_tier` | Where | Eligible for |
|---|---|---|
| `landlock_full` | Linux host | any work |
| `container_nested` | runner inside a container on a Linux host or VM (Virtual Machine) | any work — full sandbox, nested |
| `macos_seatbelt` | macOS, Seatbelt profile (weaker) | own-tenant / dev work |
| `dev_none` | no real sandbox; refused in release builds | own-tenant dev work |

On a Mac, running `zombie-runner` inside a Linux VM (Docker Desktop / OrbStack / Lima) is how a laptop earns `container_nested` instead of the degraded `macos_seatbelt`.

## Scaling

The split inverts the binding constraint. The pre-cutover runtime needed N Redis connections for N zombies and the pool ceiling was the wall. After the split, runners hold zero datastore connections; the bottleneck becomes `zombied` API replicas + Postgres writes, both of which scale horizontally. Runners scale out with no coordination — the operator enrolls a host with a pre-minted `zrn_`, and it pulls. The one piece needing care at multi-replica scale is placement (assignment / scheduler), which is the M80_006/007 concern; the hot path (lease / report) is shardable. See [`scaling.md`](./scaling.md) for the re-derived connection math.

## What does not change

- NullClaw's agent loop, its tool inventory, and secret substitution at the tool bridge. It moved into the runner as a linked engine and a sandboxed child, but its behaviour is identical.
- Event ingress: steer / webhook / cron / continuation still `XADD zombie:{id}:events`.
- The user read path: `GET /events`, the SSE live tail, `zombiectl status/events`.
- The three durable stores and their contracts (see `data_flow.md`), including row-for-row equivalence with the deleted direct path (Invariant 2 of the cutover spec).

## Roadmap (M80 workstreams)

```
 S0  M80_001  KEYSTONE   DONE — froze /v1/runners protocol + fleet schema + auth plane + lease/report handlers
 S1–S4         CUTOVER    DONE — absorbed into M80_002: zombied assignment + fencing + reclaim over PG + Redis;
                                 the zombie-runner binary with NullClaw folded in + fork-sandboxed child;
                                 thin zombied (direct worker path + executor sidecar deleted); TLS transport;
                                 data_flow.md / capabilities.md / scaling.md reconciled here
 ── cutover landed: the runner is the processor; the old direct path is gone ──────────────────
 S5  M80_004  PLATFORM   macOS Seatbelt backend + distribution / CI + runner CLI                  (pending)
 S5  M80_005  IDENTITY   DONE — platform_admin gate on enrollment (POST /v1/runners) + Option B host
                                 (operator pre-mints zrn_, no self-register); trust_class +
                                 allowed_workspace_ids + trust-gated placement deferred to M80_007
 S5  M80_006  FLEET      node inventory + heartbeat-driven reassignment + PER-LEASE RENEWAL        (pending, MANDATORY)
                         (closes the > 30 s renewal gap; sub-10 s recovery; cordon / reaping)
 S6  M80_007  SCHEDULER  placement on labels / capacity / sandbox-tier + trust-gated placement
                         (trust_class + allowed_workspace_ids); autoscale by queue depth           (later)
 ── later ─────────────────────────────────────────────────────────────────────────────────────
     mode C    self-enrolling runners — the open "run it on your own host" case
```

M80_003 was superseded — its thin-worker slice landed inside the M80_002 cutover. M80_006 is reframed **mandatory, not optional**: heartbeat-renewed leases + decoupled liveness/execution TTL + sub-10 s recovery + cordon/reaping are the path out of the S0 lazy-reclaim SLA and the > 30 s renewal gap.
