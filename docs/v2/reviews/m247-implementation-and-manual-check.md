---
type: explanation
audience: contributor
verified: 2026-11-18
product_version: 0.78.0
executable: false
---

# M247 implementation design and manual recovery check

This guide explains the [M247 spec](../active/M247_001_P1_API_CLI_DOCS_OBS_UI_FAILED_HOSTS_RESPAWN_FROM_CHECKPOINT.md).
It maps the implementation to source, describes transaction boundaries, and gives Indy a manual recovery check.
The spec remains the authority; this guide introduces no additional implementation milestone.

Indy's Nov 17 overnight instruction is: "Indy is going to bed, ensure the PR is sent, the CI gate is green, greptile are resolved. So indy can eyeball in the morning and merge."

Required automated integration may now run before the human exercise; earlier manual-first records below describe the original order.

The human exercise remains unperformed. Indy narrowed delivery on Nov 18: "I think complete M247 for platform runners, park tenant runners", followed by "go". Platform-runner recovery is in scope; M246 tenant placement, credential boundaries and tenant-runner acceptance are parked until that stream resumes.

Implementation base: fetched `origin/main` at `97224ed7432c9dd124ea8797d4324156b7022c07`.

Commands below were checked against their parser definitions. The [file and symbol inventory](m247-files-and-symbols.md) lists the current source changes. Dated planning notes below preserve earlier design decisions.

Runtime changes and their validation are recorded below. The pre-refactor focused recovery/capacity/process proofs, full unit suite and full lint passed. The [burst and refactor review](m247-scalability-and-refactor.md) records the later 100/500-fleet performance and >99% per-file coverage requirements; final refactor gates and the human check remain separate.

Indy confirmed Astra 6 Max selection before this review. Session tools cannot independently verify the model selector.
The review is complete; Indy subsequently confirmed the switch back to GPT-6.1 Sol Max and requested continued implementation.

## What the user and operator see

A user sends one request and receives one completed answer in the same conversation after recovery.
The unfinished answer pauses, then the replacement attempt clears provisional text and shows `Run restarted`.
A short connection interruption that recovers the same live lease shows no restart marker.

Operators read runner policy, liveness and automatic quarantine separately.
A host can remain operator-active while an automatic cordon blocks new work.
No new operator browser page is proposed.

```text
Person           Chat / CLI        Daemon + Postgres       Runner A       Runner B
  | send message     |                     |                  |              |
  |----------------->| admission E stored  |                  |              |
  |                  |-------------------->| issue L1/F7       |              |
  |                  |                     |----------------->|              |
  |                  |<--------------------|<---- partial reply              |
  |                  |                     |<---- committed checkpoint C     |
  |                  |                     |                  X dies          |
  |                  | reply pauses        |                                 |
  |                  |                     | L1 deadline passes              |
  |                  |                     | claim E, fence F8                |
  |                  |                     | issue L2 COMMIT                  |
  |                  |<-- restart(E,F8) ---|                                 |
  |                  | clear draft once    |----- offer L2/C ---------------->|
  |                  |                     |<---- restore C, continue --------|
  |                  |<--------------------|<---- F8 output ------------------|
  |                  |                     |<---- final report ---------------|
  |<-- final answer -|<--------------------| final settlement COMMIT          |
```

The lease deadline normally falls 35 seconds after its last renewal; it is not measured from the crash.
Placement, available workers, admission gates and restore time also affect recovery latency.
Automatic quarantine follows 105 seconds without a heartbeat, independently of reclaim.

## Verified status commands and access

The current command family is **plural**: `agentsfleet platform runners`.
There is no `platform runner`, `runners status` or `runners history` command.
Source: `cli/src/program/tree/platform-runners.command.ts` and `platform-flags.ts`.

| Need | Existing command | Access / result |
|---|---|---|
| List physical hosts | `agentsfleet platform runners list` | Operator `runner:read`; policy state and derived connection state |
| Inspect one host | `agentsfleet platform runners show <RUNNER_ID>` | Operator `runner:read`; policy, health, checks and counters |
| Read lifecycle events | `agentsfleet platform runners events <RUNNER_ID>` | Operator `runner:read`; actual event subcommand |
| Read its lease attempts | `agentsfleet platform runners leases <RUNNER_ID> --fleet <FLEET_ID>` | Operator `runner:read`; use JSON for event ID, fence and expiry |
| Read exact JSON | `agentsfleet --json platform runners show <RUNNER_ID>` | Same authority; does not bypass authorization |
| Check a normal user's fleets | `agentsfleet fleet status` | Existing workspace access; fleet state, not physical-host administration |
| Inspect that user's work | `agentsfleet fleet show <FLEET_ID>` / `agentsfleet fleet events <FLEET_ID>` | Existing workspace/fleet access |
| Inspect a tenant's own runners | M246 proposes `agentsfleet runner list` and `agentsfleet runner placement` | Pending dependency; these are not commands available in this checkout |

`<RUNNER_ID>` comes from operator runner list; `<FLEET_ID>` comes from the active workspace's fleet list.
The current CLI hides platform leaves whose scopes the authenticated account lacks.
A regular end user cannot gain runner visibility by spelling an operator command.

M247 adds optional `cordon` to runner list/detail JSON and matching text fields.
The operator list gains a cordon column; show gains the reason, cordoned time and healthy-return rule.
The M246 owner-facing reader must carry the same field once that dependency exists; tenant lease/event history remains outside M246's scope.

```text
Proposed runner show fields:
State:       active
Connection:  offline
Auto cordon: missed_heartbeats, since <CORDON_TIME>
Return rule: 30 seconds of continuous healthy heartbeats
```

## Owners and call sequence

```text
afr_supervisor::Worker::next
  +-- fleet offer -----> Lessee::run_lease -> LeaseRun::drive -> afr_agent::Loop
  +-- authoring offer -> Lessee::author
  +-- both -----------> Holds occupation guard -> Keeper

native runner session
  +-- offer ------> LeaseService -> ledger_claim -> issue_for_revision
  +-- heartbeat --> Runners::heartbeat_connected -> fenced liveness transition
  +-- output -----> LeaseCache::target -> Published::of -> core NATS -> clients
  +-- report -----> current report owner -> fenced settlement + usage accounting

Postgres owns admissions, slots, leases, cordons and committed recovery pointers.
Core NATS carries disposable wakes and live output; it grants no recovery authority.
```

The claim reserves a slot before issue. Those operations remain separate transactions.

A crash after claim creates an expiring reservation; a crash after issue creates an expiring lease.
Neither a selected candidate nor a published notification proves that a runner received an offer.

### Methods and types to change

Paths in these tables start at the repository root. The spec carries the complete file inventory, tests and generated-document scope.

| Component / file | Existing method or type | Proposed change and reason |
|---|---|---|
| `afd_runner/src/capacity/mod.rs` under `rustd/crates/daemon/` | `lock(tx, guard)`, `lock_host(tx, guard)`, `Capacity`, `Held`, `Host` | Add claimant purpose to the full census; both claim predicates read automatic cordon under the host lock. |
| Same module | `Guard::{Enrolled,Connected}`, `proof` | Preserve exact digest/connection authority. A claimant says what is counted; it never grants authority. |
| `afd_fleet/src/lease/store.rs`, `afd_authoring/src/draft/mod.rs` | `lock_host_capacity` wrappers | Pass `Claimant::Authoring` for new authoring work and `Existing` for renewal; fleet issue keeps its inlined census. |
| `afd_fleet/src/lease/issue.rs` | `issue_for_revision`, `permits_issue`, `Issued` | Reject cordoned hosts under lock; consume a fleet's hold only within successful issue; attach frozen resume metadata. |
| `afr_supervisor/src/worker_pool/mod.rs` under `rustd/crates/runner/` | `Worker::next` | Both dispatched entry points use the same attempt-keyed occupation guard, including cancellation and error paths. |
| `afr_supervisor/src/holds/mod.rs` | `Holds::{occupy,take,park}`, `Occupied`, private `Request` | Proposed `Occupant::{Fleet,Authoring}` and attempt-keyed handoff; consume guards when moving busy state into a hold. |
| `afr_supervisor/src/holds/keeper.rs` | `Keeper::{occupy,park,cap,retire}` | Count all busy workers; retire oldest holds to preserve a free worker; protect a hold transferring into the current lease. |
| `afd_runner/src/sweep/liveness/mod.rs` | `Liveness::{due,visit,record_offline,finish_drain}` | Recheck stale observations under lock; record automatic cordon atomically; select actionable stale hosts and drains. |
| `afd_runner/src/heartbeat/mod.rs` | `heartbeat_connected`, `record_beat`, `bump_liveness` | Carry connection proof into the actual mutation; decode committed cordon/lift outcome without a second per-beat query. |
| Proposed `afd_runner/src/heartbeat/cordon.rs` | New transition helper | Mint required audit IDs, bind healthy-return rules and decode `CordonTransition`; never infer health from event history. |
| `afd_fleet/src/lease/assign/ledger.rs`, `envelope.rs` | `Admission::acquired`, `Acquired`, `Kind` | Preserve predecessor ID for diagnostics while recovery uses stable event/task identity; reuse `Fresh` and `Reclaim`. |
| `afd_fleet/src/lease/deliver/issue.rs` | `issue_ready`, `render_offer`, `metadata` | Announce an issued reclaim after commit and before offer encoding; include resume data in the offer. |
| `afd_fleet/src/lease/activity/published.rs` | `Published::of`, `Published` variants | Stamp every output variant with `Target::fence` as a decimal string. Runner-supplied text cannot choose this value. |
| `afd_gateway/src/session/dispatch/progress.rs` | `SessionServices::progress` | Keep cached expiry/cancel checks. No new SQL read per chunk; consumer fences cover delayed publication. |
| Browser streaming modules | `Entry`, `createEntry`, `applyFrame`, `writeChunk`, decoder disposal | Keep greatest observed fence per event; guard asynchronous callbacks by attempt and decoder identity. |
| `cli/src/commands/fleet/steer-events.ts` | `makeFrameCallback`, `SteerPresentation`, `openEventTail` | Filter by event first, then attempt; reset printer state and print one separator for a newer attempt. |
| `afd_api_wire/src/operator.rs`, `admin/mod.rs` | `RunnerItem`, `RunnerDetail`, `RunnerEventType` | Add canonical `CordonReason`, `RunnerCordon` and `RunnerUncordoned`; detail inherits item's cordon. |
| `afd_runner/src/view/{mod.rs,decode.rs}` | `RunnerItem`, `ItemRow::parse` | Decode optional cordon as typed domain state; reject malformed reason/time pairs rather than guessing. |
| `afd_api_operator/src/handler/operator/runners.rs` | `item`, `detail_payload` | Map the domain field into the real list/detail routes. |
| `cli/src/commands/platform/{runner-schema.ts,runner-vocabulary.ts,runners.ts}` | Effect schemas; `platformRunnersList`, `platformRunnerShow`, `platformRunnerHistory` | Decode/render cordon; accept `runner_uncordoned` in event filters; keep existing command grammar. |

All shortened crate paths above use the layer named in their first occurrence.
`Claimant`, `Occupant`, `CordonTransition` and the recovery types below are proposed types, rather than existing symbols.

### Traits and recovery interfaces

| Interface | Design |
|---|---|
| Existing `afr_agent::Checkpoint::push(memory)` | Keep semantic-memory behavior and signature. It cannot save execution history. |
| Existing `Inbox::claim(after_sequence)` | Keep its signature; add `Folded::event_id` and preserve the wire ID in `FencedVerbs`; snapshot exact incorporated IDs and sequences together. |
| Existing `EventSink::emit(frame)` | Keep its signature. The daemon attaches output attempt identity after receiving activity. |
| Existing `AgentEngine::run(AgentRun)` | Keep the method signature; add a recovery port and restored plain-turn state to `AgentRun` and `Shared`. |
| Proposed `RecoveryBoundarySink::save(snapshot)` | Async, object-safe port returning `Result<RecoverySave>`: `Committed(RecoveryReceipt)` or `Skipped(CaptureSkip)`. Only a validated receipt advances local recovery state; no successful default implementation. |
| Proposed `RecoverySnapshot::{PlainTurn,Task}` | Own scrubbed history, driver state, terminal children, local memory overlay, immutable scope witnesses and incorporated cursors; exclude credentials and transfer URLs. |
| Proposed `RecoveryReceipt` | Name committed generation, snapshot identity, digest and covered inputs; validate before advancing local recovery state. |
| Existing `guffbolt::Extension` | Thread `DriveState` through `drive` and `before_model` in place. Keep `after_group` semantics; capture continuing groups at the following pre-model boundary. |
| Existing `Sandbox::{freeze,thaw}` | Reuse freeze/thaw; add fail-closed `Sandbox::recovery_quiescent` to prove the frozen tenant process tree is empty. Session counts alone are insufficient. |
| Existing `TaskRecovery::{capture,restore}` | Reuse task history identity/validation; introduce separate `TurnRecovery` for event-owned plain turns. |
| Existing `MemoryBackend` | Add required `snapshot() -> Result<MemoryRecovery>`; `Hydrated::recover(seed, state)` restores pending writes, forgotten keys and recall budget under current grants. |
| Existing `Sweep`, `af_context::CheckpointStore`, `RunnerApi`, `Services` | Keep method signatures. The context store is unrelated to the concrete `R2CheckpointStore` used for file custody. |

An action group is safe only after its model response and tool results are remembered together.
`after_group` completes task assessment; continuing groups and answer-only follow-ups reach capture at the next `before_model`.
Do not checkpoint a fold cursor ahead of the conversation bytes that include its message.

`Children::beside` currently polls nested work while the root awaits other operations.
A root hook alone therefore cannot freeze the workspace safely.
Live capture gates new child work and requires no open or reserved executor sessions.

It skips capture while work remains; it never kills work to create a boundary.

The supervisor must own this handoff while its renewal loop stays live.
Capture cannot borrow a workspace before sandbox construction finishes.

Cleanup outlives cancellation and joins every file worker before thaw or teardown.
The reviewed design below pins this ordering; implementation must prove it before grading recovery.

## SQL and transaction changes

### Admission capacity and held sandboxes

```text
capacity::lock_host              separate statement snapshot after host lock
  | FOR UPDATE runner row        |
  +------------------------------v
    fleet issue census OR authoring census
      active unexpired fleet leases
    + active unexpired authoring attempts
    + live held slots, excluding own fleet only when consumed
      |
      +-- full / cordoned -> no issue, transaction rolls back
      +-- fits ----------> guarded lease + hold consumption -> COMMIT
```

| SQL file | Change |
|---|---|
| `afd_runner/src/capacity/sql/lock_host.sql` | Read cordon fields under the existing host lock and preserve credential/connection checks. |
| `afd_runner/src/capacity/sql/census.sql` | Add holds using the existing held-slot index; use fresh database time after the lock and a bound claimant purpose. |
| `afd_fleet/src/lease/sql/lease/insert_lease_with_event.sql` | Apply the same hold rule in the inlined census; clear the fleet's consumed hold in the guarded issue transaction. |
| `afd_fleet/src/lease/sql/commit_report.sql` | Preserve late completion/billing, but reject publishing a future hold from an expired lease. A live lease-to-hold transition must count once. |
| `afd_fleet/src/lease/sql/lease/release_fleet_slot.sql` | Apply the same hold eligibility rule to the older release writer, using authoritative lease identity/deadline rather than trusting a requested timestamp. |
| `afd_fleet/src/lease/sql/renew_batch.sql` | Preserve hold-free existing-work renewal and exact connection/fence checks; automatic cordon blocks no valid renewal. |

Do not add a host lock after a report already holds a slot lock.
Declining an expired lease's new hold preserves current report lock order and late usage settlement.
The report acknowledgement must tell the supervisor whether its parked hold was accepted; the current heartbeat reconciliation cannot provide this proof.

Add `ReportResponse::hold: Option<AcceptedHold>` with the reporting lease, fence and accepted deadline.
Carry it through `ReportAnswer`, committed settlement, gateway `ReportAck`, `Plane::report` and `LeaseRun::settle` instead of discarding the response.

On `hold=None`, call `Holds::supersede` for that exact parked lease, even when report settlement succeeded.
An acknowledgement only confirms an existing lease-keyed hold; it never recreates one after transfer or cancellation.
Replay preserves completion and usage idempotency; return a hold only while that report's authoritative slot still has its fence, owner and live hold.

The hold response can conservatively become absent after consumption or expiry; replay never republishes the hold.
Do not release holds merely because a heartbeat observes a null database hold: local park precedes report commit.

Authoring occupies shared workers even when it creates no sandbox.
Share attempt-keyed occupation accounting across fleet and authoring entry points; otherwise authoring can consume the reserved worker while the keeper retains a hold.
Atomic held-to-busy and busy-to-held transfers prevent double charging the same sandbox.

### Offline episodes, return and drain completion

| SQL/schema file | Change |
|---|---|
| `schema/20261108123100_runner_runners.sql` | Add nullable `cordon_reason`, `cordoned_at`, `cordon_healthy_since_at`; require reason/time to agree and the healthy marker to belong to an automatic cordon. |
| `afd_runner/src/sql/sweep/select_due_runners.sql` | Select actionable offline episodes and actionable drains, or use a fair bounded cursor. Old stale/draining rows cannot block later hosts. |
| `afd_runner/src/sql/sweep/cordon_silent_runner.sql` | New statement under the host lock: recheck stale `last_seen_at`, set the automatic fields and write the reason audit. |
| `afd_runner/src/sql/sweep/insert_offline_event.sql` | Delete the standalone writer; `cordon_silent_runner.sql` now keeps episode uniqueness and automatic audit in one atomic statement. |
| `afd_runner/src/sql/sweep/mark_drained_if_idle.sql` | Consider live fleet leases and authoring attempts, using their actual deadlines; exclude expired-but-unreported rows from active work. |
| `afd_runner/src/sql/sweep/expire_active_lease_slots.sql` | Delete with both callers and its constant. The liveness sweep never rewrites lease deadlines. |
| `afd_runner/src/sql/runner/heartbeat_with_transition_event.sql` | Lock and prove current credential/connection; update liveness, health proof and any automatic lift in one guarded row mutation. |
| `afd_runner/src/sql/runner/touch_runner_last_seen.sql` | Fallback still proves authority under lock, clears health proof and never lifts a cordon. |
| `afd_runner/src/sql/runner_view/{list_runners_first.sql,list_runners_after.sql,runner_detail.sql}` | Select optional automatic-cordon data for both real API readers. |

```text
sweep due-read -> host lock -> stale observation still valid?
                                 +-- no -> nothing
                                 +-- yes -> offline + auto cordon + audit COMMIT

valid current beat -> healthy_since starts
next valid beats -- continuous 30s --> clear auto cordon + runner_uncordoned
gap >=20s / degraded / invalid ------> reset healthy_since
superseded credential/connection ----> no liveness write and no health advance
operator cordon/drain/revoke --------> never cleared by automatic recovery
```

Use database time read after locking, rather than transaction-start time.
Audit events describe committed transitions; prunable history never supplies the health timer.
Draining-but-busy hosts need the same starvation proof as already processed offline hosts.

### Reclaim and settlement

| SQL file | Change or preserved guard |
|---|---|
| `afd_fleet/src/lease/sql/lease/select_ledger_candidates.sql` | Cordon filter saves work; preserve tags and finalized M246 placement filters. |
| `afd_fleet/src/lease/sql/lease/claim_admission_head.sql` | Refuse new claims for a cordoned claimant; preserve slot expiry, admission/task limits and the guarded fence increment; return predecessor identity. |
| `afd_fleet/src/lease/sql/lease/insert_lease_with_event.sql` | Recheck capacity and current claim at issue under the host lock, even when a candidate was selected before cordon. |
| `afd_fleet/src/lease/sql/commit_report.sql` | Current admission/slot fence decides completion. Preserve idempotent accounting of known old-lease requests when a stale report loses execution authority. |
| `afd_fleet/src/lease/sql/lease/settle_folds.sql` | Require current lease ID/fence for each covered input; checkpoint restoration must adopt those claims before skipping delivery. |
| `afd_fleet/src/lease/sql/bill_expired_slice.sql` | Keep each lease's meter cursor and usage-line uniqueness; do not charge beyond the old deadline. |

The replacement passes current provider, funds, setup and approval gates again.
It can have different billing facts from its predecessor.
The expiry biller currently waits 120 seconds after the old deadline before its final-slice attempt.

### Task checkpoint reuse and new plain-turn state

Task recovery already has a complete server publication path.
`task/checkpoint/commit.rs::publish` validates the live lease and commits the ready manifest and task pointer before returning.
`task/checkpoint/sql/task_checkpoint.sql` advances that pointer; M247 adds no parallel task-head column.

```text
complete group -> incorporate inputs / reconcile native effects
  -> pre-model boundary -> exclusive permit -> snapshot root state
  -> freeze -> prove tenant tree empty -> scan/upload/verify
  -> matching files + root state + head COMMIT
  -> join file workers -> thaw -> receipt -> next model call
```

Plain chat uses six event-owned tables described in the reviewed design below.
They separate the current head, immutable plans, pages, object references, pins and replay receipts.
The earlier single-row proposal is replaced by this design.

| Owner | Operation and authority |
|---|---|
| `TurnCheckpoints` | `begin`, register/verify objects, write/read pages, renew/release pins, `commit`, `read`, `adopt_folds`, `abandon`, `maintain`; API-only checkpoints need no file-storage access. |
| `TurnLeaseGuard` | Prove current connection, workspace/fleet/event, active plain lease, both deadlines and exact slot fence inside every transaction; do not call the task-capability guard for ordinary chat. |
| `turn_checkpoint/sql/commit.sql` | Compare expected head, validate full manifest/root digest and exact fold membership, then mark ready and swap head atomically. |
| `turn_checkpoint/sql/load.sql` | Pin/read the offer's exact committed generation under the replacement's live authority; never silently choose a different generation. |
| `turn_checkpoint/sql/adopt_folds.sql` | Restamp only validated covered admissions with the replacement lease/fence; preserve conversation, approval and task exclusion rules. |
| `Up::TurnCheckpoint` / `Down::TurnCheckpoint` | One command/response family on the existing reports lane; correlate request, lease, operation and command kind. |
| `TurnRecovery` | Restore full messages, accepted-answer segments, terminal children, pinned input and local memory overlay; rebuild derived context under the new lease. |
| `Build::acquire` / `LeaseRun::sandboxed` | Release unrelated warm holds, skip fresh checkout for a saved manifest, restore into an empty workspace and await validation before model/tools. |

Offers carry `TurnResume` descriptors, never history bytes.
Both complete command and response messages have a one-MiB bound under the existing two-MiB native message limit.
The writer checks the prospective read-response size before committing; oversize state keeps the previous generation and logs a typed skip.

Seal root state with the daemon's existing `Sealer` and `Envelope`.
`Aad::turn_recovery` binds workspace, fleet, event, generation and format; the master key never leaves the daemon.
Authorized file transfers retain the existing short-lived object-key handling.

Present invalid state refuses restore; only absence permits original-input replay.
Task inner history gains the same root components and a new format version; existing file manifests and task pointers remain authoritative.
No recovery receipt authorizes another attempt's billing or task-completion report.

### Output and client ordering

```text
first ordinary frame E/F7 -> initialize silently
restart E/F8             -> clear draft/gaps; close old decoder; mark restart
new E/F8 seq0            -> start replacement decoder
late E/F7 frame/callback -> ignore
duplicate E/F8 restart   -> keep new draft
lost restart + E/F9      -> greater attempt resets once
same F9 missing sequence -> durable final-answer repair
terminal saved event    -> reject all late cosmetic mutations
```

A first observation above fence one does not prove this event restarted; the fleet fence spans more than one event.
An explicit first-seen restart may show the marker.
Parse fences as bounded decimal strings so JavaScript keeps the database integer exactly.

Completed tools can remain visible as provisional previous-attempt entries during retry.
The winning report's saved trace becomes authoritative; current SQL deletes other-attempt full tool records.
This design promises no durable archive of every attempt's tool bodies.

## Manual smoke check before automated recovery lanes

The read commands work now for an authorized operator.
The recovery result below is a **post-implementation target**; the current branch cannot pass missing M247 checkpoint/cordon behavior.
No integration, end-to-end or acceptance lane is started by this review.

### Before you begin

Use a disposable development fleet and two separately enrolled runners against the same checkout's development daemon.
Use the existing [development setup](../../development.md#shared-development-worktrees); prepare the disposable runner lab separately from remote host deployment.
The runners need compatible policies, matching labels, funded model access and separate private storage homes/tokens.

The daemon needs its configured encryption key and session-code pepper.
File-backed task and ordinary-turn drills also require the daemon's configured private checkpoint storage.
Use existing credential setup; put no machine runner token into the human CLI session.

Use snapshot-capable sandboxes for a task-file recovery check; `--unsandboxed` cannot prove that property.
Have an operator account with `runner:read` and control of the exact runner containers used below.
Prepare those runners without invoking `make acceptance`; setup and a manual smoke are separate from automated lanes.

The native-task variant requires durable coordination and an accepted task; an ordinary chat prompt does not establish those conditions.
The ordinary file-backed variant uses its event-owned checkpoint path instead.

The container lab and controllable second-action fixture remain setup work before the human recovery check.
The fixture must hold after an actual checkpoint commit until the user induces failure; a model promise or unspecified long action is insufficient.
Until that fixture is prepared, the commands below specify the target drill rather than a deterministic runnable recovery test.

Record `<RUNNER_A_ID>`, `<RUNNER_B_ID>` from runner list and `<FLEET_ID>` from fleet list.
`<RUNNER_A_CONTAINER>` names only A's disposable container; confirm its identity before pausing it.
If the runner is a host service instead, use an approved whole-service isolation method rather than guessing a PID.

For the current environment baseline, the existing local daemon commands are:

```bash
FOLLOW_LOGS=0 make up
make status
bash scripts/local-dev.sh url
```

```text
<varies>: existing configured development infrastructure and daemon start successfully.
The final command prints this worktree's API URL; it creates no runner containers.
```

These commands require the trusted development environment described in the linked setup guide.
They establish readiness only; they do not prove any proposed M247 behavior.

### 1. Confirm the two targets

```bash
agentsfleet platform runners list
agentsfleet --json platform runners show <RUNNER_A_ID>
agentsfleet --json platform runners show <RUNNER_B_ID>
```

```text
<varies>: both intended runners exist, are operator-active, healthy and connected.
Target M247: neither has an automatic cordon before the drill.
```

Use the authenticated CLI's configured development API and workspace.
For a different target, the existing root `--api <API_URL>` flag chooses the API; it changes no permission.
Do not point the drill at shared production runners.

### 2. Send one disposable task

```bash
agentsfleet fleet steer <FLEET_ID> "Run the prepared recovery smoke task in the disposable workspace, using its marker and controlled second-action barrier."
```

```text
<varies>: streamed work begins for one event E on runner A.
```

Keep B healthy and eligible, and confirm A owns E through its lease JSON.
If B received the task, use that runner as the failure target and swap the labels in this procedure.
A prompt alone does not prove the task created a durable recovery point.

### 3. Wait for an actual checkpoint receipt

M247 must expose a debug log `respawn_checkpoint_committed` only after validating the server's committed receipt.
Its allowed fields are event/task identity, lease, fence, checkpoint identity and generation; no history, tokens or transfer URLs.
Capture that line and the matching lease JSON before inducing failure.

Enable that module's debug logging on the dedicated runner before startup; this log is a proposed implementation surface.

```bash
agentsfleet --json platform runners leases <RUNNER_A_ID> --fleet <FLEET_ID>
docker logs --tail 100 <RUNNER_A_CONTAINER>
```

```text
<varies>: E, L1, F7 and the stored lease deadline are visible in lease JSON.
Target debug log: respawn_checkpoint_committed, checkpoint C, committed generation G.
```

Pause only while the prepared second-action barrier is holding after the saved checkpoint.
If the task finishes first or no committed receipt exists, restart with fresh disposable work; do not mark the test passed.

### 4. Freeze A as the failure surrogate

Pausing the whole owned container freezes its supervisor and sandbox descendants together.
It models lost heartbeats/renewals; the later automated hard-kill drill separately proves abrupt process death.
The pause affects every process in that container, so use only the disposable target recorded above.

```bash
docker pause <RUNNER_A_CONTAINER>
```

```text
<RUNNER_A_CONTAINER>
```

Keep the original chat/steer open and read B's attempts without resending the request.

```bash
agentsfleet --json platform runners leases <RUNNER_B_ID> --fleet <FLEET_ID>
```

```text
Target M247: same event E, new lease L2, fence F8 greater than F7, kind reclaim.
No replacement issue precedes L1's recorded deadline.
Chat: one Run restarted marker, then replacement output and one final answer.
```

Require proposed `respawn_checkpoint_restored` evidence for C/G after file and history validation, before B's next model/tool action.
Correlate that verified restoration with the marker file; do not mark restore passed from the final answer alone.

A successful answer alone proves takeover, rather than restoration of saved files/history.
Recomputed external actions may incur new legitimate usage; this smoke does not prove exactly-once external effects or complete billing correctness.

### 5. Inspect quarantine, then restore the lab

Leave A paused until its 105-second silence threshold and the paced sweep have passed.
Read both current status and lifecycle history.

```bash
agentsfleet --json platform runners show <RUNNER_A_ID>
agentsfleet platform runners events <RUNNER_A_ID>
docker unpause <RUNNER_A_CONTAINER>
```

```text
Target M247: operator status active, cordon.reason missed_heartbeats.
History includes runner_offline and runner_cordoned.
docker unpause prints <RUNNER_A_CONTAINER>.
```

After 30 seconds of continuous healthy heartbeats, show A again and read its events.
Expect automatic cordon to be absent and `runner_uncordoned` present, while B's completed answer stays unchanged.
Always unpause the recorded container during cleanup, including after a failed observation.

### Human pass evidence and automation order

| Check | Evidence Indy records |
|---|---|
| Actual recovery point | A's committed receipt C/G and matching event/lease/fence |
| Takeover after expiry | A and B lease JSON showing same E, different leases and greater fence |
| Restored progress | B's verified restore receipt and pre-model file/history evidence |
| Coherent conversation | Restart marker once, replacement answer once, follow-up input retained if sent |
| Host recovery | Cordon reason/history, then healthy-return lift with operator status preserved |

The manual result remains ungraded until Indy records these observations.
After build and focused unit checks, prepare this smoke for morning review; the overnight instruction permits required recovery integration to run first.
After the human result, run required integration checks and the dedicated hard-kill test; acceptance follows its normal separate authorization and environment rules.

## Readiness and review disposition

The reviewed plan covers capacity, quarantine, output fencing, operator readers, report acknowledgements and recovery.
Only the capacity draft has runtime edits so far.
The recovery design below specifies paired files, full root state and whole-run capture.

Implementation and failure proofs remain outstanding.
M246's owner-facing commands and response mapping belong to the parked tenant-runner stream and must be checked when that stream resumes.

This document is a design and manual procedure, not a record that the proposed recovery test passed.
The spec's Discovery records this review after Indy's confirmed model selection, its findings and remaining proofs.

### Additional interface file changes from the focused review

| File(s) | Methods / types and proof |
|---|---|
| `rustd/crates/runner/afr_agent/src/engine/mod.rs`, `rustd/crates/runner/afr_supervisor/src/verbs/mod.rs` | Add `Folded::event_id`; `FencedVerbs::claim` preserves and validates wire identity instead of dropping it. `Inbox::claim` keeps its signature. |
| `rustd/crates/runner/afr_agent/src/loop/fold_tests/{mod.rs,fixture.rs}`, `rustd/crates/runner/afr_supervisor/src/{verbs/tests/folds.rs,lease_loop/drive.rs}` | Update direct folded-input literals and verify snapshot membership matches IDs and sequences actually incorporated. |
| `rustd/crates/protocol/af_wire/src/report/mod.rs` | `AcceptedHold`, `ReportResponse::hold`; define lease/fence/deadline identity and replay downgrade to no retained hold. |
| `rustd/crates/daemon/afd_fleet/src/lease/{sql/report_statement.rs,commit/mod.rs,report/finalize.rs}`, `rustd/crates/daemon/afd_gateway/src/session/dispatch/report.rs` | Carry authoritative hold result from SQL commit through settlement to the native report acknowledgement. |
| `rustd/crates/runner/afr_supervisor/src/{client/stream.rs,lease_loop/settle.rs,lease_loop/mod.rs,test_util/mod.rs}` | Preserve typed report responses; release the exact declined hold; update the response fixture without changing unrelated verbs. |
| `rustd/crates/runner/afr_supervisor/src/report/delivery/mod.rs` | Reuse its already-generic `until_acknowledged`; no method signature change is needed to return the typed response. |
| `rustd/crates/protocol/af_wire/src/lease/turn_checkpoint/{mod.rs,command.rs,response.rs}` and native client/dispatch modules listed above | Add fenced read request/response and exact descriptor verification; test stale reads, wrong generation and complete-message size limits. |

## Build opening and Pull Request Session Notes

Indy authorized implementation using `orly-build` on Nov 17, 2026.
The isolated worktree is `/workspace/agentsfleet-m247`, branch `feat/m247-checkpoint-respawn`, based on `97224ed7432c9dd124ea8797d4324156b7022c07`.

**Implementer restatement:** Preserve one admission through runner failure.
Use expired leases and rising fences for takeover, automatic quarantine for host health, and committed checkpoints for recovery.
Start with independent capacity and keeper changes, then cordon and attempt-aware output; complete checkpoint design before Section 4 code.

**ASSUMPTIONS I'M MAKING:**

1. The start instruction authorizes the normal feature-branch commits, non-force push and Pull Request after the required gates.
2. Existing reviewed documentation changes belong to M247 and are carried into this isolated stream; the original checkout remains untouched.
3. M246 remains pending in the parked tenant-runner stream; no tenant ownership surface or proxy behavior is claimed to exist.
4. The reviewed design resolves disk/history consistency and capture ownership in the plan. Implementation and named failure proofs remain required.
5. Indy originally requested the controlled smoke first; his Nov 17 overnight instruction permits required automated integration before his manual review. Paid acceptance retains separate authorization.
6. Before a review that needs a different model, ask Indy to switch it manually and stop until confirmation.

Opening validation: `orly doctor` passed in the source checkout.
No implementation Dimension is marked done by the opening commit.

## Implementation evidence before this review

The implementation worktree is `/workspace/agentsfleet-m247`, branch `feat/m247-checkpoint-respawn`.
Opening commit `86f481126` moves the spec to active and records the start; `orly gate work` passed.

The first runtime draft changes `capacity::lock` to require `Claimant::{Fleet,Authoring,Existing}`.
The census counts live holds for new work; existing-work renewal ignores them.
Fleet issuance counts other fleets' holds and clears its own hold inside the issuance transaction.

The gateway registration caller now takes `lock_host`; registration proves ownership without inspecting free capacity.

Five new database tests cover held capacity, own-hold reuse, renewal, expired holds and 100 concurrent authoring claims.
These tests await the integration lane; no Dimension is DONE from test source alone.
The keeper reserve and accepted-hold report acknowledgement must accompany this draft before it can ship.

`gitleaks detect --redact --no-banner` reported 13 historical matches across 16,252 commits.
A subsequent redacted directory scan reported no leaks in the current checkout.
No scanner suppression was added, and no credential values were printed.

First `make lint-rustd` attempt failed during BoringSSL binding generation: Clang could not find `stddef.h`.
Its coverage-harness tests passed (16), and its formatting check passed before that compiler prerequisite failed.
The host had `libclang1-19`, but lacked its matching resource headers.

Matching Debian package `libclang-common-19-dev` (`1:19.1.7-3+b1`) was downloaded and extracted under `/workspace/.cloud-tools/clang-headers`.
The retry uses `BINDGEN_EXTRA_CLANG_ARGS=-resource-dir=/workspace/.cloud-tools/clang-headers/usr/lib/llvm-19/lib/clang/19`.
Keep that environment argument on later Rust lanes on this machine.

The retry passed: `make lint-rustd` completed with exit 0.
Its 16 coverage-harness tests, formatting check, workspace Clippy check and dependency scan passed.
The full output is `/tmp/m247-lint-rustd-retry.log`; the earlier failed attempt remains `/tmp/m247-lint-rustd.log`.

The runtime draft remains uncommitted and cannot ship separately from the keeper reserve and accepted-hold acknowledgement.
No implementation Dimension is DONE, and no product integration test has run.

Worktree preparation passed: root and CLI frozen installs, CLI build, and frozen installs in every `ui/packages/*` package.
The built CLI help confirms `platform runners list` requires `runner:read`.
`docker info` reports server version `28.4.0`; no manual-test containers have been started.

Integration, end-to-end and acceptance lanes remain unrun.

Indy receives a prepared manual test setup and exact commands, then reports the observed failure-and-resume result.
The Nov 17 overnight instruction supersedes waiting for that result before required automated integration.

## Full-spec review after confirmed Astra 6 Max selection

Indy confirmed the manual model selection with “yes selected” before this pass.
Tools expose no independent model-selector evidence; this records user-confirmed selection, without claiming tool-verified provenance.
The earlier disputed pass remains historical advice and supplied no implementation proof.

The review read all six Sections against the implementation worktree and fetched `origin/main` again.
`origin/main` remains `97224ed7432c9dd124ea8797d4324156b7022c07`; the worktree opening commit is `86f481126c1aa053c34ef3bdaaa6f07baa1c74ab`.
The existing capacity draft was inspected and left unchanged.

### Findings and dispositions

Eight material corrections to the provisional design were accepted in this pass.
The earlier child, driver and paired-file findings were also checked against source and retained.

| # | Finding and source | Disposition / proof |
|---|---|---|
| 1 | `afr_memory/src/hydrated.rs` holds pending stores, superseded keys and recall counts outside conversation messages. | Snapshot the local overlay and recall budget; restore through `Hydrated` under current grants. Dimension 4.13. |
| 2 | `recovery/{capture,restore,session}.rs` uses `spawn_blocking`; dropping its handle does not stop disk work. | Keep cleanup outside the agent future, cancel cooperatively and join all file work before thaw/destruction. Dimension 4.9. |
| 3 | `afr_executor/src/server/process/mod.rs` permits descendants outside the ended process group; `afr_sandbox/src/cgroup/mod.rs` separates sandbox and tenant leaves. | After confirmed freeze, require `tenant/cgroup.events` to report `populated 0`; a detached process denies capture. Dimension 4.8. |
| 4 | `lease_loop/build.rs::acquire` bypasses warm holds only for native tasks; ordinary execution can start beside build. | Apply the same fresh-workspace and pre-model restore barrier to `TurnResume`; never overlay saved files onto a warm workspace. Dimension 4.11. |
| 5 | The provisional driver plan did not fix hook order or terminal continuation semantics. | Capture only at the continuing root's pre-model boundary; keep attempt-local first-call checks separate from saved counters. Ordinary terminal reports do not publish a new continuation checkpoint. Dimension 4.10. |
| 6 | The guide contradicted itself on one row versus six tables, retention, transport variants and skipped saves. | One head/plan design, one command family, explicit committed/skipped results, bounded retention and monotonic allocation. Dimensions 4.3 and 4.12. |
| 7 | Existing task guards require task capability; native correlation also validates response kind and lease in `downlink/pending.rs`. | Add a plain-turn guard with mutation-time connection proof, typed correlation and exact fold adoption; never reuse the task guard to broaden access. Dimension 4.14. |
| 8 | The proposal implied arbitrary permission recovery and left byte/lifetime bounds and object namespace implicit. | Specify supported file metadata, concrete limits and collision-free event namespaces; retain existing quota and purge custody. Dimensions 4.11-4.12. |

Short source paths use the crate layers in the spec's file inventory.
These decisions complete the design review; passing tests must establish the implementation's behavior.

### One capture coordinator and one cleanup owner

```text
remember complete action group -> take inputs -> task reconciliation, if any
  -> continuing root reaches before_model
     -> ask Children coordinator for exclusive permit
        +-- child future / queued start / open or reserved session -> SKIP
        +-- empty -> gate starts -> snapshot history + driver + children + memory
                                    |
                   BoundaryOwner outside agent future
                     +-- API-only ------> bounded sealed state publication
                     +-- Building ------> SKIP, never history-only for file runs
                     +-- Ready workspace
                          -> freeze confirmed
                          -> tenant process tree empty? -- no --> thaw -> SKIP
                          -> scan / upload / verify / commit
                          -> join file workers -> thaw or destroy
                          -> validated receipt; root continues only after thaw
```

`Children::beside` arbitrates the root's request while polling child starts and actual child futures.
A terminal registry status alone grants no permit: interrupted futures may still be unwinding.
The registry gates starts during capture; no synchronous mutex guard crosses an await.

`Sessions::is_quiescent` checks both open sessions and reserved starts under its existing short lock.
The root awaits capture and the coordinator admits no child, so neither can start a new executor call.
A live session causes a skip; capture never kills it merely to create a boundary.

After freeze, `Sandbox::recovery_quiescent` proves the tenant process subtree is empty.
The resident sandbox/executor leaf may remain populated; its process memory is never part of the snapshot.
Unsupported engines return an explicit unsupported result; read errors or populated trees publish nothing.

Capture terminal child IDs, status/results, depth, call counters, started count and next ID under the registry gate.
Restore terminal watches with closed input channels and cancelled tokens, without starting children.
Scrub restored result text again and validate IDs, counters and collection bounds before exposing the registry.

The root snapshot includes a `MemoryRecovery` from the shared memory backend.
It carries ordered pending entries, sorted superseded keys and capped recall misses; pending-byte accounting is recomputed on load.
Current shared-memory reads and publish grants come from the replacement's seed, never saved credentials or permissions.

`MemoryBackend::snapshot` is required and returns an error for unsupported recovery.
`Hydrated::recover` validates and scrubs the saved overlay, hides superseded own keys, and reinstalls pending entries before tools run.
A saved workspace-visible write with a revoked publish grant refuses restore; it cannot silently gain authority or vanish.

Memory already pushed after an older checkpoint remains a durable effect; recovery does not roll that write back.
This follows the same replay limitation as external tools and prevents a claim of exact process-state rollback.

`guffbolt::DriveState` stores accepted answer segments and completed-turn count.
`drive` accepts that state; `before_model` receives its read-only view plus the existing attempt-local call number.
Update the existing callers in place; `after_group` keeps its task-assessment role.

The root recovery extension runs after input incorporation and native task reconciliation, before `prepare` or another model call.
A continuing tool group reaches that boundary; an answer with follow-up input reaches it after `remember` and answer accumulation.
Never capture from `on_answer`, which can run before the answer enters messages.

Ordinary terminal paths report normally and retain their last continuation checkpoint.
A crash before final report may repeat work after that checkpoint; saved assistant text alone never proves settlement.
Native task terminal capture keeps its existing cleanup handoff and native reassessment rules, with the richer inner history format.

For terminal task capture, close the start channel and drain/drop children before closing sessions.
Do not send a terminal capture request to a coordinator whose `beside` loop already returned.
The supervisor freezes and checks the tenant tree before its existing terminal capture; leftover processes refuse that capture.

`RecoveryBoundarySink::save` returns `Result<RecoverySave>` with `Committed(receipt)` or `Skipped(reason)`.
Only a validated committed receipt advances the caller's checkpoint generation.
Skips cover busy children/sessions/processes, workspace build, unavailable storage and bounds; they keep the previous checkpoint and produce a bounded reason log.

`BoundaryOwner` lives beside build and drive inside `LeaseRun::sandboxed`, or beside drive in the explicit API-only branch.
A one-job channel carries owned snapshot bytes; the sandbox cell outlives the owner, build and agent futures.
The owner remains polled while the root awaits its reply, and the enclosing lease loop keeps renewal and activity pumping live.

Cancellation closes intake and requests cooperative stop; it never drops an in-flight freeze, thaw or file-worker handle.
Join all started blocking scans, chunk reads and restore writes before thaw or teardown.
Cancellable transfer waits leave incomplete plans for bounded cleanup; no remote operation holds a database transaction open.

A lost reply, timeout or engine panic still reaches cleanup before the sandbox can be parked or released.
An uncertain freeze cannot publish; failed thaw cancels the run, marks the sandbox unusable and hands ownership to teardown.
Destruction failure remains owned by existing boot cleanup; process or machine death cannot promise in-process cleanup.

### Event-owned publication and restore

```text
E/L1/F7 -> Begin(expected head G) -> plan P/G+1 + writer pin
         -> root bytes H + verified file pages M + object refs
         -> Commit(expected G, P, H, M, exact folds) -> head G+1
         -> lost reply? exact operation replay returns same receipt
A dies
E/L2/F8 -> offer descriptor P/G+1
         -> fenced Read + reader pin for exactly P/G+1
         -> fresh empty sandbox -> restore files -> validate root state
         -> adopt exact covered folds under L2/F8 -> release reader pin
         -> current policies / native reconciliation -> next model call
```

API-only checkpoints carry an explicit no-workspace variant and use encrypted Postgres state without object storage.
A sandbox turn carries the complete supported workspace manifest, even when the current action group used no filesystem tool.
Building or unavailable workspaces cannot claim the API-only variant.

`Build::acquire` releases a warm hold when a committed descriptor requires recovery.
A resumed file run skips initial repository checkout and restores into an empty workspace before exposing the executor slot.
Build, pin, file or history validation failure prevents every model/tool call.

Reuse the descriptor-based scanner, path/link validation, chunk encryption and private R2 transfer backend.
A manifest restores file bytes, directories, safe symbolic links and executable bits; files use normalized owner permissions and sandbox ownership.
Process memory, sockets, arbitrary ownership/mode bits, extended attributes and temporary files outside the recovery workspace are not restored.

The frozen tenant tree must be empty because this design restores files and agent state, not running application processes.
Long-lived background processes can prevent a new checkpoint; recovery then uses the last committed boundary.
Before the first checkpoint, failure replays original input and pending folds through existing admission gates.

| New table | Owner, constraints and lifecycle |
|---|---|
| `runner.turn_recovery` | One head per event, monotonic allocation counter and optional current checkpoint. Its composite reference binds the exact plan/event pair; mutators derive and check fleet/workspace from the locked lease and admission. |
| `runner.turn_checkpoints` | Immutable generation identity, writer lease/fence, expected head, compatibility witnesses, encrypted root state and manifest/history digests; pending/ready/abandoned timestamps. Unique event/generation and one live pending plan. |
| `runner.turn_checkpoint_pages` | Plan-owned numbered pages, unique plan/page number, bounded body and digest; no update to an acknowledged page's bytes. |
| `runner.turn_checkpoint_refs` | Plan/workspace to physical object references, present during capture and retention; scope-qualified foreign keys and object lookup index protect custody. |
| `runner.turn_checkpoint_pins` | Exact plan, writer/reader purpose, lease/fence, expiry and operation deadline; renewal uses resource-version comparison and fresh ownership proof. |
| `runner.turn_checkpoint_operations` | Immutable event/operation identity, request digest and receipt identity; survives private-plan deletion and prevents old operations allocating again. Stores no history or keys. |

Each new timestamped schema file includes indexes, constraints and `api_runtime` grants; registration follows foreign-key order.
Create plans before the head's cyclic reference, or close that reference in the later table file under the repository's schema rule.
Event deletion removes private state and refs; opaque operation tombstones mirror the existing task receipt retention policy.

A terminal admission makes its head unreadable to runners immediately.
Bounded maintenance removes terminal private plans after live pins end; parent deletion still uses the shared durable object purge ledger.
Failed or abandoned pending plans cannot become the head.

Reuse workspace-owned `fleet.task_objects`, `task_identity_keys`, `task_storage_quota` and `task_storage_purges`.
Use the reserved identity namespace `turn:<event UUID>` in the existing repository namespace field; current task targets use repository slugs or `api-only`.
The daemon derives that namespace from proved scope; the runner cannot supply it.

A turn reads or reuses objects only through its own plan refs and workspace/namespace proof.
Object registration locks shared quota, then object rows in identifier order before adding refs.
Orphan candidate and post-lock tombstone SQL must both inspect task and turn refs; surviving pins retain their plans and refs.

All operations use short transactions in the existing parent-first lock order.
For turn requests: workspace/fleet, host connection proof, slot, lease, admission/head/plan/pin, then quota and sorted objects where required.
No path takes a host lock after a slot; pin/retention writers share the head/plan locking order.

Validate deadlines with database time after any blocking ownership lock.
Commit also rechecks expected head, scope, current session, live lease/fence, verified pages/objects and exact incorporated fold membership.
The complete ready transition and head swap commit before any successful receipt leaves the daemon.

Lost acknowledgement retries the same operation and digest under a still-live owner.
Conflicting bytes refuse; an old or deleted operation never allocates a fresh generation.
A replacement uses stable event identity to find the latest committed head, even after several failed leases.

### Limits and compatibility

| Boundary | Selected limit / failure policy |
|---|---|
| Complete turn command or response | One MiB, including envelope; native shared maximum remains two MiB. Validate actual serialized size at writer and reader. |
| Turn manifest page | 512 KiB serialized page payload; at most 64 pages and 256 objects per registration/authorization batch. Complete envelopes still obey one MiB. |
| Physical file chunk / workspace | Existing eight-MiB chunks, 24-GiB logical file bound and 200,000 entries. Unsupported file kinds or escaping links refuse capture/restore. |
| Full turn capture / shared quota | Existing full-capture upload ceiling of 16 GiB; physical workspace quota 50 GiB. Refuse reservations before crossing either bound. |
| Transfer/pin/URL | Fifteen-minute operation deadline, pins bounded by that deadline, signed URLs at most five minutes; lease authority is independently rechecked. |
| Retained turn state | One pending plan and at most two ready plans per unsettled event. Retire an unpinned non-head before another begin needs space; otherwise skip. Generation allocation never moves backward. |
| Orphan cleanup | Existing one-hour orphan guard and batches of 256; retain `reclaim_after_at` through signed URL/pin validity before remote deletion. |
| Task inner history | Increment format from 1 to 2; unsupported formats refuse before model. Task manifest format and checkpoint pointer remain unchanged. |
| Plain root format | Version 1, strict fields/bounds, scope/setup/toolbox witnesses, full messages, exact folds, pinned input, driver, terminal children and memory overlay. |

Model/provider changes keep authenticated history but discard incompatible signed provider replay, then use current policy and current secret scrubbing.
Plain setup/toolbox mismatch refuses recovery; native tasks retain their existing goal-change and effect-reconciliation rules.
Saved child text, memory entries and answer segments receive the same scrubbing as messages.

The driver restores accepted-answer segments without replaying them as new streamed chunks.
Final saved response text can include those segments, while the new attempt emits only new output.
Billing meters and public tool-call IDs remain attempt-local; old usage never enters the new lease's report totals.

Add one native command family with begin, objects, uploads, authorization, page read/write, pin renew/release, commit, read, adopt and abandon variants.

`Expected::TurnCheckpoint` checks lease, operation and response variant; reconnect retries retain semantic operation identity with new transport request IDs. Implementation adds `client/turn_checkpoint.rs`, `lease/turn_checkpoint/head.rs`, and `TurnMessageSize` in the shared turn wire module; existing protocol `up/{mod,other}.rs`, `down.rs`, uplink/downlink dispatch and gateway dispatch enforce the complete-message budget.

Incorrect correlation, oversize envelopes, stale connections and wrong generations cannot reach restore or advance local state.

Use existing stale-lease/connection errors for ownership loss.
Add registered `RUN_CHECKPOINT_{INCOMPATIBLE,CONFLICT,UNAVAILABLE,LIMIT}` codes for recovery-specific refusals, with safe details and explicit retryability.
Unavailable storage is retryable within operation lifetime; incompatible state, conflicting replay and exceeded bounds never invent an empty fallback.

### Additional implementation files and symbols

All files below belong to the active spec's implementation scope; NEW names are selected targets, not claims of existing symbols.

| Files under `rustd/crates/` unless stated | Methods / traits / types |
|---|---|
| `runner/afr_memory/src/{lib.rs,hydrated.rs,recovery.rs,recovery_tests.rs}` | NEW `MemoryRecovery`; required `MemoryBackend::snapshot`; `Hydrated::recover` and validation; preserve `Checkpoint::push` behavior. |
| `runner/afr_sandbox/src/{engine/mod.rs,engine/tests.rs,cgroup/mod.rs,cgroup/tests.rs,bubblewrap_engine/mod.rs,bubblewrap_engine/tests/freeze.rs,unsandboxed/mod.rs,unsandboxed/tests.rs,warm_slots/tests/fakes.rs}`; `runner/afr_supervisor/src/test_support/sandbox/{mod.rs,executor.rs}` | `Sandbox::recovery_quiescent`, `LeaseCgroup::tenant_empty`; fail-closed unsupported engines; real Linux detached-process test and explicit fake behavior. |
| `harness/guffbolt/src/{lib.rs,extension.rs,drive/mod.rs,drive/tests/mod.rs,drive/tests/extension.rs,drive/tests/fixture.rs}`; its `README.md` | NEW `DriveState`; change `drive` and `Extension::before_model`, preserve `after_group` semantics and attempt-local first-call behavior. |
| `runner/afr_agent/src/nested/{registry.rs,child.rs,recovery.rs,recovery_tests.rs}`; `runner/afr_tools/src/sandbox/sessions/{mod.rs,tests.rs}` | Terminal-child snapshot/restore; registry capture gate; `Sessions::is_quiescent`. |
| `runner/afr_agent/src/loop/{mod.rs,children.rs,task.rs,shared.rs,conversation.rs,context/mod.rs,recovery_boundary.rs}` | Root recovery extension, exclusive permit, full root encoding, restored local memory and derived context rebuild. |
| `runner/afr_supervisor/src/lease_loop/{workspace.rs,work.rs,drive.rs,build.rs,recovery.rs,hold.rs,recovery_boundary.rs,recovery_capture.rs,tests/recovery/ownership/boundary.rs}` | NEW `BoundaryOwner`, workspace-state enum and one-job channel; fresh restore barrier; cleanup before releasing sandbox ownership. |
| `runner/afr_supervisor/src/recovery/{mod.rs,capture.rs,restore.rs,session.rs,catalog.rs,transfer.rs,turn/{mod.rs,capture.rs,files.rs,restore.rs,session.rs,publication.rs},tests/capture.rs,tests/restore/mod.rs}` | Reuse leaf scanner/crypto/transfer helpers, introduce typed turn owner, retain and join blocking jobs; avoid fake native task IDs. |
| `daemon/afd_fleet/src/lease/turn_checkpoint/{mod.rs,guard.rs,plan.rs,objects.rs,pages.rs,pins.rs,commit.rs,restore.rs,maintenance.rs,refusal.rs,sql/mod.rs}` and matching SQL | `TurnCheckpoints`, `TurnLeaseGuard`, generation publication, pin custody, exact fold adoption and cleanup. |
| `daemon/afd_fleet/src/task/storage/{mod.rs,transfer.rs,bounded.rs,signing.rs}`; `task/checkpoint/{identity.rs,maintenance/mod.rs,maintenance/owners.rs,maintenance/purge.rs}` and `sql/{orphans.sql,orphan_tombstone.sql}` | Share physical backend/quota with typed owner checks; protect both reference families in both orphan queries. |
| `runner/afr_supervisor/src/downlink/{pending.rs,pending_tests.rs,dispatch.rs}`; protocol/client/gateway files in the main inventory | `Expected::TurnCheckpoint`, one correlated command/response family; tests reject cross-lease and wrong-operation responses. |
| `foundation/af_core/src/{error_code/fleet.rs,error_code/mod.rs,problem/fleet.rs}`; `daemon/afd_fleet/src/error/{kind.rs,lift.rs,permanence.rs,classify/code.rs,classify/detail.rs,classify/task.rs,classify/decision.rs,classify/availability.rs}` | Registered turn-recovery refusal codes and exhaustive error classification through the existing error macros. |
| Six timestamped turn table files under `schema/`; `daemon/afd_db/src/migration.rs`, `daemon/afd_db/tests/migrations.rs` | Grants, scoped foreign keys, unique head/generation/operation indexes and registration checks. |

Split operation modules under the existing code-size limits. Reuse concrete helpers without introducing a generic repository framework.

Physical object preparation is shared in `daemon/afd_fleet/src/task/storage/registration.rs` (`Registration`, `NewObject`, `prepare`).
`task/checkpoint/{mod.rs,objects.rs,object_row.rs,identity.rs,transfer/mod.rs,transfer/verify.rs}` expose concrete scope-based crypto helpers for the event owner; ordinary recovery supplies `turn:<event UUID>`, never a synthetic task.

`main/src/plane/mod.rs` injects the same optional private file backend into both owners.
Turn `objects.rs`, `pages.rs`, `transfer.rs`, and their SQL use the shared quota and immutable-object tables under the ordinary guard.

Turn file authority and publication are split into daemon `files.rs`, `manifest.rs`, and `signing.rs`.
Runner `recovery/turn.rs` moves to `recovery/turn/mod.rs`.

Its `capture.rs`, `session.rs`, `restore.rs`, and `files.rs` children own native calls, shared file helpers, pin renewal, and joined work.
New SQL includes `refs_insert.sql`, `objects_scope.sql`, `plan_bytes.sql`, `page_read.sql`, and `page_write.sql`.

`turn_checkpoint/maintenance.rs` runs through existing task-storage maintenance before its optional-backend check, so API-only history is collected too.
It uses `maintenance_{candidates,lock_head,lock_plans,lock_pins,delete,head,operations,orphans}.sql`; expired pending plans and seven-day terminal state are eligible only after pin rechecks.

Reader custody retains original operation IDs, with at most 64 reader pins per generation (`reader_capacity.sql`).
This exceeds the ordinary attempt limit and bounds maintenance lock work.

`runner/afr_supervisor/src/lease_loop/task_workspace.rs` extracts the existing native-task restore/base preparation from `recovery.rs`; ordinary restoration remains in `turn_recovery.rs`. Both paths keep their existing freeze/cleanup owner.

`daemon/afd_fleet/tests/integration_turn_checkpoints/{files.rs,storage.rs}` reuses the existing `integration_tasks/decisions/checkpoint_server.rs` HTTPS/S3 leaf fixture. Its tests exercise event-namespaced registration, immutable upload descriptors, authenticated publication and reader transfers, independently of native-task authority. The positive round trip and negative corruption case require live runs before they count as proof.

The shared test server becomes crate-visible through `integration_tasks/{mod.rs,decisions/mod.rs,decisions/checkpoint_server.rs}`. `runner/afr_supervisor/src/test_support/daemon/stream.rs` forwards the new correlated command to unit fixtures; an unconfigured scripted peer explicitly refuses checkpoint custody.

### Selected SQL statement inventory

All new statements below live in `rustd/crates/daemon/afd_fleet/src/lease/turn_checkpoint/sql/`.
These names separate operations for prepared-statement checks and code-size limits; existing task statements retain their task ownership.

| New files | Calling operation / guarded change |
|---|---|
| `scope.sql`, `lock_scope.sql`, `lock_slot.sql`, `lock_lease.sql`, `lock_admission.sql`, `fresh_clock.sql` | `TurnLeaseGuard::lock`: parent first; reuse `Leases::lock_host` for current digest/connection, then slot/lease/admission. Recheck time after child-lock waits. |
| `head.sql`, `head_insert.sql`, `head_allocate.sql`, `plan.sql`, `plan_insert.sql`, `operation_replay.sql`, `operation_insert.sql`, `abandon.sql` | `begin`/`abandon`: lock event head, allocate monotonic generation and immutable operation identity; abandon only the proved pending writer. |
| `objects_content.sql`, `objects_insert.sql`, `refs_insert.sql`, `objects_scope.sql`, `objects_verify.sql` | Register/verify: typed event namespace, shared quota/object locks, immutable ciphertext and scoped references; reuse common quota/crypto helpers. |
| `pages_allocate.sql`, `page_write.sql`, `page_read.sql`, `pages_complete.sql` | Allocate/write/read bounded ordered manifest pages; identical replay only. |
| `pin_insert.sql`, `pin.sql`, `pin_renew.sql`, `pin_release.sql` | Exact generation custody and resource-version renewal; live ownership and operation deadline on every authorization. |
| `commit.sql`, `load.sql`, `validate_folds.sql`, `adopt_folds.sql` | Verify complete objects/pages/state; publish ready plan and compare-and-swap head; load exact descriptor; adopt only validated covered inputs. |
| `retention_due.sql`, `retention_lock.sql`, `retention_delete.sql`, `cleanup_terminal.sql` | Bounded private-state cleanup; never delete a current unsettled head or pinned generation; retain opaque replay receipts. |

Existing `task/checkpoint/sql/{orphans.sql,orphan_tombstone.sql}` gain turn-ref checks under the existing object-lock and purge sequence.
`schema/20261108123100_runner_runners.sql` changes in place; all six turn tables use new timestamped schema files.



### Added scenario sequences and proofs

```text
child says Interrupted -> future still draining -> permit denied -> old checkpoint stays
exec session closed -> detached tenant remains -> freeze/probe -> thaw/skip
root cancelled -> capture receiver gone -> blocking read joined -> thaw/teardown
memory.store/forget -> root snapshot -> kill -> restore overlay -> same local tool view
answer A -> follow-up -> snapshot -> kill -> answer B -> final text A + B
old warm workspace -> committed TurnResume -> retire hold -> empty restore -> model
commit G+1 -> acknowledgement lost -> same operation -> receipt G+1, no G+2
G and G-1 pinned -> next capture wants space -> skip -> never delete pinned objects
superseded connection -> read/commit/adopt -> refuse -> no private state or cursor change
final answer streamed -> crash before report -> last continuation checkpoint -> report once
```

Dimensions 4.8-4.14 cover these branches alongside the existing recovery Dimensions.
The complete ASCII atlas remains in the active spec; no proposed test is marked passed by this review.

The human lab belongs in `/workspace/agentsfleet-m247` and needs the dedicated daemon plus two isolated runner containers.
It is not prepared yet; the earlier commands remain a target procedure until actual container IDs and the checkpoint barrier are supplied.
Build and focused unit checks precede preparation; required automated integration may now precede Indy's morning smoke.

The lab must cover an API-only ordinary turn and a file-backed recovery, with checkpoint/restore receipts and a controlled post-checkpoint barrier.
Acceptance retains its separate authorization and environment requirements.

### Review outcome and model handoff

The capture and filesystem design decisions are settled in this plan; runtime implementation and failure proofs remain open.
M246 is still PENDING. The Nov 18 scope decision parks tenant-owned placement; it no longer blocks this platform-runner delivery, and M247 must not claim those controls exist.
The full-history secret scan also remains unresolved as recorded above.

This design provides explicit invariants and tests, not formal verification or deterministic model output.
Recovery can repeat external actions after the last saved boundary, and it can refuse incompatible state safely.

Review checks passed: `orly doctor`, the changed-document language scanner and `git diff --check`.
`gitleaks dir docs/v2 --redact --no-banner` scanned 17.14 MB and reported no leaks; the historical scan remains unresolved.
A document check counted six Sections, 500 spec lines, 52 ASCII scenarios and 14 recovery Dimensions.

The tracked runtime diff and untracked hold test hashes match their values before this review.
No product test or manual recovery drill ran during this documentation review.

The review is complete after Indy's confirmed Astra 6 Max selection.
Indy confirmed the switch back with “Yes switched, keep going”; runtime implementation has resumed.
Continue the existing capacity draft by pairing the keeper reserve with the accepted-hold acknowledgement.


### Implementation resumed after the manual model switch

Indy confirmed: “Yes switched, keep going”.
He then asked: “Keep going, i will come and review.”

Section 1 now has keeper transfers and accepted-hold acknowledgement drafts beside the capacity census.
Shared attempt-keyed guards cover both fleet and authoring entry points, including callers outside the worker pool.

Sections 2 and 5 have quarantine, connected-heartbeat return and operator API/CLI drafts.
The connected write compares the current assignment witness after the host lock; a changed assignment cannot advance healthy proof.
The standalone offline SQL was folded into the atomic quarantine statement and removed.

The first new lint attempt found report-row conversion and fixture signatures; those compile errors were corrected.
The runner unit lane then stopped at one remaining old occupancy fixture; it was updated before the next checks.
No new integration or end-to-end recovery result has been recorded.

Section 3 now has daemon-stamped output fences and a post-commit restart publication draft.

The runner unit retry stopped at two executor process-group assertions (55 passed, 2 failed in that suite).

Evidence: `/tmp/m247-section1-runner-unit-retry.log`; the host PID 1 is `tail`, and orphaned `sleep` processes remain zombies.

The cause still needs a clean rerun with an adopting/reaping parent; no lane pass is claimed.

The client drafts reject missing, malformed and older decimal-string fences.

A greater attempt resets the live decoder, draft, reasoning clocks, task/context progress and read owners once.

The CLI separates printed attempts with “Run restarted”; the browser keeps that marker on the existing reply.

Files: `fleet-stream-attempt.ts`, `attempt-output.ts` and the corresponding registry/render tests.

`make lint-app lint-cli` passed after the first type correction (`/tmp/m247-section3-clients-lint.log`).

The first client unit attempt was stopped after two callback-cleanup regressions and unrelated setup failures appeared.

The cleanup guards now release departed subscribers without changing their rows; a rerun is required.

Machine `AGENTSFLEET_*_ENV_FILE` variables name missing files; the unit retry must omit those unavailable machine inputs.

Evidence: `/tmp/m247-section3-clients-unit.log`; this interrupted run supplies no coverage pass.

Section 4 has initial `DriveState`, `MemoryRecovery`, session-reservation and tenant-cgroup proof drafts.

`DriveState` retains accepted answer segments/completed turns while provider-call numbers remain attempt-local.

`MemoryRecovery` reinstalls pending writes and hidden keys against current grants and the current secret scrub.

The frozen tenant subtree must prove empty; a resident executor alone may remain.

These primitives still require the root snapshot, capture owner, native turn protocol and event-owned store wiring.

The first Rust lint attempt lacked cargo-shear on the non-login PATH; the retry found a custom-validator reference lint, now corrected.

No Section or recovery Dimension is marked complete on this evidence.

The corrected browser unit retry passed 4,644 tests in 491 files, with all lines/functions covered.

Its coverage gate remained red: 99.96% statements and 99.91% branches versus the required 100%.

Evidence: `/tmp/m247-section3-clients-unit-retry.log`; targeted disposal, ownership and saved-trace cases were added afterward.

The following `make lint-app lint-cli` passed (`/tmp/m247-section3-clients-lint-retry.log`).

The native task inner history is now format 2 and pairs messages with driver/child/memory state and the pinned index.

Restore rejects format 1 and validates the saved root before any model call; current memory grants and secret policy still apply.

The child coordinator gates starts only after checking its actual future set and start queue under registry custody.

Terminal cleanup closes intake and drops children before sessions close and history captures.

A typed boundary sink and receipt witness are drafted; mismatched event/lease/fence/history/generation receipts stop continuation.

The supervisor still supplies no live sink. Its independent capture owner, ordinary-turn state/store and native request family remain unfinished.

No recovery integration, human smoke or end-to-end recovery result has been recorded.

The next browser retry passed its complete package gate: 4,649 tests in 491 files and 100% of statements, branches, functions and lines.
Evidence: `make test-coverage-app` within `/tmp/m247-section3-clients-unit-retry2.log`.
The CLI package follows in that same lane; no CLI pass is claimed before its completion.

Fold claims now retain their stable event IDs beside sequences and record the exact incorporated list in the private root draft.

This prepares the later native commit/adoption checks; the ordinary checkpoint store and adoption writer are still missing.

Accepted sandbox-hold receipts now also narrow the local deadline; another lease cannot change it and an acknowledgement cannot extend it.

Those latest Rust changes still need compilation and runtime checks.

The following CLI package run reached 100% function/line coverage but failed one loopback streaming fixture.

It still emitted unfenced chunks; its mock producer now emits the daemon's current fenced shape.

Evidence: `/tmp/m247-section3-clients-unit-retry2.log` (2,376 passed, 22 skipped, one failed).

The package unit target includes these loopback transport cases; no live runner-recovery acceptance test ran.

Turn compatibility needs a durable witness of the setup delivered by its lease.

The issue statement already rechecks immutable revision `$20` but did not retain it in `runner.leases`.

The draft now stores that UUID as nullable `setup_revision_id` in the existing schema file and insert statement.

New ordinary-turn plans will derive their setup identity from this frozen lease fact, rather than a later fleet head or a runner assertion.

Null remains possible for old/fixture issues; it cannot authorize a new turn checkpoint.

The witness has no foreign key because settled lease history survives fleet/setup deletion.

The event-owned schema draft now has six registered tables under `runner`: head/allocation, plans, pages, object references, pins and opaque operation receipts.

The complete private root is sealed in the plan row; files reuse `fleet.task_objects` and private R2 custody.

Both existing object-orphan statements now honor turn references as well as task references.

No datastore migration or recovery integration lane had run at that point.

The CLI retry passed: 2,377 tests, 22 live-target skips, zero failures and its 100% function/line floor.
Evidence: `make test-coverage-cli`, `/tmp/m247-section3-cli-unit-retry.log`.
The runner unit shard is being retried under an adopting/reaping parent because this machine's PID 1 does not reap orphan processes.

That retry passed the executor and sandbox suites, including the corrected tenant-process proof.

The new proof had accidentally read `memory.events`; it now reads the tenant's `cgroup.events`.

The negative fixture puts misleading bytes in the memory counters to prevent the same regression.

Evidence: `/tmp/m247-runner-unit-subreaper-retry2.log` (sandbox: 293 passed).

The full runner shard remained red on four supervisor hold/heartbeat assertions.

Parking now consumes only a matching fleet reservation; an authoring reservation cannot disappear during a fleet park.

Hold retirement after keeper close no longer schedules a redundant heartbeat.

Two hold fixtures now prepare three workers for two holds plus the mandatory free worker.

A supervisor `BoundaryOwner` draft is now connected beside build/drive and in the explicit API-only branch.

It retains a live capture through caller loss and attempts thaw after failed freeze, publication failure or panic.

An unavailable building workspace skips capture; failed cleanup cancels the run before its sandbox can be held.

New tests cover lost callers, foreign authority, busy tenant processes and freeze/thaw failures.

Those changes still require compilation and runtime checks.

The ordinary-turn protocol, event-owned store and restore/adoption wiring are now drafted; live runs remain required.

No human smoke, live recovery integration, host-crash acceptance result or Pull Request is claimed.

The next draft adds a descriptor-only ordinary offer and an issued setup witness to `LeasePayload`.

Native negotiation is version 2 so an old runner cannot ignore ordinary resume metadata and run fresh.

The offer builder now looks up the event-owned committed head under the issued lease guard and sets `turn_resume` to its descriptor. A missing head returns no descriptor; a present incompatible head refuses.

`TurnSnapshot` and `TurnRecovery` now separate captured state from restored/adopted state.

Their private format binds event/fleet/workspace/setup, file mode/toolbox, model/provider and complete root state.

Restore compares the exact offer, digest and adopted fold membership before installing any root.

Unsupported formats refuse; a changed model/provider discards provider replay and retains authenticated work.

The agent restores the incorporated fold list and starts its suffix claim from that sequence under the replacement.

The supervisor's new owner now supplies the native task sink through the real `drive` call.

The ordinary branch now calls the event-owned native publisher for API-only and matching-file captures. It returns a committed receipt or a classified skip.

The supervisor restores the root and matching files before adopting folds and installing `TurnRecovery`.

The native command family, files/pins, publisher, reader and maintenance path are drafted; live failure tests remain necessary before manual testing.

`Aad::turn_recovery` and its negative crypto test bind owner, generation, format and wrapping version.

The new ordinary validators and owner tests still need a successful runner-shard result.

The first owner lint retry found a large receipt enum and an unnecessary lifetime declaration.
The receipt now uses owned indirection and the lifetime declaration was removed; the repository lint target is being retried.
Evidence for the red: `/tmp/m247-recovery-owner-rust-lint.log`; no whole Rust or repository pass is claimed.

An implementation inspection found another quiescence requirement in `afr_executor/src/server/session.rs`.

Cancelling an agent-side file call does not end the executor's `spawn_blocking` worker.

A child can therefore be terminal while its old file operation still owns the workspace.

The draft now sends an ordered `fs/recovery_idle` query before freezing the executor's resident threads.

The executor checks its actual file-worker set; a busy worker skips live capture and blocks terminal publication.

Only an idle reply proceeds to freeze, followed by the separate tenant-cgroup proof.

```text
child caller ends -> file worker still running
root gated -> ordered idle query -> busy -> keep previous checkpoint
worker finishes -> next boundary -> idle -> freeze -> tenant empty -> capture
```

`Executor::recovery_idle` fails closed by default; `Client` serves the native query.

New server/client/owner tests cover unfinished file work, busy/idle replies and no freeze while busy.

The supervisor's fake sandbox and executor now have separate fixture files to retain the source-size bound.

This discovery adds no process-memory restoration and does not kill work to create an idle boundary.

The latest draft still needs a green Rust lint and runner-shard run before any live recovery claim.

The following `make test-unit-rustd-runner` passed: 1,812 tests, with the adopting/reaping parent and normal test file permissions.

Evidence: `/tmp/m247-runner-unit-recovery-primitives.log`, ending `unit: runner — 1812 passed`.

It includes native/ordinary root validation, child and memory state, worker holds, executor file-job proof and capture cleanup.

This is a datastore-free runner shard; it is not a live host-loss, recovery integration, kernel or acceptance result.

The full Rust lint target is being rerun after the latest executor changes.

Checkpoint creation is automatic at proved continuation boundaries; a person does not issue a save command.

It happens before failure. A crashed host cannot create a final checkpoint on demand.

Capture may skip when processes, file jobs, children, sessions, bounds or storage prevent a complete safe save.

Checkpoint latency has not been benchmarked. The runner unit target's elapsed time is not a capture benchmark.

API-only ordinary saves use sealed database state; file saves scan the workspace and upload new encrypted content.

The selected owner holds freeze through scan, transfer, verification and commit, then thaws before acknowledging.

A large workspace or slow transfer can therefore pause the sandbox for a material interval.

The fifteen-minute transfer deadline is an operation bound, not a normal latency target or measured performance.

Measure first/incremental capture and freeze-to-thaw separately in the prepared recovery lab before claiming acceptable latency.

The database-only case is the new ordinary API-only path; existing native task history also uses private object storage.

For illustration, 100 MiB of new content at 20 MiB/s needs at least five seconds of upload alone.

That calculation excludes scanning, request latency, verification and commit; it is not a measured checkpoint result.

### Checkpoint benchmark plan

Verify the wired publisher and reader first, then prepare the manual lab described above.
Required overnight integration may precede Indy's manual smoke; benchmark results still require the complete live publisher/reader.
Unit-suite duration, virtual clocks, mocked storage and model response time are not checkpoint latency evidence.

Use release binaries on an isolated Linux runner with real cgroups, PostgreSQL and a private test object-storage namespace.

Use a scripted model so every trial reaches the same continuation boundary without model timing variance.

Record machine resources, storage/network placement and dataset sizes with the result.

Keep first connection/capture separate from runs with initialized clients and previously committed objects.

Generate repeatable file contents with distinct chunks and record the fixture seed.

Use a fresh event namespace for first-upload trials so existing content cannot quietly turn them into reuse trials.

| Measurement | Start / finish |
|---|---|
| User-visible wait | Root enters checkpoint work / root is allowed to continue toward its next model request |
| Freeze request | Freezer request starts / kernel confirms the freeze |
| Frozen interval | Kernel confirms freeze / kernel confirms thaw |
| Local work | Workspace enumeration, hashing and encryption, timed separately |
| Transfer | Object registration/authorization, upload and native verification, timed separately |
| Publication | Commit request starts / committed receipt arrives; record database lock wait separately |
| Restore | Exact descriptor read / files, history and folded-input adoption all validate |
| Crash-to-resume | Failure injection / replacement starts its next scripted model request; separate lease expiry and scheduling from restore |

Use monotonic clocks for elapsed durations and correlated structured records for each trial.

Record logical bytes, file count, new/reused object bytes, request counts, skips/refusals, processor time and peak memory.

Verify the restored bytes/history and lease renewal alongside timing; a fast skipped or inconsistent checkpoint is not a successful sample.

Measure each process's intervals locally; never subtract monotonic timestamps from different hosts.

The failure controller and scripted model observer use one clock for the end-to-end crash-to-resume result.

Report checkpoint age at failure and work repeated after restore alongside recovery time.

| Workload | Cases |
|---|---|
| Ordinary API-only state | Small, medium and near-envelope-limit roots; no file-storage dependency |
| Native task history without files | Same history-size classes through its existing object-storage path |
| First file capture | 10 MiB, 100 MiB and 1 GiB datasets; report first-upload cost |
| Incremental capture | No file changes, small edits and broad edits against the same committed dataset |
| File layout | Few large files versus many small files; include a large unchanged workspace to expose scan cost |
| Storage/network | Baseline placement plus controlled latency and bandwidth on the isolated test host |
| Concurrent saves | One capture, then several eligible workers capturing together; record worker count, saturation and database lock wait |
| Bounds | Representative near-limit cases and expected typed refusals; report these individually |

Shape storage traffic separately from the daemon control connection when measuring transfer cost.

Use a separate combined-load case to check heartbeat and renewal behavior under broader contention.

Compare restored file digests, history, incorporated inputs and the next scripted action with the expected fixture.

After warm-up, collect at least thirty successful baseline samples per ordinary-sized case and report median, p95 and maximum.

For expensive near-limit cases, report each measured result and its conditions instead of inventing a percentile from a few trials.

Retain raw trial records and a stage-breakdown table so slow results can be attributed to scanning, transfer or commit.

Agree a latency budget from this evidence before grading performance; the current fifteen-minute deadline is not that budget.

If upload dominates the task wait, evaluate an immutable local snapshot with later transfer and review the ordering/cleanup changes before adopting it.

### Ordinary allocation draft

`Leases::begin_turn_checkpoint` now has a concrete event-owned allocation path.

It accepts a validated native command and proves workspace/fleet, current runner connection, slot, ordinary lease and unsettled admission ownership.

The guard owns its transaction, rereads the database clock after waits, and closes its connection on cancellation.

Native task admissions and leases are refused by this path; no task capability is borrowed to authorize ordinary chat.

Begin locks the event head, compares the exact expected predecessor and allocates a monotonically increasing generation with preassigned page IDs and a writer pin.

The pin starts with two minutes of custody and has the original fifteen-minute operation ceiling; renewal compares its resource version and preserves that ceiling.

An immutable allocation receipt binds the operation to its request digest.

Exact replay returns the original live plan/pin; changed content or a collected plan refuses without allocating again.

Pending allocation does not publish a usable head or private root.

| Implemented draft | Files / symbols |
|---|---|
| Ordinary ownership | `lease/turn_checkpoint/{guard.rs,transaction.rs}`: `TurnLease`, `TurnLeaseGuard::{lock,fresh,commit}`; `Held::can_continue` rechecks current host state while retaining automatic-cordon/draining continuation |
| Allocation / retention | `plan.rs`, `allocation.rs`, `row.rs`, `pins.rs` and matching SQL: one pending plan, at most two ready plans, unpinned non-head retirement |
| Public refusal mapping | `TurnCheckpointRefusal`, fleet `ErrorKind` and exhaustive classifiers retain the four registered recovery codes |
| Native shapes | `af_wire/src/lease/turn_checkpoint/{command.rs,response.rs,mod.rs}`: bounded command/action/response types; correlated reports-lane frames validate the full encoded envelope on both peers |
| Queued live proofs | `afd_fleet/tests/integration_turn_checkpoints.rs`: lost begin acknowledgement, changed replay, surviving allocation receipts, foreign authority, parked/settled sources and revoked/drained/degraded hosts |

The allocation/schema path still needs live checks; its database tests remain ignored in datastore-free lanes.

Publication, protocol envelopes/correlation, file transfer, restore/adoption and maintenance are drafted.
Their live proofs and the two-process drill remain necessary before the manual lab is ready.

`claim_inputs.sql` now retains the `task_generation` barrier after privacy cleanup nulls a task foreign key.

The new integration case checks that an earlier valid steer is returned while that row and later conversation inputs remain untouched.
Its live result remains pending.


### Current verification boundary

The first full integration attempt reached the migration invariant suite and failed six checks. Its log is `/tmp/m247-full-native-integration-persistent.log`; this is a failed attempt, not a repository pass.

The schema correction uses the durable event parent, full foreign-key names and `cordon_healthy_since_at`; immutable tables have update-refusal triggers and explicit grants.

The migration inventories now include all six new tables. Application limits stay in the validated Rust types; the schema retains structural constraints.

Maintenance locks at most 64 eligible plans per event and deletes only that selected set after fresh pin/deadline checks. Opaque receipt cleanup is also bounded.

The Rust syntax-tree size check reports zero oversized changed functions and zero parse errors after splitting registration, decryption and restore helpers.
Strict compilation is being rerun.

No process-crash result, checkpoint latency measurement, human smoke or merge-readiness claim follows from those edits.


### Exact-base verification counts

The base is `97224ed7432c9dd124ea8797d4324156b7022c07`.
Both workflow heads were read back from GitHub before copying these counts.

| Lane | Base passed | Evidence |
|---|---|---|
| Rust runner unit | 1,777 | [run 44180867822](https://github.com/agentsfleet/agentsfleet/actions/runs/44180867822), `unit: runner` deciding line |
| Rust daemon unit | 172 | Same run, `unit: daemon` deciding line |
| Rust daemon libraries unit | 3,698 | Same run, `unit: daemon libraries` deciding line |
| App unit | 4,627 across 490 files; 100% coverage | [run 44180861605](https://github.com/agentsfleet/agentsfleet/actions/runs/44180861605), job 134384504077 |
| CLI unit | 2,359; 22 live skips; zero failures | Same run, job 134384503543; 2,381 total tests across 244 files |
| Website unit | 133 across 20 files; 100% coverage | Same run, job 134384504003 |
| Design system unit | 516 across 65 files; 100% coverage | Same run, job 134384503615 |

The base integration workflow is [run 44190178041](https://github.com/agentsfleet/agentsfleet/actions/runs/44190178041).
Its head matches the same base, and every job passed.
The merged report records `line coverage 98.0027% >= 98% floor`: 107,656 of 109,850 lines covered, 2,194 missed.

| Base coverage shard | Passed test invocations | Evidence job |
|---|---|---|
| Runner | 1,239 | 134417343046 |
| Stream | 41 | 134417343024 |
| API | 1,329 | 134417343053 |
| Substrate | 4,359 | 134417343116 |
| Daemon | 284 | 134417343200 |
| Kernel | 72 | 134417174724 |

These counts sum the job's `test result: ok` lines, including unit tests run under coverage.
They are not counts of integration-only tests or unique tests across shards.
Branch counts and final gate evidence remain separate until their commands finish.

### Process drill wiring

`daemon/main/tests/recovery_process` extends the existing booted-daemon scenario over real PostgreSQL and core NATS.
Its two workers run in separate processes with independent local homes.
The worker uses the production supervisor and agent loop, with the existing pinned TLS model-client seam as its only transport injection.

The parent admits one follow-up before the save and another after it.
It waits for the committed head and the following model request, kills the first process, and deletes that worker's home.
The second process must wait for the real saved deadline, restore the accepted prefix and own both input claims before its model response.

The assertions also require a higher fence, one final joined answer, current-attempt token accounting and one atomic offline/cordon episode.
The test remains ungraded until its live run passes.
The integration and coverage recipes build the test-only worker where their test binary expects it.


### Secret scanner comparison

`gitleaks protect --staged --redact --no-banner` passed with zero findings across about 619 KB of staged content.
The full-history `gitleaks detect --redact --no-banner` returned 13 findings and failed.

The same command in the isolated exact-base checkout returned the same 13 locations, rules and commits.
No additional history finding appeared on this branch at that point.
This comparison does not turn the failed full-history scan into a pass or approve a suppression.

The redacted reports are `/tmp/m247-history-gitleaks-redacted.json` and `/tmp/m247-base-history-gitleaks-redacted.json`.
The owner must resolve or explicitly decide the existing history findings before claiming that required full-history check passed.


## Reviewed failure repairs and focused proof, Nov 18, 2026

The source inventory lists every changed path and the added, changed or removed declarations.
Rust and TypeScript syntax trees identify functions, methods, traits, structs, enums and types; SQL rows identify their definitions.
It complements the design tables above.

| Failure | Implementation | Focused evidence |
|---|---|---|
| Ordinary pages exceeded their native message ceiling | `Catalog::pages` takes the receiving byte ceiling; `layout::Pages` packs ordinary pages below 512 KiB | Runner regression passed with 3,000 long paths |
| A local workspace exceeded bounds | `CaptureLimit` returns a typed skip after thaw; cleanup and authority errors retain their failure behavior | Oversized sparse workspace regression passed |
| Every commit acknowledgement was lost | `NativeTurn` retains its exact pending operation and expected descriptor across capture jobs | Lost-acknowledgement regression passed; the following capture advances |
| Four sequential downloads exceeded their shared pin window | A bounded batch starts four downloads together, retains manifest order and checks custody before writing | Four verified HTTPS transfers and restored file bytes, link, modes and ownership passed |
| Directory creation continued after cancellation | Restore checks custody before each directory job | Cancellation leaves the empty workspace unchanged |
| Manifest pages changed order or a composed link escaped | Server compares the allocated page ordinal and validates the complete link graph before head publication | Wrong ordinal, cycles, composed escape and valid chain tests passed |
| Pins or plans expired during a lock wait | Commit and adoption read fresh database time after manifest or fold locks | Controlled commit and adoption waits refused; head and input ownership remained intact |
| Event identity keys outlived their objects indefinitely | Bounded key scans retain repository keys, live objects and pending purges; the quota lock serializes registration | Fair scan past 101 repository keys and purge-custody cases passed |
| Cleanup selected an object before a new reference committed | Cleanup rechecks references after its quota/object locks | Live registration/cleanup race passed; retained content survived and terminal cleanup released quota once |
| Immutable receipt cleanup requested an update lock | Receipt collection uses bounded delete queries without an update-lock privilege | Maintenance runs passed under the API runtime role; immutable grants remain enforced |
| Fleet issue consumed an idle hold before refusing attachment | Existing issue transaction rolls back the hold, lease, host choice and counters | Both real fleet-issue capacity tests passed |
| Restart had no output row or spoken notice | Restarted rows remain visible and the existing polite status announces each observed attempt once | Focused regressions were authored; full app and rendered-browser checks remain pending |

`make -f Makefile -f /tmp/m247-inner-lanes.mk checkpoint-inner` exited 0 on the working tree.
It ran 28 migration tests, 19 checkpoint/recovery tests, two fleet-issue capacity tests and the process drill.
These focused commands do not replace the final repository gate.

The process drill passed `recovery_process::killed_runner_mid_turn_restarts_on_another_host` in 111.04 seconds.
It killed the first OS process and removed its home, then proved restoration, exact saved/new input ownership, one final answer, current-attempt billing and automatic quarantine.
The controlled model fixture makes this a process-level recovery proof; paid provider acceptance and Indy's human check remain unperformed.

### Lost commit acknowledgement

```text
runner A                 daemon / PostgreSQL
   | Commit(op, root G1) -------> | head = G1
   | <------- all replies lost -X
   | retain exact op + root      |
   | thaw; continue the run      |
   |                             |
next safe boundary               |
   | retry same op + root ------>| return receipt G1
   | Begin(expected G1) -------->| allocate G2
   | Commit(root G2) ----------->| head = G2
   | <------ exact receipt G2 ---|
   | thaw; advance agent cursor  |
```

The capture owner retains the uncertain command in memory and wipes its private bytes when it settles or the owner ends.
A host crash loses that local command; the committed database head remains available to the replacement.
A definitive refusal permits another capture only after abandonment proves the old pending generation did not publish.

### Registration racing cleanup

```text
controller             native registration            cleanup
 lock object O          lock shared quota Q              |
      |                 wait for object O                |
      |                          |                  select orphan O
      |                          |                  wait for quota Q
 release O -----------> register reference; commit Q     |
      |                          |                  acquire Q and O
      |                          |                  recheck references
      |                          |                  keep O
```

The later positive control removes terminal recovery ownership, deletes the private object and acknowledges quota release exactly once.
The tests use real PostgreSQL and core NATS, plus the production object SDK against a local HTTPS object-storage fixture.

## Earlier consult records

These entries describe the earlier planning/review state. Current requirements and proof status are recorded above and in the scalability review.

- **Model reviews:** `gpt-6-astra`, reasoning `ultra`: initial review found 12 issues, all accepted. Final review found four clarifications and one diagram correction, all accepted. Revisions add atomic keeper transfers, degraded-health resets, mutation-time connection proof and first-observation semantics; recovery design blockers remain open.
- **Full-spec review requested Nov 17, 2026:** `gpt-6-astra`, reasoning `ultra`, reviewed the entire specification and source: nine material findings accepted. Seven major amendments cover late holds, authoring occupancy, drains, exact operator commands, metering exceptions, provisional trace lifetime and the human smoke. Two critical blockers remain: plain-chat state/disk consistency and a safe live-capture handoff. A focused follow-up found four further interface/proof issues: preserve fold IDs, add fenced checkpoint reads, acknowledge accepted holds and prepare a controlled manual lab. Those amendments are included; lab setup remains ungraded.
- **Current source review:** `git fetch origin main` returned HEAD and `origin/main` `97224ed7432c9dd124ea8797d4324156b7022c07`. PR #803 merged as `67f5f7d19`.
- **Earlier open design questions:** Plain-chat format/bounds, disk consistency, native acknowledgements and live capture ordering. The confirmed-selection review resolves these in the plan; platform implementation proofs remain open; the Nov 18 decision parks tenant placement.
