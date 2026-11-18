---
type: explanation
audience: contributor
verified: 2026-11-18
product_version: 0.78.0
executable: false
---

# Platform runner checkpoint recovery

A platform runner resumes an interrupted event from its last complete checkpoint after the previous lease expires. PostgreSQL holds encrypted ordinary-turn conversation history. The private object store holds encrypted workspace chunks and native task history.

The completed specification records the agreed scope. The review ledger retains measured timings and remaining acceptance checks. [M247 specification](../v2/done/M247_001_P1_API_CLI_DOCS_OBS_UI_FAILED_HOSTS_RESPAWN_FROM_CHECKPOINT.md) · [Verification ledger](../v2/reviews/m247-review-ledger.md)

## Deployment settings

The shared loader is `rustd/crates/foundation/af_config`. It reads an optional
TOML file, merges explicit invocation overlays, deserializes the caller's
settings type and validates that type with `garde`.

```text
daemon ConfigToml --+
                   +--> af_config::ConfigBuilder --> validated application type
runner type -------+                                      |
                                                          +--> service construction
```

The loader accepts an injectable `FileSource`. It depends on foundation code
and parsing libraries.

The daemon owns its fields, environment names and
policy validation in `daemon/main/src/config` and `preflight`. Its existing
`BootConfig` carries the resolved settings into the stores and scheduler.

The pattern follows the Codex configuration builder reviewed at
`ca21fe65d18a9b6326759fb4263c65dfbb645aa5`. Invocation overlays merge before
typed deserialization. [Reference loader](https://github.com/openai/codex/blob/ca21fe65d18a9b6326759fb4263c65dfbb645aa5/codex-rs/config/src/loader/README.md)

Select the daemon file with `--config <FILE>` or `AGENTSFLEET_CONFIG_FILE`.
For supported settings, an explicit CLI value wins, followed by a present
TOML field, its environment value, and the current default. Secrets resolve
through the environment or the platform vault.

The daemon's normal
no-subcommand preflight reads the same settings as serving.

The Fly application files live beside their respective `fly.toml` files.
Fly writes the selected file at `/etc/agentsfleet/agentsfleetd.toml`.

Both deployment files set `runner.recovery.resume_from_checkpoint = false`.
A configuration change takes effect when each daemon replica restarts.

Environment-only startup defaults to resume enabled. Set the flag explicitly
for a staged rollout and confirm its effective value on every replica.

With resume disabled, an ordinary admission with a committed checkpoint
waits at its fleet's head after its lease expires. Candidate selection and the
locked claim both check the setting.

Enabling resume permits a new fenced
claim. Active leases retain their stored deadlines and can reconnect.
Checkpoint capture continues while resume is disabled.

The deployment clock controls grants and renewals and derives the offline
threshold as three lease lifetimes. Its default is 35 seconds.

Startup
requires a value greater than the existing 32-second renewal/reconnect budget
and below the twelve-hour runtime ceiling. The recovery probe interval must
be positive and shorter than the lease lifetime.

The configuration extension is under verification. Source inspection and
focused checks are recorded in the [review ledger](../v2/reviews/m247-review-ledger.md#proposed-agentsfleetd-deployment-configuration).

## What gets checkpointed

The runner saves progress automatically at a safe boundary before a continuing model call. The root contains the conversation, accepted driver answers, completed child results, local memory changes, and the exact input folds it covers.

Model policy and credentials come from the new lease.

| Workload | Private state | Durable owner |
|---|---|---|
| Ordinary API-only turn | Encrypted history root in PostgreSQL | Event head in `runner.turn_recovery` |
| Ordinary turn with workspace files | Same history root plus an ordered manifest and encrypted private object-store chunks | Same event head |
| Native durable task | Task history and file manifest with its bound base and overlay | Existing task checkpoint and task generation |

The agent requests capture after a completed turn and input reconciliation. Busy children, open executor sessions, unfinished file work, and live tenant processes can defer it.

File capture requires a verified toolbox and an available runner transfer permit before the agent builds the snapshot. [Agent boundary](../../rustd/crates/runner/afr_agent/src/loop/recovery_boundary.rs) · [Supervisor admission](../../rustd/crates/runner/afr_supervisor/src/recovery/admission.rs)

```text
Harness::save_boundary                         recovery_boundary.rs
  |-- optional file admission busy -> skip -> keep previous ready head
  `-- admitted -> BoundaryOwner::capture       recovery_capture.rs
         -> freeze and prove quiescence
         -> scan, seal, authorize, PUT, authenticate readback
         -> publish exact root and manifest under lease fence
         -> thaw -> acknowledge saved generation
```

A pending generation becomes usable when its root and required objects are fully verified and its head advances in PostgreSQL. An uncertain publication is reconciled against the original operation and generation.

If a refused replay leaves a pending generation, the owner must prove exact abandonment before starting another capture. [Publication reconciliation](../../rustd/crates/runner/afr_supervisor/src/recovery/turn/publication.rs)

The complete ordinary native message is capped at 1 MiB. A file capture can describe up to 24 GiB or 200,000 entries, with at most 64 manifest pages of 512 KiB each.

Newly uploaded ciphertext is capped at 16 GiB and counts toward the 50 GiB workspace quota. [Wire limits](../../rustd/crates/protocol/af_wire/src/lease/turn_checkpoint/command.rs)

## What happens when a host fails

```text
A (16 workers)  F01..F16 running, each at fence N
B (16 workers)  compatible, idle and connected
                 |
A stops          +-> no more renewals
                 |
saved deadline   +-> event is eligible after its old lease and slot expire
                 |   -> B claims the event under fence N+1
                 |   -> B starts renewal for the new lease
                 |   -> exact checkpoint is restored and checked
                 |   -> covered folds are adopted under fence N+1
                 |   -> first resumed model request -> one fenced settlement
                 |
heartbeat sweep  `-> separate offline record and automatic quarantine
```

The 35-second lease TTL starts at the last grant or renewal. It governs reclaim.

A separate sweep quarantines a host after 105 seconds without a valid heartbeat. [Lease and liveness timing](../../rustd/crates/foundation/af_core/src/timing/mod.rs) · [Runner status](runner_fleet.md#failure-recovery-model)

The lease TTL includes a 5-second tick, 5-second answer window, 7-second reconnect, and 15-second renewal margin. The compile-time sums require 32 seconds within the 35-second lease, and 14 seconds for the second answer and physical cleanup within that 15-second margin.

Host quarantine tolerates three lease lifetimes of heartbeat silence and is evaluated by the paced 10-second sweep. [Renewal decision](../v2/done/M243_001_P1_API_DOCS_INFRA_OBS_RUNNERS_HOLD_ONE_PUSH_STREAM.md#renewal-recovery-budget-correction) · [Liveness sweep](../../rustd/crates/daemon/afd_runner/src/sweep/liveness/mod.rs)

```text
brief link break + reconnect before expiry -> same lease and fence
expiry + other eligible capacity           -> higher fence on replacement
expiry + capacity still busy               -> admission waits for a later offer
replacement + ready head                   -> restore root and matching files
replacement + absent head                  -> original input and unsettled folds
replacement + invalid head or file         -> stop before model or tools
return + 30 s of continuous healthy beats  -> automatic quarantine lifts
```

After dispatch, a required file restore waits for runner admission while lease renewal continues. The supervisor validates the exact event, setup, generation, root, and toolbox.

It writes files through checked directory descriptors and applies modes before thawing. [Ordinary restore](../../rustd/crates/runner/afr_supervisor/src/recovery/turn/restore.rs) · [Workspace preparation](../../rustd/crates/runner/afr_supervisor/src/lease_loop/turn_recovery.rs)

The new attempt replaces the provisional answer in the Dashboard. Stale frames carry the old fence and cannot change the current attempt.

The final result and usage settle through the report transaction. [Result and lease flow](lease_flow.md) · [Runner stream protocol](runner_protocol.md)

## How a recovery burst drains

A disposable wake can be lost between durable admission and its notification. The replica's independent five-second probe supplies a recovery generation.

Ordinary rings retain their successful-issue budget and fleet locality.

```text
Ordinary ring -> scheduler/mod.rs::pass -> local fleet preference + ring budget

Recovery wake -> scheduler/recovery/mod.rs::notifications
              -> scheduler/recovery/drain.rs::drain
                 queue [B:s1, C:s2, D:s3, ...]
                   |  up to eight concurrent claims across sessions
                   |  one offer in flight per session
                   v
                 session/supervisor.rs::offer -> lease/assign/ledger.rs::ledger_counted
                   |  oldest compatible admission first
                   |  full unclaimable page -> keyset seek, bounded retries
                   v
                 issued + remaining credit -> rotate that session to the tail
                 empty or uncertain reply   -> defer that session for this pass
```

Credits at the start of the drain bound how much work a session can acquire. PostgreSQL is the final authority on capacity and ownership.

A completed drain acknowledges only its original recovery generation. [Recovery drain](../../rustd/crates/daemon/afd_gateway/src/scheduler/recovery/drain.rs) · [Candidate SQL](../../rustd/crates/daemon/afd_fleet/src/lease/sql/lease/select_ledger_candidates.sql)

After skipping a full locked page, the session revisits its prefix after a short empty page or 64 later claims. This bounds the delay for older rows that become eligible while newer work stays ready. [Locked-page regression](../../rustd/crates/daemon/afd_fleet/tests/integration_ledger_queue/recovery_page.rs)

The previous recovery pass spent one successful-offer credit per fallback probe. The new drain lets B use its available worker credits after the first probe, and a stalled session can leave other sessions moving.

The benchmark ledger records the observed throughput separately. [Measured comparison](../v2/reviews/m247-review-ledger.md#performance-and-coverage-acceptance)

## Transfer and database limits

Each runner shares one HTTP client and admits up to two file or task recovery operations. An optional file capture can skip a busy pool before freeze.

Required restores queue and retain their reservation through their last blocking write.

```text
B:w01..w16 -> Lessee.recovery
                |-- admitted #1 -> one window, up to four physical transfers
                |-- admitted #2 -> one window, up to four physical transfers
                |-- required restore -> wait with lease renewal
                `-- optional file capture -> skip safely when busy

F01 file a [c,d] + file b [a,b]
   -> one restore authorization for the chunk window
   -> authenticated byte owners [a,b,c,d]
   -> ordered handles [c,d,a,b] -> checked writes -> release owners
```

A restore window holds up to four 8 MiB chunks and can cross a file boundary. Four full chunks charge 32 MiB of plaintext through the writer's last use.

Two full windows therefore account for up to 64 MiB of runner plaintext payload before transport and runner overhead. [Restore window](../../rustd/crates/runner/afr_supervisor/src/recovery/window.rs)

For newly uploaded chunks, the runner stages up to four maximum-size ciphertexts or a bounded page of smaller objects. One authorization supplies create-only transfers.

One subsequent readback command authenticates the batch before commit. [Upload batch](../../rustd/crates/runner/afr_supervisor/src/recovery/uploads.rs)

| Contention case | Mutation boundary |
|---|---|
| F01 and F02 checkpoint independently on B | Share the host authority lock. Rotation and exclusive capacity changes wait for those short transactions. |
| F01 and F02 restore already verified objects in workspace W | Recheck lease, pin, and scope. Lock quota only when newly verified upload receipts change accounting. |
| F16 reports a sandbox hold as its lease expires | Keep shared host authority from before the hold decision through commit. Competing issue serializes with the report. |
| A four-chunk authenticated readback needs more time | Apply a 35-second answer budget with 25 seconds of daemon work, plus limits of two in-flight verifications per session and eight per replica. |
| The oldest checkpoint owner is locked | Defer it and advance the indexed cursor, keeping later maintenance work moving. |

Remote object I/O runs outside the checkpoint transaction. The return path checks the pin deadline and rechecks custody before recording verified receipts.

[Transfer guard](../../rustd/crates/daemon/afd_fleet/src/lease/turn_checkpoint/transfer.rs) · [Report SQL](../../rustd/crates/daemon/afd_fleet/src/lease/sql/commit_report.sql) · [Maintenance](../../rustd/crates/daemon/afd_fleet/src/lease/turn_checkpoint/maintenance.rs)

The product database is development-only. Columns and indexes are defined in their owning initial schema files.

Fresh development rigs apply those definitions from an empty database. [Recovery schema](../../schema/20261117154626_runner_turn_recovery.sql) · [Checkpoint schema](../../schema/20261117154627_runner_turn_checkpoints.sql)

## Operator and lab checks

Operators can use `agentsfleet platform runners list`, `agentsfleet platform runners show <RUNNER_ID> --json`, and `agentsfleet platform runners events <RUNNER_ID>` with `runner:read`. Runner details expose liveness, operator status, and optional automatic cordon.

[Public recovery guide](https://docs.agentsfleet.net/runners/recovery)

The one-fleet rig check uses registered runner processes and a controlled model. It waits for a complete source checkpoint, kills the source, verifies the replacement's higher fence and restored inputs, and waits for settlement.

Use an owned local rig whose database the target can reset.

```bash
make bench-lease PROFILE=rig BENCH_SHAPE=recovery BENCH_FLEETS=1 BENCH_WORKSPACES=shared
```

```text
→ [bench-lease] profile=rig shape=recovery
... Running benches/lease/main.rs (<varies>)
{"shape":"recovery","workload":"api_only","fleets":1,...}
```

The reported latency starts at the later of the old lease and slot expiries and ends at the first restored model request. It also records issue and settlement timestamps.

[Raw baseline](../../bench/baselines/lease-recovery-api.rig.json) · [Test and commands](../../rustd/crates/daemon/main/benches/lease/measure.rs)

## Implementation paths and symbols

The tables cover 68 production Rust and SQL paths and one test-only measurement adapter against `d4823563e80829b97589846a1c7997964fdf0274`. **New** marks added paths.

The daemon generated the [operator API reference](../../public/openapi.json). F16 changes browser test fixtures.

Benchmark drivers, test files, documentation and generated output are accounted separately from production sources.

### Recovery dispatch

| Repository path | Symbol or statement | Change |
|---|---|---|
| [`rustd/crates/daemon/afd_bus/src/wake/mod.rs`](../../rustd/crates/daemon/afd_bus/src/wake/mod.rs) | Wake::generations, recheck, recovered | Publish the recovery generation separately from ordinary ring counts. |
| [`rustd/crates/daemon/afd_bus/src/wake/queue.rs`](../../rustd/crates/daemon/afd_bus/src/wake/queue.rs) | WakeQueue | Keep fleet reminders in the locality queue and store generation counters in the watch. |
| [`rustd/crates/daemon/afd_gateway/src/scheduler/mod.rs`](../../rustd/crates/daemon/afd_gateway/src/scheduler/mod.rs) | notifications, pass | Preserve ordinary locality and ring budgets. Route recovery hints to bounded round-robin and acknowledge only the observed generation. |
| **New** `rustd/crates/daemon/afd_gateway/src/scheduler/recovery/drain.rs` | drain, Host | Rotate credited sessions using a queue and up to eight concurrent claims. An uncertain reply defers that host. |
| [`rustd/crates/daemon/afd_gateway/src/scheduler/recovery/mod.rs`](../../rustd/crates/daemon/afd_gateway/src/scheduler/recovery/mod.rs) | module declaration | Connect the drain to the existing independent probe. |
| [`rustd/crates/daemon/afd_gateway/src/session/offers.rs`](../../rustd/crates/daemon/afd_gateway/src/session/offers.rs) | OfferMode, OfferOutcome, SessionHandle::claim | Carry the selection mode to the session's existing offer owner and return its claim outcome. |
| [`rustd/crates/daemon/afd_gateway/src/session/mod.rs`](../../rustd/crates/daemon/afd_gateway/src/session/mod.rs) | SessionHandle | Type the claim channel for the selection mode and its reply. |
| [`rustd/crates/daemon/afd_gateway/src/session/runtime.rs`](../../rustd/crates/daemon/afd_gateway/src/session/runtime.rs) | session handle construction | Use the typed claim channel for registered physical sessions. |
| [`rustd/crates/daemon/afd_gateway/src/session/events.rs`](../../rustd/crates/daemon/afd_gateway/src/session/events.rs) | EventSource::next | Deliver queued claims to the supervisor with their original mode. |
| [`rustd/crates/daemon/afd_gateway/src/session/supervisor.rs`](../../rustd/crates/daemon/afd_gateway/src/session/supervisor.rs) | SessionSupervisor::offer | Pass the mode through the one in-flight offer and return its outcome. |
| [`rustd/crates/daemon/afd_gateway/src/session/offer.rs`](../../rustd/crates/daemon/afd_gateway/src/session/offer.rs) | SessionServices::offer, issue_offer | Pass recovery selection to fleet issue while preserving advisory credits. |
| [`rustd/crates/daemon/afd_fleet/src/lease/offer.rs`](../../rustd/crates/daemon/afd_fleet/src/lease/offer.rs) | OfferRequest | Carry ordinary or recovery selection context between gateway and fleet placement. |
| [`rustd/crates/daemon/afd_fleet/src/lease/mod.rs`](../../rustd/crates/daemon/afd_fleet/src/lease/mod.rs) | offer module exports | Expose the selection context alongside OfferRequest. |
| [`rustd/crates/daemon/afd_fleet/src/lease/pull/select.rs`](../../rustd/crates/daemon/afd_fleet/src/lease/pull/select.rs) | LeaseService::claim_fleet_with | Pass the selection context to the ledger. |
| [`rustd/crates/daemon/afd_fleet/src/lease/claim.rs`](../../rustd/crates/daemon/afd_fleet/src/lease/claim.rs) | LeaseService::claim_with | Pass the session-owned scan through native issue. |
| [`rustd/crates/daemon/afd_fleet/src/lease/assign/mod.rs`](../../rustd/crates/daemon/afd_fleet/src/lease/assign/mod.rs) | Leases::select and internal selection | Keep the ordinary entry point and implement bounded recovery selection. |
| [`rustd/crates/daemon/afd_fleet/src/lease/assign/ledger.rs`](../../rustd/crates/daemon/afd_fleet/src/lease/assign/ledger.rs) | Leases::ledger_counted, ledger_claim, record_claim | Advance past an unclaimable page and revisit older heads after bounded later claims. |
| [`rustd/crates/daemon/afd_fleet/src/lease/assign/measured.rs`](../../rustd/crates/daemon/afd_fleet/src/lease/assign/measured.rs) | Leases::select_recovery_measured | Expose the real scan count under `test-util` for the locked-prefix regression. |
| [`rustd/crates/daemon/afd_fleet/src/lease/sql/lease/select_ledger_candidates.sql`](../../rustd/crates/daemon/afd_fleet/src/lease/sql/lease/select_ledger_candidates.sql) | candidate query | Return a stable time/ID key. Seek beyond a blocked page while respecting each fleet's unsettled head and placement rules. |
| [`rustd/crates/daemon/afd_fleet/src/lease/store.rs`](../../rustd/crates/daemon/afd_fleet/src/lease/store.rs) | Leases::for_connection and host guard helpers | Scope disposable scan progress to the connection and expose shared host authority for checkpoint calls. |

### Checkpoint capture and restore

| Repository path | Symbol or statement | Change |
|---|---|---|
| [`rustd/crates/runner/afr_agent/src/engine/boundary.rs`](../../rustd/crates/runner/afr_agent/src/engine/boundary.rs) | RecoveryBoundarySink and capture permit | Reserve optional capture capacity before snapshot allocation. Return a typed skip on contention. |
| [`rustd/crates/runner/afr_agent/src/engine/mod.rs`](../../rustd/crates/runner/afr_agent/src/engine/mod.rs) | boundary exports | Expose the owned reservation to the existing agent/supervisor interface. |
| [`rustd/crates/runner/afr_agent/src/lib.rs`](../../rustd/crates/runner/afr_agent/src/lib.rs) | public boundary exports | Make the reservation available to the supervisor implementing the sink. |
| [`rustd/crates/runner/afr_agent/src/loop/recovery_boundary.rs`](../../rustd/crates/runner/afr_agent/src/loop/recovery_boundary.rs) | Harness::save_boundary | Take the reservation before cloning history, children and memory. |
| [`rustd/crates/runner/afr_supervisor/src/lib.rs`](../../rustd/crates/runner/afr_supervisor/src/lib.rs) | supervisor startup | Build shared transfer and admission resources once for all platform workers. |
| [`rustd/crates/runner/afr_supervisor/src/lease_loop/lessee.rs`](../../rustd/crates/runner/afr_supervisor/src/lease_loop/lessee.rs) | Lessee | Own the transfer client and admission limits for the runner process. |
| [`rustd/crates/runner/afr_supervisor/src/lease_loop/recovery_boundary.rs`](../../rustd/crates/runner/afr_supervisor/src/lease_loop/recovery_boundary.rs) | Sink, Job, BoundaryOwner | Move the reservation to the job that owns cleanup, including on caller cancellation. |
| [`rustd/crates/runner/afr_supervisor/src/lease_loop/recovery_capture.rs`](../../rustd/crates/runner/afr_supervisor/src/lease_loop/recovery_capture.rs) | BoundaryOwner::capture, LeaseRun::reserve_terminal_capture | Keep the reservation through capture and thaw. |
| [`rustd/crates/runner/afr_supervisor/src/lease_loop/recovery.rs`](../../rustd/crates/runner/afr_supervisor/src/lease_loop/recovery.rs) | LeaseRun::prepare_recovery and task capture/restore callers | Use cancellable admission and shared transfers for required task recovery. |
| [`rustd/crates/runner/afr_supervisor/src/lease_loop/hold.rs`](../../rustd/crates/runner/afr_supervisor/src/lease_loop/hold.rs) | LeaseRun::keep | Wait for terminal capture capacity before freezing. |
| [`rustd/crates/runner/afr_supervisor/src/lease_loop/task_workspace.rs`](../../rustd/crates/runner/afr_supervisor/src/lease_loop/task_workspace.rs) | LeaseRun::native_workspace, native_base | Wait for required base capture or restore before modifying the task workspace. |
| [`rustd/crates/runner/afr_supervisor/src/lease_loop/turn_recovery.rs`](../../rustd/crates/runner/afr_supervisor/src/lease_loop/turn_recovery.rs) | LeaseRun::prepare_turn_recovery | Acquire required file restore admission before freeze and pin acquisition. |
| [`rustd/crates/runner/afr_supervisor/src/lease_loop/work.rs`](../../rustd/crates/runner/afr_supervisor/src/lease_loop/work.rs) | ordinary API-only restore call | Supply shared resources to NativeTurn while keeping the no-files path lightweight. |
| [`rustd/crates/runner/afr_supervisor/src/recovery/mod.rs`](../../rustd/crates/runner/afr_supervisor/src/recovery/mod.rs) | Session and recovery module wiring | Give task sessions shared operation resources with a bounded lifetime. |
| **New** `rustd/crates/runner/afr_supervisor/src/recovery/admission.rs` | RecoveryResources::file, try_file | Limit file and task recovery to two concurrent operations. Required work queues and can be cancelled. Optional work skips a saturated pool before freezing. |
| **New** `rustd/crates/runner/afr_supervisor/src/recovery/uploads.rs` | UploadBatch::seal, put, verified | Share descriptor validation, byte bounds and the four-transfer window across task and ordinary uploads. |
| **New** `rustd/crates/runner/afr_supervisor/src/recovery/window.rs` | new batch and file-window helpers | Bound bytes, unique objects, entries and open writers. Share zeroizing chunk owners only within a window. |
| [`rustd/crates/runner/afr_supervisor/src/recovery/transfer.rs`](../../rustd/crates/runner/afr_supervisor/src/recovery/transfer.rs) | Transfers | Reuse one HTTP client and account for buffers until their final consumer releases them. |
| [`rustd/crates/runner/afr_supervisor/src/recovery/capture.rs`](../../rustd/crates/runner/afr_supervisor/src/recovery/capture.rs) | Capture::run, Session::capture_chunks | Use the same bounded upload grouping for task capture. |
| [`rustd/crates/runner/afr_supervisor/src/recovery/session.rs`](../../rustd/crates/runner/afr_supervisor/src/recovery/session.rs) | Session::upload_batch, download_batch | Share physical batching while retaining task-specific custody and checks. |
| [`rustd/crates/runner/afr_supervisor/src/recovery/restore.rs`](../../rustd/crates/runner/afr_supervisor/src/recovery/restore.rs) | Restore::run, Session::restore_files | Apply bounded windows across files after resolving the task base and overlay. |
| [`rustd/crates/runner/afr_supervisor/src/recovery/turn/mod.rs`](../../rustd/crates/runner/afr_supervisor/src/recovery/turn/mod.rs) | NativeTurn | Carry shared resources through ordinary-turn operations. |
| [`rustd/crates/runner/afr_supervisor/src/recovery/turn/capture.rs`](../../rustd/crates/runner/afr_supervisor/src/recovery/turn/capture.rs) | TurnSession::capture_files | Batch missing objects and verify the complete upload before publication. |
| [`rustd/crates/runner/afr_supervisor/src/recovery/turn/session.rs`](../../rustd/crates/runner/afr_supervisor/src/recovery/turn/session.rs) | TurnSession::new, upload_batch, authorize | Authorize and reconcile a physical batch with one command each. |
| [`rustd/crates/runner/afr_supervisor/src/recovery/turn/files.rs`](../../rustd/crates/runner/afr_supervisor/src/recovery/turn/files.rs) | TurnSession::restore_files, download | Use cross-file windows and shared chunk bodies while preserving safe application order. |
| [`rustd/crates/runner/afr_supervisor/src/recovery/turn/restore.rs`](../../rustd/crates/runner/afr_supervisor/src/recovery/turn/restore.rs) | NativeTurn::restore | Carry the file reservation into the writer and retain fenced adoption after validation. |

### Authority, deadlines and cleanup

| Repository path | Symbol or statement | Change |
|---|---|---|
| [`rustd/crates/daemon/afd_runner/src/capacity/mod.rs`](../../rustd/crates/daemon/afd_runner/src/capacity/mod.rs) | shared host authority helper | Prove token and connection under a shared host lock. Fleet and authoring issue retain their exclusive locks. |
| [`rustd/crates/daemon/afd_runner/src/capacity/sql/mod.rs`](../../rustd/crates/daemon/afd_runner/src/capacity/sql/mod.rs) | capacity SQL bindings | Register the shared-lock statement. |
| **New** `rustd/crates/daemon/afd_runner/src/capacity/sql/share_host.sql` | new host authority query | Read the host with FOR SHARE so rotation and issue conflict with checkpoint authority. |
| [`rustd/crates/daemon/afd_fleet/src/lease/turn_checkpoint/guard.rs`](../../rustd/crates/daemon/afd_fleet/src/lease/turn_checkpoint/guard.rs) | TurnLeaseGuard::lock | Use shared host authority, preserve parent/slot/lease order, and recheck time after waits. |
| [`rustd/crates/daemon/afd_fleet/src/lease/commit/mod.rs`](../../rustd/crates/daemon/afd_fleet/src/lease/commit/mod.rs) | Leases::run_report_statement | Bind the current connection proof for a report that can create a hold. |
| [`rustd/crates/daemon/afd_fleet/src/lease/sql/report_statement.rs`](../../rustd/crates/daemon/afd_fleet/src/lease/sql/report_statement.rs) | ReportStatement and its SQL binds | Supply the host proof to the report statement and preserve the existing result columns. |
| [`rustd/crates/daemon/afd_fleet/src/lease/sql/commit_report.sql`](../../rustd/crates/daemon/afd_fleet/src/lease/sql/commit_report.sql) | scope, locked_slot, released | Lock host after parents and before slot for a hold-producing report. Retain it through settlement. |
| [`rustd/crates/daemon/afd_fleet/src/lease/turn_checkpoint/transfer.rs`](../../rustd/crates/daemon/afd_fleet/src/lease/turn_checkpoint/transfer.rs) | execute, confirm | Skip quota locking and writes for empty verification receipts. Recheck custody after remote I/O. |
| [`rustd/crates/protocol/af_wire/src/lease/turn_checkpoint/command.rs`](../../rustd/crates/protocol/af_wire/src/lease/turn_checkpoint/command.rs) | TurnCheckpointAction | Classify upload verification for a consistent client and server deadline. |
| [`rustd/crates/runner/afr_supervisor/src/client/turn_checkpoint.rs`](../../rustd/crates/runner/afr_supervisor/src/client/turn_checkpoint.rs) | impl Command for Turn | Select a bounded bulk answer timeout for upload verification. |
| [`rustd/crates/daemon/afd_gateway/src/lib.rs`](../../rustd/crates/daemon/afd_gateway/src/lib.rs) | Gateway::new, Inner | Own a replica-wide limit for concurrent bulk verifications. |
| [`rustd/crates/daemon/afd_gateway/src/session/dispatch/turn_checkpoint.rs`](../../rustd/crates/daemon/afd_gateway/src/session/dispatch/turn_checkpoint.rs) | SessionServices::turn_checkpoint | Admit or refuse bulk work promptly and apply operation_until with reply margin. |
| [`rustd/crates/daemon/afd_fleet/src/lease/turn_checkpoint/allocation.rs`](../../rustd/crates/daemon/afd_fleet/src/lease/turn_checkpoint/allocation.rs) | head, initialize_head | Read an optional committed descriptor. Initialize metadata on the Begin path. |
| [`rustd/crates/daemon/afd_fleet/src/lease/turn_checkpoint/plan.rs`](../../rustd/crates/daemon/afd_fleet/src/lease/turn_checkpoint/plan.rs) | Leases::begin_turn_checkpoint | Initialize the recovery head after validating the lease and allocation request. |
| [`rustd/crates/daemon/afd_fleet/src/lease/turn_checkpoint/signing.rs`](../../rustd/crates/daemon/afd_fleet/src/lease/turn_checkpoint/signing.rs) | issue_object | Bound physical authenticated readback by the current pin deadline. |
| [`rustd/crates/daemon/afd_fleet/src/task/types.rs`](../../rustd/crates/daemon/afd_fleet/src/task/types.rs) | CheckpointMaintenanceCursor | Keep turn-owner scan progress alongside task-owner progress. |
| [`rustd/crates/daemon/afd_fleet/src/task/checkpoint/maintenance/mod.rs`](../../rustd/crates/daemon/afd_fleet/src/task/checkpoint/maintenance/mod.rs) | maintain | Carry turn retention counts and deferrals through independent maintenance stages. |
| [`rustd/crates/daemon/afd_fleet/src/lease/turn_checkpoint/maintenance.rs`](../../rustd/crates/daemon/afd_fleet/src/lease/turn_checkpoint/maintenance.rs) | maintain_turn_checkpoints, retain | Bound the scan, advance before processing a contended owner, and retry on wraparound. |
| [`rustd/crates/daemon/afd_fleet/src/lease/turn_checkpoint/sql/maintenance_candidates.sql`](../../rustd/crates/daemon/afd_fleet/src/lease/turn_checkpoint/sql/maintenance_candidates.sql) | owner scan | Page by indexed cursor before inspecting owner-specific work. |
| [`rustd/crates/daemon/afd_fleet/src/lease/turn_checkpoint/sql/maintenance_lock_head.sql`](../../rustd/crates/daemon/afd_fleet/src/lease/turn_checkpoint/sql/maintenance_lock_head.sql) | owner head lock | Defer a contended owner promptly so the current pass can collect other owners. |
| [`schema/20261117154626_runner_turn_recovery.sql`](../../schema/20261117154626_runner_turn_recovery.sql) | runner.turn_recovery indexes | Index the referenced head checkpoint and the maintenance cursor key. |
| [`schema/20261117154627_runner_turn_checkpoints.sql`](../../schema/20261117154627_runner_turn_checkpoints.sql) | runner.turn_checkpoints index | Index the fleet FK used by deletion and retention. |
| [`rustd/crates/daemon/afd_api_operator/src/handler/operator/events.rs`](../../rustd/crates/daemon/afd_api_operator/src/handler/operator/events.rs) | list query documentation | Include runner_uncordoned in the operator event filter contract. |

### Native connection settings

Each daemon shares its message-buffer budgets across its connected runners.
The TOML values below use exact byte counts.
Each budget gives half its bytes to incoming messages and half to outgoing messages.

```toml
[runner.listener]
address = "[::]:4443"
tls_cert_file = "/etc/agentsfleet/runner-stream/server-chain.pem"
tls_key_file = "/etc/agentsfleet/runner-stream/server.key"
max_udp_payload_bytes = 1232

[runner.stream.control]
max_buffered_bytes = 16_777_216

[runner.stream.data]
max_buffered_bytes = 50_331_648

[nats]
wake_queue_capacity_fleets = 64
```

When incoming work fills its buffer budget, later messages wait for room.
Heartbeats and commands have their own budget.

The NATS queue retains recent fleet wake-up hints used by the scheduler. [Message budgets](../../rustd/crates/protocol/af_stream/src/framing/budget.rs)

The UDP ceiling controls the daemon's maximum packet payload on a native QUIC connection.
Its default fits a 1,280-byte IPv6 path after IP and UDP headers.

The peer's advertised receive limit also applies. [Datagram limit](../../rustd/crates/protocol/af_stream/src/mtu.rs)

Tokio chooses daemon worker threads from the CPU capacity visible to the process.
Fly's configured development machines have two virtual CPUs, and production machines have four.

Runner work slots come from each runner's saved assignment. [Runtime](../../rustd/crates/daemon/main/src/main.rs)
