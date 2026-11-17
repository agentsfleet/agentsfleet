---
type: explanation
audience: contributor
verified: 2026-11-17
product_version: 0.77.0
executable: false
---

# M247 implementation design and manual recovery check

This guide explains the [M247 spec](../active/M247_001_P1_API_CLI_DOCS_OBS_UI_FAILED_HOSTS_RESPAWN_FROM_CHECKPOINT.md).
It maps the proposed behavior to source, describes transaction boundaries, and gives Indy a manual check before automated recovery lanes.
The spec remains the authority; this guide introduces no additional implementation milestone.

Source baseline: fetched `origin/main` and HEAD `97224ed7432c9dd124ea8797d4324156b7022c07`.

Existing commands below were checked against their parser definitions. Proposed signatures, fields, logs and results describe implementation targets.
No runtime code or datastore has been changed for this design review.

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
| `afr_supervisor/src/worker_pool/mod.rs` under `rustd/crates/runner/` | `Worker::next` | Create one occupation guard for either fleet or authoring dispatch, including cancellation and error paths. |
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
| Proposed `RecoveryBoundarySink::save(snapshot)` | Async, object-safe port returning `Result<RecoveryReceipt>`. No default no-op implementation may claim recovery support. |
| Proposed `RecoverySnapshot::{PlainTurn,Task}` | Own scrubbed history, immutable scope witnesses and incorporated cursors; exclude keys and signed transfer URLs. |
| Proposed `RecoveryReceipt` | Name committed generation, snapshot identity, digest and covered inputs; validate before advancing local recovery state. |
| Existing `guffbolt::Extension` | Reuse its async `after_group` and `before_model` hooks; introduce an agent-owned plain extension instead of changing guffbolt's shared trait. |
| Existing `Sandbox::{freeze,thaw}` | Reuse only after supervisor-owned workspace availability and whole-run quiescence are proved. |
| Existing `TaskRecovery::{capture,restore}` | Reuse task history identity/validation; introduce separate `TurnRecovery` for event-owned plain turns. |
| Existing `Sweep`, `CheckpointStore`, `RunnerApi`, `Services` | No breaking method change is required for the confirmed work. |

An action group is safe only after its model response and tool results are remembered together.
`after_group` covers tool-call groups; answer-only continuation needs the next valid boundary or terminal handoff.
Do not checkpoint a fold cursor ahead of the conversation bytes that include its message.

`Children::beside` currently polls nested work while the root awaits other operations.
A root hook alone therefore cannot freeze the workspace safely.
The live-capture implementation must prevent new child work and close executor sessions before capture; otherwise it keeps the previous checkpoint.

The supervisor must own this handoff while its renewal loop stays live.
Capture cannot borrow a workspace before sandbox construction finishes or outlive the lease's cancellation.
These quiescence and lifetime details remain implementation-blocking design work in Section 4 of the spec.

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
Move local occupation accounting to the shared dispatch boundary; otherwise authoring can consume the reserved worker while the keeper retains a hold.
Atomic held-to-busy and busy-to-held transfers prevent double charging the same sandbox.

### Offline episodes, return and drain completion

| SQL/schema file | Change |
|---|---|
| `schema/20261108123100_runner_runners.sql` | Add nullable `cordon_reason`, `cordoned_at`, `cordon_healthy_since`; require reason/time to agree and the healthy marker to belong to an automatic cordon. |
| `afd_runner/src/sql/sweep/select_due_runners.sql` | Select actionable offline episodes and actionable drains, or use a fair bounded cursor. Old stale/draining rows cannot block later hosts. |
| `afd_runner/src/sql/sweep/cordon_silent_runner.sql` | New statement under the host lock: recheck stale `last_seen_at`, set the automatic fields and write the reason audit. |
| `afd_runner/src/sql/sweep/insert_offline_event.sql` | Keep episode uniqueness inside the same transaction as cordon; a failed cordon commits no offline event. |
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
root reaches proved safe point
  -> supervisor freezes workspace
  -> Capture::run: encrypted files + matching private history
  -> existing checkpoint pages / objects / commit
  -> Postgres ready manifest + task pointer COMMIT
  -> validate RecoveryReceipt
  -> update local task recovery generation
  -> thaw and continue while same lease remains live
```

Plain chat needs an event-owned checkpoint store and restore path.
Proposed `TurnCheckpoint` binds workspace, fleet, event, writer lease/fence, generation, setup/model witnesses, history digest and exact incorporated fold membership.
`TurnResume` carries a validated restore descriptor, rather than a predecessor lease pointer alone.

The offer carries only that bounded descriptor, never the saved history bytes.
The replacement sends correlated `Up::TurnCheckpointRead` through the existing reports lane with lease, fence, checkpoint identity and generation.
The daemon proves current session and live slot/lease ownership, then decrypts and returns the exact version in `Down::TurnCheckpointRead`.

`Plane::read_turn_checkpoint` validates the response descriptor, digest, format, scope and current policy before the agent starts.
Use the same proposed wire/client/dispatch module for save and read; a stale owner cannot fetch private history.
A changed or missing expected generation refuses restore rather than quietly selecting another snapshot.

Proposed default: one MiB for each complete serialized plain checkpoint request and read response, including their envelopes.
The native protocol currently caps a message at two MiB (`af_wire/src/protocol/mod.rs::MESSAGE_BYTES_MAX`).
Before saving, validate both the request and prospective read-response sizes; oversized snapshots retain the previous generation without advancing a cursor.

| Proposed owner/file | New operation / requirement |
|---|---|
| New timestamped `runner_turn_checkpoints.sql` schema file | One current event-owned recovery row, generation and fenced writer witnesses; event deletion cleans it up. Bound size and retention before implementation. |
| `afd_fleet/src/lease/turn_checkpoint/sql/save.sql` | Lock existing scope/slot/lease in the established order; validate live ownership; compare expected generation; save history and fold membership atomically. |
| `afd_fleet/src/lease/turn_checkpoint/sql/load.sql` | Load only the current event/workspace/fleet snapshot and its compatibility witnesses. A second replacement can find an older valid generation. |
| `afd_fleet/src/lease/turn_checkpoint/sql/adopt_folds.sql` | Prove the replacement's live lease and exact snapshot; restamp only checkpoint-covered unsettled inputs belonging to this event/conversation. |
| `af_wire/src/protocol/{up/mod.rs,up/decode.rs,up/other.rs,down.rs}` | Correlated save/acknowledgement AND read/response variants within existing native frame limits; validate exact replay after a lost receipt. |
| `afd_gateway/src/session/dispatch/turn_checkpoint.rs` | Authenticate the request's current session and delegate to the durable owner; no history on core NATS. |
| `afr_supervisor/src/client/turn_checkpoint.rs` | Save bounded snapshots and read the descriptor's exact generation; verify receipts/responses under cancellation/deadline rules. |
| `afr_agent/src/engine/turn_recovery.rs` | Capture/restore full scrubbed message state; validate scope/model/setup and seed the root's incorporated cursor. |
| `afd_fleet/src/lease/answer/mod.rs` | Add resume metadata to `answer::Context` and `payload`; do not duplicate it into `fleet.sessions`. |

Proposed format version one rejects unknown versions and fields before restore.
The daemon seals plain-turn state with existing `af_crypto::{Sealer,Envelope}` and a new domain-specific `Aad::turn_recovery` constructor.
Associated data binds workspace, fleet, event, generation and format; ciphertext cannot move between those owners.

Inject the existing daemon key handle into `TurnCheckpoints` in `daemon/main/src/plane/mod.rs::lease_plane`; expose no key through the runner protocol.
Keep history in zeroizing buffers, scrub current secrets, and drop provider replay that cannot pass the current policy.
Present-but-corrupt or incompatible state stops recovery; only an absent checkpoint permits the original-input fallback.

Save identity is event plus expected generation and content digest.
The same live writer replaying identical bytes receives the same receipt; different bytes at the same expected generation fail without overwriting state.
The new table keeps one snapshot per unsettled event; event deletion cascades, and bounded maintenance removes snapshots of terminal events.

Add `turn_checkpoint/sql/cleanup_terminal.sql` and extend existing `daemon/main/src/sweepers/{checkpoints.rs,mod.rs}` wiring for that maintenance owner.
Use its own bounded pass without extending task storage locks across remote work.
No new timer service or checkpoint pointer is required for native tasks.

Checkpoint bytes and folded claims must describe the same input set.
Adoption changes ownership, while subsequent `Inbox::claim` supplies the unincorporated suffix.
A successful report settles both the restored prefix and newly incorporated suffix under the replacement fence.

Plain-chat history after filesystem tools cannot safely resume against an unrelated empty disk.
The workspace policy and capture barrier must be pinned before implementation.
The one-MiB default addresses transport size only; it does not resolve those consistency requirements.

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

The daemon needs its configured encryption key and session-code pepper; runners need configured checkpoint storage for the task-file variant.
Use existing credential setup; put no machine runner token into the human CLI session.

Use snapshot-capable sandboxes for a task-file recovery check; `--unsandboxed` cannot prove that property.
Have an operator account with `runner:read` and control of the exact runner containers used below.
Prepare those runners without invoking `make acceptance`; setup and a manual smoke are separate from automated lanes.

The task-file variant also requires durable coordination and an accepted native task; an ordinary chat prompt does not establish those conditions.

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
After build and focused unit checks, offer this smoke before starting recovery integration or end-to-end lanes.
After the human result, run required integration checks and the dedicated hard-kill test; acceptance follows its normal separate authorization and environment rules.

## Readiness and review disposition

Confirmed local changes cover capacity, quarantine, output fencing, current operator readers and the missing report/authoring writers.
Checkpoint work remains larger: plain-turn durable state, disk consistency and live whole-run capture must be fully specified before implementation starts.
M246's owner-facing commands and response mapping must be checked again when that dependency merges.

This document is a design and manual procedure, not a record that the proposed recovery test passed.
The spec's Discovery records Astra's full-spec findings and dispositions.

### Additional interface file changes from the focused review

| File(s) | Methods / types and proof |
|---|---|
| `rustd/crates/runner/afr_agent/src/engine/mod.rs`, `rustd/crates/runner/afr_supervisor/src/verbs/mod.rs` | Add `Folded::event_id`; `FencedVerbs::claim` preserves and validates wire identity instead of dropping it. `Inbox::claim` keeps its signature. |
| `rustd/crates/runner/afr_agent/src/loop/fold_tests/{mod.rs,fixture.rs}`, `rustd/crates/runner/afr_supervisor/src/{verbs/tests/folds.rs,lease_loop/drive.rs}` | Update direct folded-input literals and verify snapshot membership matches IDs and sequences actually incorporated. |
| `rustd/crates/protocol/af_wire/src/report/mod.rs` | `AcceptedHold`, `ReportResponse::hold`; define lease/fence/deadline identity and replay downgrade to no retained hold. |
| `rustd/crates/daemon/afd_fleet/src/lease/{sql/report_statement.rs,commit/mod.rs,report/finalize.rs}`, `rustd/crates/daemon/afd_gateway/src/session/dispatch/report.rs` | Carry authoritative hold result from SQL commit through settlement to the native report acknowledgement. |
| `rustd/crates/runner/afr_supervisor/src/{client/stream.rs,lease_loop/settle.rs,lease_loop/mod.rs,test_util/mod.rs}` | Preserve typed report responses; release the exact declined hold; update the response fixture without changing unrelated verbs. |
| `rustd/crates/runner/afr_supervisor/src/report/delivery/mod.rs` | Reuse its already-generic `until_acknowledged`; no method signature change is needed to return the typed response. |
| Proposed `rustd/crates/protocol/af_wire/src/lease/turn_checkpoint.rs` and native client/dispatch modules listed above | Add fenced read request/response and exact descriptor verification; test stale reads, wrong generation and complete-message size limits. |

## Build opening and Pull Request Session Notes

Indy authorized implementation using `orly-build` on Nov 17, 2026.
The isolated worktree is `/workspace/agentsfleet-m247`, branch `feat/m247-checkpoint-respawn`, based on `97224ed7432c9dd124ea8797d4324156b7022c07`.

**Implementer restatement:** Preserve one admission through runner failure; use expired leases and rising fences for takeover, automatic quarantine for host health, and committed checkpoints for recovery.
Start with independent capacity and keeper changes, then cordon and attempt-aware output; complete checkpoint design before Section 4 code.

**ASSUMPTIONS I'M MAKING:**

1. The start instruction authorizes the normal feature-branch commits, non-force push and Pull Request after the required gates.
2. Existing reviewed documentation changes belong to M247 and are carried into this isolated stream; the original checkout remains untouched.
3. M246 is a pending dependency; no tenant ownership surface or proxy behavior is claimed to exist.
4. Current recovery blockers remain in scope. Disk/history consistency and capture quiescence require a concrete design before checkpoint implementation.
5. Indy runs the controlled smoke before automated recovery integration, end-to-end or acceptance lanes.
6. Before a review that needs a different model, ask Indy to switch it manually and stop until confirmation.

Opening validation: `orly doctor` passed in the source checkout.
No implementation Dimension is marked done by the opening commit.
