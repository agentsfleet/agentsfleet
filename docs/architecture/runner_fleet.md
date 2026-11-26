---
type: explanation
audience: contributor
verified: 2026-11-25
product_version: 0.90.0
executable: false
---

# Runner fleet

## What it is

A runner is a host process, `agentsfleet-runner`, that runs fleet turns for `agentsfleetd`. This page covers the daemon's side: how runners enrol, take policy, report health, receive work and get replaced.

| Question | Answer |
|---|---|
| What a runner holds | A runner token (`agt_r`), its assigned policy and the runs it is working on |
| What it never holds | A Postgres, NATS or vault credential |
| How it gets work | The daemon pushes runs over one stream ([runner protocol](runner_protocol.md)) |
| Where its policy comes from | The daemon's runner row, sent on every heartbeat reply |
| What stops a stale runner | A fencing token that rises with every assignment |
| What happens when it dies | Its lease expires and another runner takes the turn |

```text
+- PLATFORM ------------------+          +- HOST: bare metal or a VM ------------+
| agentsfleetd                |  native  | agentsfleet-runner                    |
|  owns Postgres, NATS, vault |<-------->|  supervisor: stream, agent loop       |
|  assignment, fences, leases |  stream  |  one sandbox per run for tool calls   |
+-------------+---------------+   agt_r  +---------------------------------------+
              |
   Postgres . NATS . vault  (never reachable from a host)
```

A fleet is an agent with its own thread, memory and access, such as `morning-rooster` in [the fixture fleets](../../tests/fixtures/fleets/README.md). One live run per fleet holds its thread at a time.

## Why it exists

Fleets run tenant programs, so the platform must run them on hosts it may not fully trust. A runner holds no datastore credential, so a compromised host cannot reach Postgres, NATS or the vault.

Runners are replaceable. Correctness never waits on one runner being alive; any eligible runner can take the next turn.

Two independent layers guard the runner tables (`runner.runners`, `thread.runs`, `thread.threads`):

| Layer | Mechanism | Answers |
|---|---|---|
| Application authorization | Token scopes such as `runner:register` | Which caller may enrol or manage runners |
| Datastore identity | The `api_runtime` Postgres role | Which process writes the rows |

The daemon writes every `runner.*` row on the runner's behalf. There is no `runner_runtime` role, because such a role would claim that runners connect to Postgres.

## How it behaves

### Registering a runner

The host never registers itself. An operator holding `runner:register` mints the token and installs it on the host.

```bash
agentsfleet platform runners register --host-id <HOST_ID> --policy <POLICY_FILE> --token-out <TOKEN_FILE>
```

```text
<varies>
```

`<HOST_ID>` names the machine, `<POLICY_FILE>` holds its assigned policy, and `<TOKEN_FILE>` is a new owner-only file for the token. The command calls `POST /v1/admin/runners` and reveals the token once; the daemon stores only its hash.

A second registration of the same host answers `409`, and its `Location` names the existing runner. The operator then rotates that runner's token instead.

The host's environment holds the token and the stream settings ([runner protocol](runner_protocol.md#settings-and-certificates)). It holds no policy and no bootstrap credential.

A holder of `runner:write` rotates a token with `agentsfleet platform runners rotate-token`. The old token fails its next authentication. Revoking sets the runner's status to `revoked`.

### A tenant's own runner

A tenant holding `runner:tenant` registers its own host with `agentsfleet runners register`, which calls `POST /v1/tenants/me/runners`. The row records the tenant, and nothing changes it later.

A tenant runner takes only its own tenant's work, and never a fleet on the platform's model key. The platform key therefore never reaches a tenant-controlled host.

```text
                              runner's tenant
work of tenant T              none (platform)    T          U (another tenant)
fleet on the platform model   taken              refused    refused
fleet on T's own model        taken              taken      refused
```

The candidate scan, the assignment and the final issue under the host lock all apply this table. A refusal at issue logs `runner_placement_refused` (`UZ-RUN-027`).

Labels place work and carry no security meaning. A platform admin can cordon, drain, revoke and relabel a tenant runner, but only the tenant rotates its token (`UZ-RUN-026`).

### Assigned policy and capability

Policy flows down. Sandbox tier, network policy, registry allowlist, worker count and the durable-task switch live on the runner row.

An operator changes them with `agentsfleet platform runners update --policy`. Every heartbeat reply carries the current policy, so a change reaches the host within one beat.

Capability flows up. At boot the runner probes Landlock, seccomp, its delegated control groups (cgroups), bubblewrap, the toolbox filesystem, `/dev/kvm` and egress enforcement.

Every heartbeat carries that probe report. The daemon compares assigned against achievable and writes `degraded_reason` when the host falls short ([reconcile](../../rustd/crates/daemon/afd_runner/src/reconcile.rs)).

Every heartbeat also carries `host`: the processors the runner may use, its total memory, and its free disk. The free disk is on the file system that holds its sandboxes ([host facts](../../rustd/crates/runner/afr_supervisor/src/capability/host.rs)).

The daemon keeps the latest value of each on the runner row, and `agentsfleet platform runners list` and `show` print them. Memory comes from Linux's `sysinfo(2)`, so a runner built for another system reports none.

The check fails closed on both sides. The daemon issues nothing to a degraded or unreadable row, and a runner with no readable policy takes no work.

A policy change re-checks the stored report inside the same request. A tightening the host cannot meet degrades the row at once; runs already issued finish under their old policy.

The heartbeat interval travels in the welcome, never in the heartbeat reply. A changed interval reaches a host when it reconnects.

A runner whose durable-task switch is off takes no new durable run. It is never offered `task_start` or `task_input` ([durable tasks](durable_tasks.md)).

### Sandbox tiers

The tier is an assigned attribute that sets what the daemon demands of the probe report ([runner types](../../rustd/crates/protocol/af_wire/src/runner/mod.rs)).

| `sandbox_tier` | Where | What the daemon demands |
|---|---|---|
| `landlock_full` | A Linux host | Landlock, seccomp, bubblewrap and every required cgroup controller |
| `container_nested` | A runner inside a container on Linux | The same set; the sandbox applies Landlock on every tier |
| `dev_none` | Development | Nothing; any network policy except `allow_all` reads degraded, because only a sandbox enforces one |

The runner builds the same bubblewrap sandbox under every tier ([runner runtime](runner_execution.md#sandbox-engines)). The tier sets isolation strength only; network reach is the separate [egress policy](runner_execution.md#network-egress). On a Mac the runner runs inside a Linux virtual machine.

### Runner state

A runner's status is four separate facts, kept in typed columns and an event log.

| Fact | Where it lives | Values |
|---|---|---|
| Operator intent | `runner.runners.status` | `active`, `cordoned`, `draining`, `drained`, `revoked` |
| Automatic quarantine | `cordon_reason`, `cordoned_at`, `cordon_healthy_since_at` | `missed_heartbeats` |
| Liveness | Derived on read from `last_seen_at` and live runs | `registered`, `online`, `busy`, `offline` |
| History | `runner.history`, append-only | `runner_registered`, `runner_online`, `run_acquired`, `run_released`, `runner_revoked` and more |

A fresh token stores `last_seen_at = 0`, so a runner that never connected reads `registered`, not `online`. A failed authentication matches no row and is not a runner state.

A connection that ends with no successor arms one visit at its last beat plus the offline window, 114 seconds by default. The visit writes `runner_offline` and cordons an `active` host for missed heartbeats ([silence](../../rustd/crates/daemon/afd_runner/src/silence.rs)).

The cordon lifts after 30 seconds of continuous healthy heartbeats, and the daemon writes `runner_uncordoned`. A gap of two heartbeat intervals restarts that count. Lifting never clears an operator's cordon, drain or revocation.

Operators holding `runner:read` inspect runners from the command-line interface (CLI):

```bash
agentsfleet platform runners show <RUNNER_ID>
```

```text
<varies>
```

`<RUNNER_ID>` comes from `agentsfleet platform runners list`. `history <RUNNER_ID> --event-type runner_cordoned,runner_uncordoned` lists events, and `runs <RUNNER_ID>` lists runs.

### Capacity and sandbox holds

An assignment locks the runner row, counts live runs, and inserts the run in that transaction. A sandbox hold takes no worker, so the issue admits the free workers the runner's credits offer. The daemon owns `worker_count`, from 1 to 64 (`af_core::limits::MAX_WORKERS`).

Lowering the count below current occupancy stops new assignments and refuses renewals until occupancy fits. [Concurrency](concurrency.md) owns the lock order.

A run that ends processed leaves its sandbox held for the same fleet's next run ([held sandboxes](runner_execution.md#workspace-between-runs)). The report's acceptance narrows the hold to `[runner.sandboxes] max_idle_hold_seconds`, 600 seconds by default.

Every heartbeat lists the fleets the runner still holds. Other runners skip a fleet held by an active, non-degraded runner seen within the offline window.

The holder reuses its hold only when the fleet's last run ran there, the hold is unexpired and the turn is not a reclaim. Otherwise any eligible runner takes the turn in a fresh sandbox.

### Placement

The candidate query prefers held fleets, then a recent wake hint, then the runner's previous fleet. The rest go by how long each head has waited while its thread was free. That age is the later of its queued time and its thread's last release.

A head a pass freed without a run, after a fault or a refusal, so waits behind turns queued before that release. Runners that read the same order skip each other's locked rows at the assignment.

A fleet's `required_tags` must be a subset of the runner's labels. Tenant ownership, cordon state and capacity are checked again under the host lock.

The `thread.turns` ledger owns the work; the wake hint only orders candidates ([datastore design](datastore_scaling.md#the-turns-ledger-is-the-queue)).

### Failure recovery model

Every run carries a monotonic fencing token. A report from an older fence is refused (`UZ-RUN-005`) in the same statement that would settle the run.

| Failure | Recovery | Guard |
|---|---|---|
| Runner dies mid-run | A later assignment takes the unsettled turn after the 38-second lease expires | The turn's fence rises; the old runner cannot write |
| A turn's runs keep lapsing | After `[turns] max_runs_per_turn` runs, three by default, the next assignment ends the turn as `fleet_error` with `run_expired` | The same transaction settles the turn, so the fleet's next turn can run |
| Wake notification lost | The turn waits for the next event that starts an assignment pass | The turn is already committed in Postgres |
| The issue refuses an assigned run for a full host, a lapsed assignment or a moved placement choice | The release rings the fleet's wake, so the turn is the next pass's hint | `run_assignment_lost` names the guard as `reason`, with the host's census on a refused insert |
| Report fails before commit | The runner retries the report | No charge or settlement without the report transaction |
| Report answer lost | The runner retries and gets the stored outcome | A repeated report adds no usage |
| Sandbox build fails | The turn keeps going; sandbox calls answer `sandbox_unavailable` | Nothing runs outside a sandbox |
| Admission check fails | The run reports `fleet_error` as `startup_posture` before the turn runs | Egress and checkout plans are checked first |

The three-run cap counts every run a turn was issued, whatever ended it. A durable task's turn is exempt, because its own run allowance bounds it.

The replacement restores the turn's latest checkpoint when resume is on ([checkpoints](checkpoints.md)). The [runner protocol](runner_protocol.md#run-lease-and-renewal) owns the lease timing.

### Recovery after a lost wake

No timer looks for lost work. A recovery pass starts when the message bus reports a gap or lag, a wake subscription opens or drops, or a notification is unreadable.

A connection that ends with no successor sleeps once until the latest deadline of its live runs, then requests one pass. At boot the gateway reads the latest outstanding deadline and schedules one pass at it.

```text
recovery wake -> scheduler/recovery/drain.rs
                 queue of connected runners with free workers
                   |  up to 8 concurrent assignments, one per connection
                   v
                 oldest compatible turn first
                 issued + credit left  -> that runner rejoins the queue's tail
                 empty or uncertain    -> skip that runner for this pass
```

`[runner.scheduling] max_concurrent_recovery_assignments` sets the 8. A busy runner keeps its recovery request until a worker frees, and only an empty scan clears it ([drain](../../rustd/crates/daemon/afd_gateway/src/scheduler/recovery/drain.rs)).

### Fleet configuration freshness

A fleet's configuration is its `fleet.fleets` row and its approved grants. The policy build reads both for every run ([policy build](../../rustd/crates/daemon/afd_gate/src/policy/build/mod.rs)).

An edit therefore reaches the next run on every daemon replica, with no reload signal. A run already going keeps the policy it was issued.

### Run history

A runner's runs and lifecycle events are read through `GET /v1/admin/runners` and its sub-routes. The read derives liveness and never returns token hashes.

A settled run outlives its fleet's purge, with its fleet and turn fields set to null. Each item's `destination` says whether the caller can still open that fleet.

The dashboard's **Runs** tab shows one row per run; its **Activity** tab shows lifecycle events only. Per-runner counters are metrics, listed in [observability](observability.md).

## Limits

- A runner fleet is not a general scheduler. It has label placement and no autoscale, fairness engine, priority lanes, preemption or second workload type.
- Cordon and drain refuse the runner's authentication, renewals and reports included. Existing work can stop; neither action is a graceful finish.
- Nothing moves a `draining` runner to `drained`, and no operator action reaches `drained`.
- The capability report is the host's own claim. A compromised host can lie, so placement trust stays operator-assigned.
- Nothing deletes finished runs or runner events, and nothing moves an expired run out of `active` until its fleet's next assignment.
- No policy decides where work goes when every runner is down. The work waits in Postgres until capacity returns.

## Related pages

- [Runner protocol](runner_protocol.md): the stream, leases and refusals
- [Runner runtime](runner_execution.md): the supervisor, sandbox, network and credentials
- [Checkpoints](checkpoints.md): what a replacement runner restores
- [Data flow](data_flow.md): one turn end to end, live activity, steer, archive and delete
- [Memory](memory.md): what a fleet remembers and how a run carries it
- [Concurrency](concurrency.md): lock order for assignment and renewal
- [Billing](billing.md): how renewals and reports charge the payer
- [Observability](observability.md): per-runner metrics and logs
