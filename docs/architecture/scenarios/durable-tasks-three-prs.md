# Three Durable Tasks in one Linkwarden fleet

## Status and assumptions

This is M236's proposed review walkthrough, not a completed demonstration.
Task names, defects, pull requests and commit letters below are illustrative; no Linkwarden diagnosis or repair is claimed.
The repository's actual transfer size and working-tree size are unknown.

Indy uses Claude Code, Codex or the Codex app as the caller.
The command-line interface (CLI) uses Indy's authenticated user-scoped credential; a machine API key cannot impersonate a human approval.
The browser reaches the same workspace/fleet ownership checks.

The fleet has reviewed setup revision R7, current Linkwarden grants and explicit permission to repair assigned existing pull requests (PRs).
The example runner selects the Jev profile; another evaluated provider uses the same lifecycle and lease path.

Its goal requires the unchanged regression and required Continuous Integration (CI) checks at the current head.
Merge and deployment are excluded; restoring files never grants either permission.

| Durable Task (DT) | Assigned objective | Approved publication target |
|---|---|---|
| T1, Import repair | Fix a missing-title import crash while preserving normal fallback behavior. | Exact existing PR X and its same-repository branch. |
| T2, Search repair | Fix pagination duplicates in its assigned change without weakening tests. | Exact existing PR Y and a different branch. |
| T3, Export repair | Fix Unicode export handling while preserving the accepted format. | Exact existing PR Z and a different branch. |

Each task gets separate objective history, workspace state, effect identities, waits and cumulative limits.
Shared repository base bytes can be reused within the authorized scope.
One fleet runtime slot still permits only one active lease.

## Picture 1: caller messages, intent and three task identities

```text
Indy                  Caller                  agentsfleetd             Runner/Jev
 |                       |                         |                        |
 | "Repair PR X" ------>| POST F/messages         |                        |
 |                       |------------------------>| auth + save A1         |
 |                       |<------------------------| 202 accepted A1        |
 |                       |                         | claim fleet slot       |
 |                       |                         |---- lease L1 ---------->|
 |                       |                         |                        | Jev intent:
 |                       |                         |                        | new_work
 |                       |                         |<-- tasks/bindings ------|
 |                       |                         | validate target/grants |
 |                       |                         | commit T1, item I1     |
 |                       |                         |---- current T1 -------->|
 |                       |                         |                        | work on X
 |                       |                         |                        |
 | "Also repair PR Y    | POST F/messages         |                        |
 |  and PR Z" --------->|------------------------>| save A2                |
 |                       |                         |<-- tasks/inputs --------|
 |                       |                         |---- A2 ---------------->|
 |                       |                         |                        | Jev + bounded
 |                       |                         |                        | decomposition
 |                       |                         |<-- immutable plan ------|
 |                       |                         | items I2/I3 -> T2/T3   |
 |                       |                         | append ready work Q2/Q3|
 |<-- "Both queued" ----|<-- native receipt -------|                        |
 |                       |                         |                        |
 | "For import PR X,    | POST F/messages         |                        |
 |  keep its existing   |------------------------>| save A3                |
 |  fallback title" --->|                         |<-- tasks/inputs --------|
 |                       |                         |---- A3 ---------------->|
 |                       |                         |                        | Jev: steer T1
 |                       |                         |<-- input receipt -------|
 |                       |                         | save A3 meaning/cursor |
 |                       |                         | suppress repeat delivery
 |                       |                         |---- accepted steer ---->|
 |                       |                         |                        | next safe group
 |
F = /v1/workspaces/{workspace_id}/fleets/{fleet_id}
The user supplies real PR URLs, never internal T1/T2/T3 identifiers.
```

The input view reads human messages in source order even when queued machine work lies between them.
That lets A3 steer T1 while Q2 and Q3 wait; it does not promote T2 or T3 ahead of FIFO work order.
At most four messages enter one safe-boundary batch.

The conversational model proposes distinct bounded actions for A2.
The daemon assigns stable item identities and saves the immutable plan before creating tasks.
Replaying A2 returns T2/T3 rather than creating T4/T5.

A3 changes the repair method within T1's accepted objective, not the repository grant or goal revision.
The daemon saves its meaning before removing duplicate delivery eligibility.
If the worker crashes, accepted A3 remains available beyond the checkpoint's covered-input cursor.

For a steer to T2 while T1 runs, native mode is queue-only.
The T1 lease cannot write PR Y or attach T2's writable files.
An ambiguous `cancel that` asks which task rather than guessing from the newest sidebar row.

### Code and API path for picture 1

Paths below are relative to `rustd/crates/` unless prefixed with `cli/` or `ui/`.
The table separates current foundations from task-runtime wiring still to implement.

| Boundary | Current source/type | Remaining M236 integration |
|---|---|---|
| Caller admission | `afd_api_tenant/src/handler/fleet/message_steer/mod.rs`, `SteerRequest`, `SteerAccepted` | Preserve source admission and optional validated work selection; ordinary chat syntax stays unchanged. |
| Saved acceptance | `afd_admission/src/admission.rs`, `Admission`, `Producer`, `Reply`; `admit.rs` | Nullable task/item binding, `Producer::TaskContinuation`, durable single-use input claims. |
| Fleet serialization | `afd_fleet/src/lease/claim.rs`, `Leases`; `lease/pull/mod.rs`, `Plane` | Keep the fleet-slot fence and queue ownership; attach only the selected task context. |
| Agent entry | `afr_agent/src/engine/mod.rs`, `AgentRun`, `AgentEngine`; `loop/mod.rs`, `Loop` | Supply `TaskContext` and the safe input boundary through existing engine/lease verbs. |
| Decision call | `afr_providers/src/decision/`: trait `DecisionEvaluator`, configured `DecisionClient` and four native wire adapters | Call the client from agent policy in `afr_agent/src/decision/`, with native reservations and usage receipts. |
| Native task creation | `afd_fleet/src/task/mod.rs`: struct `Tasks`, trait `TaskLifecycle`, native binding and control | Task binding handler under `afd_api_runner/src/handler/runner/` and task-aware lease execution. |
| Shared wire | Existing `afd_wire` lease/report types | `afd_wire/src/task/`: `TaskContext`, `TaskBindingRequest`, `TaskBindingResponse`, `TaskInputReceipt`. |

`DecisionClient` uses the lease's frozen runner profile; `DecisionEvaluator` evaluates through that configured client.
[Decision models](../goal_evaluation.md#one-trait-inside-the-existing-provider-layer) owns its adapters and capabilities.

`Tasks` owns database mutations; no generic second scheduler or plugin-selected workflow engine is proposed.
Existing `LeaseVerbs` carries native task calls; do not add a competing task-runtime interface to agent tools.

Runner task APIs use prefix L: `/v1/runners/me/leases/{lease_id}`.
`POST L/tasks/bindings`, `GET L/tasks/inputs` and `POST L/tasks/input-receipts` bind A1/A2/A3 under current lease/fence and expected generations.
`POST L/tasks/decisions` reserves a physical decision call; `PATCH L/tasks/decisions/{call_id}` records its typed result, identity and usage.

## Picture 2: work, publication, save and fair switching

```text
Existing queue       Fleet slot / agent             Native publisher         R2 + Postgres
     |                        |                            |                       |
 T1 current ---------------->| read exact PR X/head A      |                       |
 T2 ready                    | reproduce -> patch -> test|                       |
 T3 ready                    | save compatible G1 ------------------------------>|
                             |                            |                 base/chunks,
                             |                            |                 history cursor
                             | publish E1 (one POST)     |                       |
                             |--------------------------->| intent E1 before HTTP |
                             |                            | append only if head A |
                             |                            |---- GitHub -> head B  |
                             |<-- acknowledgement lost ---| E1 outcome uncertain  |
                             |                            |                       |
 Caller steer A3 accepted -->| save accepted meaning beyond G1's input cursor    |
                             X worker/runner disappears   |                       |
                             |                            |                       |
 lease expires -> reclaim T1 |                            |                       |
             [picture 3 resumes this same work before queue switching]           |
                             |                            |                       |
 T1 recovered -> corrected head C -> checkpoint G2 -> wait for CI(C)               |
                             |---- settle wait/usage/report + release slot ------>|
                             |                            |                       |
 T2 ready ------------------>| fresh T2 context/workspace  |                       |
                             | repair PR Y -> head Y1     |                       |
                             | save -> WAIT -> release    |                       |
 T3 ready ------------------>| fresh T3 context/workspace  |                       |
                             | repair PR Z -> head Z1     |                       |
                             | save -> WAIT -> release    |                       |
                             |                            |                       |
 All three waiting           | no worker held            |                       |
```

G1 and the crash are deliberate negative-test points in this proposed walkthrough.
The demonstration must create those conditions through a controlled run; this document does not claim they happened.
The uncertain E1 is checked before any repeat publication.

T2 and T3 never inherit T1's checkout merely because their fleet/policy matches.
Warm attachment must match task, compatible generation and base; otherwise restore their own files or create a fresh checkout.
The read-only toolbox is shared by digest and never copied into each checkpoint.

A task yields after its ten-minute slice at a completed action boundary, saving before a safe handoff.
An existing remote action retains reconciliation ownership; fairness does not justify interrupting and repeating it blindly.
Waiting has no active agent lease, so T2/T3 can use the fleet slot.

### Code and API path for picture 2

| Boundary | Existing source/type | Proposed M236 addition |
|---|---|---|
| Sandbox lifecycle | `afr_sandbox/src/engine/mod.rs`, traits `Engine` and `Sandbox`; `afr_supervisor/src/lease_loop/` | Add task-aware attachment and recovery inside the joined build and `ExecutorSlot` readiness boundary. |
| Tool dispatch | `afr_agent/src/router/mod.rs`; `afr_executor` trait `Executor` | Sandbox calls await compatible workspace readiness; supervisor calls do not gain sandbox write access. |
| Repository preparation | `afr_supervisor/src/workspace_clone/{fetch,checkout}.rs` | Reuse verified base bytes; choose recovery or fresh checkout without overwriting restored edits. |
| Candidate publishing | Existing repair policy and GitHub credential broker | Planned publication module under `afd_fleet/src/task/`, `PublicationIntent`, `EffectReceipt`; fixed expected-head mutation and readback. |
| Capture and restore | Existing host workspace and context checkpoint boundaries | `afr_supervisor/src/recovery/`, struct `Recovery`, methods `capture` and `restore`; `TaskCheckpointManifest` wire data. |
| R2 transport | New dependency on inspected `object_store` 0.14.2 | Planned storage module under `afd_fleet/src/task/`, with `R2CheckpointStore` using the existing `object_store::ObjectStore` trait. |
| Encrypted format | `afd_crypto::entropy`, `envelope`, secret byte types | Planned checkpoint module under `afd_crypto/src/` adds domain-separated object encryption and authentication, preserving vault ciphertext compatibility. |
| Settlement | `afd_fleet/src/lease/{settle,finalize}.rs`, `lease/report/` | Commit usage, manifest pointer, wait, input responses and admission settlement before releasing the fleet slot. |

`POST L/tasks/effects` saves E1 and its immutable head/tree, commits issuance, then makes one bounded native publication within that request.
It returns the confirmed receipt or an explicit uncertainty failure; no separate client dispatch or background publishing queue is added.
`GET L/tasks/effects?ordinal_after=<watermark>&starting_after=<cursor>` discovers effects even when no worker received E1's first response.

The individual effect read returns confirmed, refused or uncertain status; it does not invent publication success.
Human advancement from A to another head rejects the prepared write against A.

T1's checkpoint reserves a verified base, changed chunks and a writer pin through `POST L/tasks/checkpoints`.
Its publication and restore follow the canonical [checkpoint APIs](../durable_tasks.md#proposed-native-api) and [storage protocol](../sandbox_evaluation.md#large-repositories-and-incremental-checkpoints).
The daemon's R2 credential and master key never enter T1's sandbox.

Repository size is discovered and checked against capacity; no six-GB assumption enters the clone, restore or manifest identity.
An initial retained base can be large; later checkpoints upload only new chunk bytes plus metadata.
Parallel independent encrypted chunks permit retries without re-uploading an entire checkout.

An unsettled kill does not authorize capture.
Failed upload or exhausted quota retains the prior recoverable generation and reports unsaved loss risk.
Reader/writer pins prevent collection from deleting an object between verification and manifest publication.

## Picture 3: recovery, three external triggers and human conversation

```text
Native state                         Replacement worker / fleet          Caller / GitHub
     |                                           |                             |
 T1 lease reclaimed ---------------------------->| fresh authority            |
     |---- compatible G1 ----------------------->| restore files/history      |
     |---- accepted A3 after G1 cursor --------->| replay steer meaning        |
 E1 unknown ------------------------------------>| read PR/head/operation      |
     |                                           | confirm B already exists   |
     |                                           | do not publish B again     |
     |                                           | honor A3 -> test -> head C |
     |<--- G2 + WAIT for CI(C), release ----------|                             |
     |                                           |                             |
 T2 then T3 run and wait, as picture 2             |                             |
     |                                           |                             |
     |<--------------------- CI(Y1) success webhook ---------------------------|
 native signature/install/head/wait match        |                             |
     |---- one T2 continuation ----------------->| fresh exact-head read      |
     |                                           | Jev + native proof -> DONE |
     |                                           |                             |
     |<--------------------- CI(C) failure webhook ----------------------------|
     |---- one T1 continuation ----------------->| fix same PR X within limits|
     |                                           | head D -> save -> WAIT     |
     |                                           |                             |
     |     CI(Z1) webhook missed                  |                             |
 due fallback -> exact native status read         |                             |
     |---- unchanged/pending -> retain wait       | no model / no restore      |
 later read sees success                         |                             |
     |---- one T3 continuation ----------------->| fresh proof -> DONE        |
     |                                           |                             |
     |<--------------------- "How is the import repair?" ----------------------|
 recorded T1 status ---------------------------->| brief status with freshness|
     |                                           | no new DT or diagnosis     |
     |<--------------------- "Stop the import repair" -------------------------|
 native stop generation advances                 | no new effects or late wake|
     |                                           | already-issued effects     |
     |                                           | still need reconciliation  |
```

This example intentionally ends T1 cancelled while T2/T3 finish.
A cancellation does not merge, deploy, delete a PR or undo a commit that GitHub already accepted.
It prevents new effects after the committed cutoff and reports any remaining uncertainty.

If Indy instead lets T1 continue, a later success at D still needs current-head checks before DONE.
The objective, accepted A3 and all spending/attempt limits remain the same task across those wakes.
No source message, fallback read or replacement worker resets its allowance.

### Code and API path for picture 3

| Trigger | Existing entry | Proposed M236 continuation |
|---|---|---|
| GitHub completion | `afd_api_ingress/src/handler/webhook/{app_route.rs,github/}`; installation and signature validation | Match task wait before ordinary investigation filters; atomically consume one generation and append one native continuation. |
| Missed event | Existing daemon supervisor/sweeper framework | Planned task maintenance under `agentsfleetd/src/sweepers/` calls `Tasks::check_due_waits`; bounded status reads enqueue only changed evidence. |
| Worker loss | Existing fleet-slot expiry, lease claim and report fences | Select compatible task history before coding inference; `Recovery::restore` reconstructs its private files and replays accepted input. |
| Human status/steer | Same `POST F/messages` path as picture 1 | Jev routing, native association and durable per-item receipt; status does not restart repair. |
| Explicit controls | New `PATCH F/tasks/{task_id}` | Existing workspace/FleetWrite guards, ETag/If-Match, idempotent lifecycle update and generation fence. |
| Progress | Existing `afr_agent::EventSink`, activity upload and event stream | Typed decision/transfer states and durable readback; CLI and browser render the same task identity. |

The fallback read obtains only repository-scoped read authority after checking current task/workspace/grants.
Its network response cannot advance a task whose wait was cancelled, consumed or replaced during the request.
Signed completions and changed fallback reads compete through the same guarded wait transition.

### Visible output and readback

```text
Checking your request (Jev)...
Import repair started. Search repair and Export repair queued.
Your import instruction was saved.
Restoring Import repair after worker replacement...
Checking whether the earlier publication succeeded...
Earlier commit confirmed; continuing without repeating it.
Saving changes: 18 / 40 MiB uploaded; base reused
Upload complete; verifying saved state...
Work saved. Waiting for checks.
Search repair completed: verified current-head checks passed.
Export repair completed: verified current-head checks passed.
Import repair cancelled. Existing published commits remain.
```

These lines illustrate proposed native states, not a transcript of a completed run.
Show unknown totals honestly; count acknowledged unique upload bytes and coalesce attached updates to at most once per second.
Only confirm saved work after manifest commit and completion after current-head proof.

The lifecycle and API owner is [Durable Tasks](../durable_tasks.md); this page remains the concrete worked example.
It does not define another implementation route or duplicate the provider policy.

## Readiness checklist for Indy

| Decision or proof | Review state |
|---|---|
| Who creates DTs and owns authority | Specified: runner interpretation, daemon validation and native records. |
| Multiple messages/tasks and steering across queued work | Specified: immutable items, human input view, queue-only other-task association, one active lease. |
| Decision-provider question bodies and uncertainty | Concrete example in [goal evaluation](../goal_evaluation.md#concrete-repair-decision-example); no live evaluation result claimed. |
| Large repository and R2 transfers | Incremental encrypted chunks and retained base; actual repository size unknown; capacity/throughput tests still required. |
| Crash/unknown effect/input replay | Defined negative boundaries; proposed integration tests must implement and prove them. |
| Progress in CLI/browser | Browser task reads, saved criteria and reconnect refresh are implemented; decision/transfer progress and CLI integration remain pending. |
| Astra review | Extra-high Astra review verified all four material findings corrected, with no material residual finding. Final dispositions live in M236 Discovery; runtime acceptance remains unrun. |
| Runtime implementation | Foundations are implemented; the [acceptance ledger](../../../playbooks/operations/acceptance/baselines/M236_001-autonomy.md#owner-work-item-status) records completed components and remaining work. |
| Live acceptance | Not run; requires authorized PRs, credentials, task-capable runner and controlled fixtures. |

## Why the proposal allows 16 unfinished tasks

Sixteen is a proposed safety cap, not an expected daily workload or a requirement to create sixteen workers.
The three-task walkthrough is a normal smaller example.
A busy maintenance fleet could retain the following independently assigned repair objectives across different PRs.

| Slot | Illustrative repair | Slot | Illustrative repair |
|---|---|---|---|
| 1 | Import missing titles | 9 | Export date formatting |
| 2 | Search pagination | 10 | Background retry behavior |
| 3 | Unicode export | 11 | Dependency update regression |
| 4 | Duplicate bookmark handling | 12 | Keyboard navigation regression |
| 5 | Tag rename behavior | 13 | Preview image handling |
| 6 | Stale archive preview | 14 | Timezone display |
| 7 | Session-expiry regression | 15 | Shared-link rendering |
| 8 | Share-permission regression | 16 | Resource cleanup |

These are hypothetical assigned jobs, not claims about real Linkwarden defects.
Most can be queued or waiting; exactly one fleet lease runs at a time.
The cap needs workload evidence before it can be called optimal, and storage/budget limits can constrain work sooner.
