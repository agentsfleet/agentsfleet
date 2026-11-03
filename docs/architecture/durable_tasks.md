# Durable Tasks

## Status and ownership

This page defines the proposed M236 Durable Task (DT) lifecycle.
Indy approved implementation on November 3, 2026; runtime acceptance remains pending.
[Goal evaluation](goal_evaluation.md) owns decision-provider questions; [sandbox evaluation](sandbox_evaluation.md#durable-task-checkpoints-in-r2) owns saved files.

The mechanisms below define the approved implementation target, including the concrete limits and protocol in the final sections.
The acceptance ledger distinguishes implemented components from pending runtime integration and live proof.

A fleet retains its instructions, grants, conversation, memory and schedules.
A DT names one accepted objective within that fleet.
A lease gives one runner worker temporary permission to advance work.

```text
Workspace
  Fleet: Linkwarden repair
    ordinary message -> admission -> lease -> answer
    DT A: repair PR X
      lease A1 -> wait for checks -> lease A2 -> done
    DT B: investigate incident Y
      lease B1 -> wait for approval -> lease B2

Fleet execution slot: [A1][B1][idle][A2]
Several unfinished DTs; at most one active lease per fleet.
```

Waiting releases compute and the fleet slot.
A wait is separate from its settled run admission, so it cannot block the queue head.
Distinct fleets remain the boundary for simultaneous runs.

An ordinary greeting, status question or bounded one-request answer needs no DT.
A requested objective needing ownership across attempts or external waits uses a DT.
Earlier manual repair evidence does not prove cross-event ownership or replacement-worker recovery.

## One runtime and documentation ownership

A DT adds long-lived objective state to the current admission/lease path; it is not another worker or scheduling engine.
Every attempt still uses existing lease claim, fencing, agent loop, report and settlement.
Task methods extend existing `LeaseVerbs`; an ordinary message can retain a null task binding.

```text
fleet -> existing admission -> existing lease -> existing agent -> existing report
            |                     |
       optional task_id      optional TaskContext
            +---------- one DT across several leases --------+
```

This page owns lifecycle, storage rows, scheduling and native APIs.
[Decision models](goal_evaluation.md) owns the provider trait/configuration; [three-task Linkwarden](scenarios/durable-tasks-three-prs.md) remains a worked example.
Do not merge the complete example into this page or duplicate its API definitions; keep diagrams linked to their canonical mechanisms.

### Simplification decisions

Keep the DT row, accepted-input history, publication intent and recovery manifest because each protects a named worker-loss or external-write boundary.
The publication ledger covers supported external writes, not every read, shell command or ephemeral tool call.
Use database generations and current lease fences rather than a second task lock service.

Remove separate publication dispatch, another task-message send route and provider-specific agent networking.
Keep one trait and one guarded client inside the current provider crate; use fixed wire adapters rather than a plugin engine.
Reuse chat, runner configuration, billing, supervision and outbound delivery instead of adding parallel services.

Storage pages, URL renewal and capture/restore pins remain necessary for bounded large-repository recovery.
Their operations are batched; one logical pin renews an object set and uploads use bounded parallel chunks.
The proposed limits and performance still need workload measurements before any claim that this is optimal.

## Setup and request interpretation

Setup chat or CLI authoring drafts `instructions_md` and the fleet owner's editable `decision_policy` through the existing private flow.
Saving setup establishes a fleet revision and capability ceiling; it does not create arbitrary future repair work.
An explicit initial-work request can enter admission after activation under the existing start rules.

Question and criterion edits create a new immutable revision through Review/Apply; older revisions remain unchanged.
New tasks use the accepted policy; active tasks require explicit adoption that invalidates earlier assessments without resetting history or budgets.

```text
Setup chat -> reviewed candidate -> Save/Apply -> fleet revision R7
                                                    |
Human: "Repair PR X until its required checks pass" --+
                        |
Daemon authenticates and saves message admission
                        |
Existing fleet slot -> runner lease -> decision-provider routing
                        |
conversation/status   refresh   steer   new work   unclear
       |                 |       |         |          |
     answer          fresh read  |    prepare goal   clarify
                                 |         |
                          daemon validates association,
                          current grants, target and limits
                                 |         |
                            input receipt  create DT
```

The runner calls its assigned decision provider under the current lease and its billing limits.
The conversational model prepares a bounded objective for new work; the decision result does not generate that objective.
The daemon alone creates a DT after checking current authority and typed target evidence.

A classification cannot grant repository access or choose an unrestricted branch.
Quoted content, tool results and model summaries cannot approve work.
Slack work approval also requires the verified person-to-workspace link described in [sandbox evaluation](sandbox_evaluation.md#workspace-approval-in-chat-or-slack).

Users address the fleet without DT identifiers.
The browser can attach a selected work item as a hint; native code validates that hint against current workspace ownership.
Ambiguous references ask which work; mixed greeting-plus-work retains the work request.

An admitted message survives routing failure.
A timeout, unsupported answer or uncertain association never defaults to new work or a write target.
Signed completion events match native waits directly and skip intent classification.

## State authority and transaction boundaries

Postgres owns DTs, input receipts, waits, effect intents and checkpoint manifests.
Dragonfly retains existing wake hints; `fleet.admissions` remains the work queue.
R2 holds encrypted immutable checkpoint bytes and does not decide current ownership.

Large repositories use a retained verified base plus incremental chunks, as defined in [checkpoint storage](sandbox_evaluation.md#large-repositories-and-incremental-checkpoints).

The proposed schema adds `fleet.tasks`, `fleet.task_inputs`, `fleet.task_waits`, `fleet.task_effects` and `fleet.task_checkpoints`.

The daemon binds admissions and leases to a nullable task identity; ordinary exchanges retain the existing path.
Schema numbering must be rechecked against main before implementation.

Each DT binds its workspace, fleet, accepted instruction revision, objective, approved target and cumulative limits.
One current generation guards changes; input sequence, checkpoint generation and lease fencing remain distinct counters.
Each remote effect has an immutable identity, request digest, expected target state and reconciliation outcome.

| Boundary | Guard and committed result |
|---|---|
| Create | Lock current admission, fleet and authority; save immutable interpreted work items; deduplicate admission plus item identity; insert DT and bind current run together. |
| Resume | Lock current DT and fleet slot; reject stopped, stale, foreign or exhausted work before issuing a bound lease. |
| Accept input | Save interpreted input and association in task history under current generation; claim queued delivery; retries return the same receipt. |
| Prepare effect | Check live lease, current grants, approved target and stop generation; save immutable effect intent before the remote request. |
| Save checkpoint | Hold publication pins; verify immutable uploads; publish a compatible manifest and convert live pins to references under current generations. |
| Enter wait | Commit report, usage, recovery pointer, wait and admission settlement before releasing the fleet slot. |
| Wake | Atomically consume one current wait and append its continuation admission; competing completions and fallback reads return the same result. |
| Finish | Recheck current target and mandatory evidence; save terminal outcome and existing outbound obligation in settlement. |
| Stop | Advance stop generation; reject new effect intents and wakes; reconcile remote requests already issued. |

Database, R2 and GitHub writes cannot share one transaction.
Intent-before-effect, immutable uploads and guarded manifests bridge those boundaries.
After acknowledgement loss, read native and remote state before issuing another effect.

States distinguish ready, running, waiting, blocked, paused and terminal work.
Done, cancelled and exhausted work cannot be revived by a greeting or late event.
Resolving a blocker retains the original deadline, usage and attempt counts.

Every claim checks the accepted instruction revision and current grant ceiling.
Applying new fleet instructions does not silently replace an active DT's objective.
Explicit reviewed adoption invalidates decisions made against the prior goal revision.

## Human input while work runs

The daemon records incoming messages before acknowledging them.
At eligible boundaries between complete action groups, the runner reads pending input through the current lease.
Decision routing and native consumption receipts associate each input without taking a second fleet lease.

Input admission and active-loop acceptance share one claim to prevent later queued delivery from repeating the steer.
Acceptance saves interpreted input in canonical task history before suppressing queued delivery.
Acceptance, checkpoint inclusion and confirmed resulting effects remain separate facts.

Resume loads accepted inputs beyond the checkpoint's covered-input cursor, even when those inputs already left the queue.
The runner reconciles recorded effects before acting again on replayed input.
The proposed receipt protocol and FIFO boundaries below define active-loop acceptance and recovery.

New work discovered during a running DT becomes queued work; it does not replace the current objective.
For mixed requests, save an immutable interpretation plan and stable native item identities before creating any DT.
Retry deduplicates each admission/item pair; changed interpretations cannot silently add another item.

Status and conversation can be answered at the next safe boundary.
Neither claims that an in-flight remote action was interrupted.

Pause and cancellation must fence new effects natively.
A remote request issued before the cutoff can still complete and requires reconciliation.
Fleet deletion fences access immediately; checkpoint cleanup follows the authorized deletion policy.

## Existing pull request publication

The initial existing-Pull-Request (PR) mode accepts an exact open same-repository PR and approved base.
It refuses fork heads, default/protected target branches and workflow edits.
Repository workflow review must establish validation-only push effects for the controlled demonstration.

Use a native publisher around GitHub `createCommitOnBranch` with `expectedHeadOid`.
The publisher owns credentials, branch, expected old head and allowed file changes; the model supplies a bounded candidate change.
An ordinary ref update with `force:false` does not provide this expected-head guarantee.

Before publication, reproduce the candidate tree from its base and supported file changes.
Compare that tree with the locally tested tree; unsupported modes, symlinks, submodules or size limits block publication until explicitly supported.
After publication, bind remote CI evidence to the returned commit and verified candidate tree.

Record validation expectations before the remote request.
Early completion events remain available while publication is reconciled and cannot disappear between push and wait registration.
Match installation, repository, PR, head, workflow identity, run attempt and current wait generation.

A human head advance rejects publication against the old head.
Preserve the candidate, fetch fresh state and reevaluate rather than overwrite human changes.
PR closure, merge, revoked grants and exhausted limits stop further publication.

After a timeout, reconcile parent, tree, branch history and the recorded operation marker.
`clientMutationId` is not assumed to provide server-side deduplication.
An unresolved outcome blocks blind retries; a webhook only requests fresh evidence reads.

## Work panel and notifications

Retain the current fleet-chat layout, message presentation, composer, header and activity indicators.
Add compact inline work summaries above the composer, with details expanding in place through existing design-system components.
This is the approved mock's placement on desktop and mobile; a fixed right sidebar is outside that visual scope.

Indy approved the separate setup and fleet-chat mocks, including the existing `Save done vN → Open fleet` pill and compact work panel.
Reuse the bundled Instrument Sans font and [design-system tokens](../../ui/packages/design-system/src/tokens.css): 14px conversation/composer text, 13px work labels and 12px secondary text.
Match the approved spacing, borders and native status colors, then verify font loading and appearance in desktop and mobile browsers.

Setup chat (`DraftConversation` and `ChatComposer`) authors fleet revisions.
Work requests and DT activity use `FleetThread` and `SteerComposer`; task details stay within that fleet conversation.
Saving setup records configuration; authorized requests or configured triggers start work.

```text
Fleet chat (existing layout)
-----------------------------------------------
Messages and confirmed progress...

Current work · 3
  PR 123   Waiting for CI          [expand]
  PR 124   In progress             [expand]
  PR 125   Queued                  [expand]
    expanded item: PR link, files, checks, controls

Message your fleet...
```

Default rows show title, state, next dependency and confirmed result links.
Attempts, subagents and tool evidence stay expandable; subagents are helpers within work, not DTs themselves.
Local saved edits and published changes have different labels.

Browser reconnection reads current work before offering another attempt.
Waiting and recovery retain confirmed progress without repeating the entire incident report.
Status answers label saved evidence freshness.

Reuse existing outbound obligations, trusted destinations, Slack answer markers and repeat-delivery checks.
DT-to-notification correlation belongs in task records by default; `afd_outbound` needs no speculative rewrite.
Any demonstrated transport gap requires a named spec amendment before source edits.

## Further uses and boundaries

The same lifecycle can own dependency remediation, support-to-release follow-up, release verification and checkpointed data jobs.
Each task type needs its own completion evidence and authority ceiling.
Schedules decide when to observe; a DT owns an accepted objective until its outcome is settled.

Conditional fleet handoffs are follow-up work after M236.
The caller steer API accepts authorized target fleets; native message and schedule tools remain scoped to their existing thread or fleet.
The follow-up needs verified conditions, explicit fleet permissions, durable admission/correlation, retry deduplication and bounded chains through the existing runtime.

Product references include [Puffle](https://puffle.ai), [Grok Bot](https://x.ai/bot) and [Turnstone](https://myturnstone.ai/comparisons).
Their product descriptions suggest use cases; they do not establish internal durability guarantees.

## Proposed relational model for signoff

Use UUIDv7 native row identities and application-owned state constants, following the current schema conventions.
All task-owned rows carry workspace and fleet ownership; composite foreign keys reject cross-workspace links.
Store timestamps as `TIMESTAMPTZ(3)` and money as nonnegative integer nanodollars with overflow checks.

| Table or addition | Required fields and uniqueness |
|---|---|
| `fleet.tasks` | Identity/ownership, originating item, accepted revision/objective digest, target document, state, resource version, stop generation, deadline, limits, cumulative usage, input/effect cursors, recovery pointer, trusted reply destination. Unique originating item. |
| `fleet.task_items` | Source admission, native item identity/ordinal, immutable interpretation digest, action/association, source text spans, resulting task binding. Unique admission/ordinal; conflicting replay returns 409. |
| `fleet.task_inputs` | Task/item/sequence, accepted text and interpretation, accepting lease/fence, answer state, applied-effect references. Unique task/item and task/sequence. |
| `fleet.task_waits` | Task/generation, dependency kind, expected head/check identity/run attempt, due time, expiry, consumed admission. One current wait per task through a partial unique index. |
| `fleet.task_effects` | Task/item, native effect ordinal/identity, expected old state, immutable request/tree digests, prepared/issued/confirmed/unknown/refused state, remote receipt. Unique task/ordinal and operation identity. |
| `fleet.task_decisions` | Physical call identity, lease/task binding, requested/resolved model, question revision, assessed state/input cursor, reservation, usage and native disposition. Unique physical call identity. |
| `fleet.task_checkpoints` | Task/checkpoint generation, plan state/mode/owner/expiry, declared budgets/page count, history digest, covered-input cursor, Git/base identity, complete manifest digest, format/toolbox compatibility, effect watermark. Unique task/checkpoint generation. Only a ready incremental manifest can become the task recovery pointer. |
| `fleet.task_checkpoint_pages` | Native UUIDv7 page identity, checkpoint owner, page number, digest and complete bounded page bytes. Unique checkpoint/page number; immutable replacement conflicts. |
| `fleet.task_bases` | Workspace/repository identity, exact Git commit/refs, immutable base manifest, unique/logical bytes and current custody state. Unique scope/base digest. |
| `fleet.task_objects` | Scope, random object identity, identity-key version/content identity, private plaintext digest, size, fixed nonce, immutable ciphertext digest/length, format, wrapped key envelope, upload owner and timestamps. Unique live scoped content identity; tombstones retain distinct historical identities. |
| `fleet.task_identity_keys` | Workspace/repository, native key version, wrapped 256-bit content-identity key and active/retired state. Stable across master-key rewrapping; never sent to the sandbox or ordinary runner. |
| `fleet.task_object_refs` | Checkpoint/object or retained-base/object edges; unique owner/object. Protect all reachable objects, including previous recoverable generations. |
| `fleet.task_object_pins` | Object/capture-or-restore pin identity, protecting task, current generation and renewable expiry. Pin acquisition locks the same object row as tombstoning. |
| Existing admissions and leases | Nullable task/item identities; input claim lease/fence on admissions; capability-selected task context on leases. Existing one-shot rows retain null bindings. |

Task targets include repository installation identity, PR identity, head repository/ref, approved base and last independently verified head.
Target changes require an authorized new objective or explicit reviewed adoption; model prose cannot mutate the binding.
Private objective/input/evidence content never enters analytics or generic error bodies.

Object reference rows have either a checkpoint foreign key or base foreign key, with exactly one non-null owner.
Deleting a task removes its references after retention rules permit; shared bases/chunks survive while another retained owner references them.

Workspace deletion records durable storage-purge receipts before deleting ownership rows.
Receipts contain only random object identities, bucket references and wrapped-key deletion state, and survive owner-row removal until cleanup completes.
Use `fleet.task_storage_purges` for those independent cleanup receipts; deleting ownership cannot strand an untracked private object.

Task mutations lock the fleet parent against deletion, then its slot, admissions by sequence, lease, tasks by identity and subordinate rows by identity.
Checkpoint commit locks task before objects; collection locks candidate tasks before objects in the same order.
Network calls, encryption, scanning and object transfer happen outside database transactions.

Collectors never acquire a task lock after locking an object; orphan-only passes need no task lock.
Object deletion runs only after a committed tombstone and rechecks pins/references before the remote delete.

## Proposed input claims and scheduling

At a safe boundary, read at most four pending human admissions for the current conversation, ordered by source admission sequence.
Machine work rows do not block this input view; reading a message does not promote another task ahead of the work queue.

Stop at a prior live input claim; classify ambiguous messages into a clarification receipt rather than leaving a permanent input barrier.
Use the source message's recorded reply destination; an interpretation cannot invent a Slack thread.

The configured decision provider classifies each complete message against current native DT summaries.
The conversational model can propose up to four action items with source spans; the daemon assigns stable item identities once.
Save the complete immutable interpretation plan before any item changes task state.

An unbound run can bind its first new work item as current; other new items enter the queue tail.
An already-bound run keeps its current DT; all unrelated new work enters the queue tail.

Claim an input admission under the current slot fence without marking its effects applied.
Commit accepted text, typed interpretation and per-item DT association before removing duplicate queued delivery eligibility.
Different text, target or action under an existing plan identity returns a conflict.

An input for the current DT can affect its next action group after acceptance.
An input for another same-fleet DT is saved as queued input and cannot grant the current lease that other task's write authority.
Its first steer consumes an old wait or coalesces with an existing ready admission; duplicate completion events cannot add a second continuation.

Each accepted input gets its own native event/history row and reply obligation through the existing settlement helpers.
Report settlement includes each item's response or queued-work receipt and the current task's new covered-input cursor.
An accepted but unanswered item remains recoverable through native history after worker loss.

New work gets a fresh continuation admission at the queue tail; it never replaces the current task binding.
The original message receives a queued-work receipt; its item identity deduplicates the continuation.
Add `Producer::TaskContinuation`, keyed by task, generation and wake/item identity, to the existing ledger.

Reuse `EventType::Continuation` with a typed task payload, `Producer::TaskContinuation` and machine actor `task:<task_id>` for native continuations.
Preserve the originating person's identity separately; do not label a scheduler or webhook wake as a new human command.
Native control-plane commits, rather than model tool calls, create continuation admissions.

Claimed source admissions remain excluded from ordinary leasing until their native input outcome settles or recovery reclaims the claim.
After a claim owner expires, accepted items resume from task history; unaccepted items return to FIFO delivery without a fabricated applied receipt.

FIFO order remains authoritative at task yield boundaries.
After ten minutes of running time, a DT yields at the next completed action group; renewals reject further paid actions past its slice ceiling.
An already-issued tool can finish within its existing timeout; do not kill and blindly repeat an uncertain effect to improve fairness.

Checkpoint-capable tasks save before yielding; failed capture keeps the prior recoverable generation and reports the blocked handoff.
Do not silently discard unsaved work to serve another queued task.
The task runtime/deadline policy cannot override a smaller existing lease, credit or fleet limit.

## Proposed native API

Runner routes require `Guard::RunnerBearer`, the authenticated owning runner, live lease/fence and current task generation.
Person routes require the existing workspace guard, child ownership lookup and `FleetRead` or `FleetWrite` as shown.
Return 404 for wrong-workspace children and 409 for stale generations without exposing foreign state.

Use `/v1/runners/me/leases/{lease_id}` as prefix L and `/v1/workspaces/{workspace_id}/fleets/{fleet_id}` as prefix F.

Existing caller/configuration endpoints are reused, with additive validated fields:

| Existing endpoint | M236 addition |
|---|---|
| `POST F/messages` | Optional native task association for a selected UI row; ordinary fleet text and `operation_id` behavior stay supported. |
| `PATCH /v1/fleets/runners/{runner_id}` | Optional `assigned_policy.decision_model_id`, restricted by existing operator authorization and catalogue/vault ownership. |
| Existing runner heartbeat, lease claim, renew and report URLs | Assignment delivery, frozen decision/task context, usage and task settlement; no competing task claim route. |

The following proposed endpoints extend those existing resource families:


| Method and path | Request and successful response |
|---|---|
| `POST L/tasks/bindings` | Source admission, expected fleet revision, plan digest, four-or-fewer typed action items and source spans; 201 returns immutable native item/task bindings, replay 200. |
| `GET L/tasks/inputs?starting_after=<cursor>&limit=4` | Returns eligible human messages in sequence and current task summaries, scoped to the source conversation. Machine work rows do not obstruct steering. Does not consume them. |
| `POST L/tasks/input-receipts` | Source admission/plan/item identities, expected task generation, interpreted input and association mode; 201 accepts history plus claim, replay 200. Native mode applies here or queues for another task. |
| `POST L/tasks/decisions` | Reserve a physical decision call with purpose, request/question digests and maximum cost; 201 returns call identity, resolved rate/version policy and frozen decision profile and scoped provider credential material. |
| `PATCH L/tasks/decisions/{call_id}` | Bounded typed answers, actual resolved model and usage or explicit unknown outcome; 200 returns native disposition and settled/reserved usage. |
| `POST L/tasks/effects` | Immutable source item/cursor, operation identity, expected head, tested tree and supported changes; reserve and issue one bounded native publication synchronously. Confirmed creation returns 201; confirmed replay 200; unknown outcome returns 409 without retrying the mutation. |
| `GET L/tasks/effects?ordinal_after=<watermark>&starting_after=<cursor>&limit=50` | 200 returns the selected task's ordered effects after its checkpoint watermark, including unresolved and newly confirmed effects. Replacement workers need no previously returned effect identity. |
| `GET L/tasks/effects/{effect_id}` | 200 returns prepared/issued/confirmed/unknown/refused state and attributable remote receipt; unknown results require reconciliation. |
| `POST L/tasks/checkpoints` | Expected generation, operation identity, retain_base/incremental mode, exact repository/base, declared byte budget/page count (maximum 64) and format; 201 returns the plan, expiry, writer pin, base/checkpoint identity and native UUIDv7 page identities. |
| `PUT L/tasks/checkpoints/{checkpoint_id}/pages/{page_id}` | Complete page representation with page number, digest and contents; identical replay succeeds, changed page or number conflicts. Final root requires every declared page in order. |
| `GET L/tasks/checkpoints/{checkpoint_id}/pages/{page_id}` | 200 returns one bounded manifest page only under the current restore pin and live lease; verify its digest and ordered manifest root before using file/chunk mappings. |
| `POST L/tasks/checkpoints/{checkpoint_id}/objects` | At most 256 plaintext SHA-256/size/kind descriptors; 201 returns missing/reused/busy objects. The native upload owner receives the fixed nonce and temporary data key for new objects. |
| `POST L/tasks/checkpoints/{checkpoint_id}/uploads` | Registered object identities plus immutable ciphertext digest/length; 201 freezes descriptors and returns exact create-only PUT authorizations. It cannot change a prior descriptor. |
| `POST L/tasks/checkpoints/{checkpoint_id}/authorizations` | Read purpose without object IDs acquires/adopts one restore pin and returns root plus at most 64 ordered page IDs. Registered object batches then obtain/refresh five-minute read/upload URLs after scope/descriptor/pin checks; no arbitrary keys or paths. |
| `PATCH L/tasks/checkpoints/{checkpoint_id}/pins/{pin_id}` | Expected pin/task version; 200 renews the native reader/writer pin. One capture/restore pin protects its object set; no per-object renewal request or extension of task/transfer deadlines. |
| `PATCH L/tasks/checkpoints/{checkpoint_id}` | Ready state, expected generation, completed manifest root, covered-input cursor and trusted upload attestations; 200 publishes after native guards and object verification. |
| `GET L/tasks/checkpoints/{checkpoint_id}/objects?starting_after=<cursor>` | 200 returns at most 100 authorized object reads/keys; every page rechecks ownership, pins and current lease. |
| `GET F/tasks?starting_after=<cursor>&limit=20` | `FleetRead`; 200 returns titles, states, version, next dependency and confirmed result links. Maximum limit 100. |
| `GET F/tasks/{task_id}` | `FleetRead`; 200 returns progress, accepted questions/criteria and setup revision, assessments/evidence and ETag, with local versus published file labels. |
| `PATCH F/tasks/{task_id}` | `FleetWrite` and `If-Match`; lifecycle control or explicit reviewed `goal_revision_id` adoption with expected generation/operation identity; 200 returns state/version, ETag and any in-flight effect warning. Adoption invalidates old assessments without resetting usage or expanding authority. |

All runner mutation bodies include fencing token, task identity when already bound, expected generation and operation identity.
Bound control requests to 128 KiB and checkpoint manifest pages to 1 MiB.
Publication permits at most 32 changed regular-file paths, 1 MiB total decoded content and a 2-MiB encoded request.

Only supported changed-path modes are restricted; unchanged base symlinks or modes do not themselves prohibit a source repair.
Validate bounds with `garde`; never deserialize an unproved payload into a publishing operation.

Lease task context adds accepted goal/revision, native target, input/effect cursors, limits, compatible checkpoint reference and permitted decision policy.
Report task settlement adds expected generation, transition, evidence references, checkpoint identity, input responses and confirmed-effect watermark.
Decision usage carries the native `decision_call_id`; reserve/result/renew/report replay shares one billing line with that call's authorized protocol/profile/rates.

Ordinary leases omit those additions; task-bound leases require a `durable_tasks_v1` runner capability.

Generation changes can refuse a requested task transition without dropping the lease's measured usage or confirmed tool outcomes.
Settlement records spent work and stopped/stale disposition, then reconciles current native state instead of asserting DONE from an old report.

Unknown capabilities refuse task work; older runners may still receive ordinary one-shot admissions.
Every task API has served-route ownership, stale-fence, replay, malformed-body and stopped-task tests.
Generate OpenAPI from wire/source definitions after route inventory and route-count tests are updated.

Use new public error family `UZ-DT`: 001 not found, 002 stale state, 003 replay conflict, 004 invalid target and 005 exhausted limits.
Use 006 ambiguous input, 007 decision unavailable, 008 incompatible checkpoint, 009 storage unavailable and 010 publication uncertain.
Use 011 busy control boundary and 012 protocol incompatible; register codes and problem types only with their implementation.

Map missing children to 404, stale/replay/target/checkpoint/protocol conflicts to 409, limits to 429 and unavailable decision/storage to 503.
Ambiguous input is a normal clarification response; code 006 is reserved for malformed explicit association requests with HTTP 409.
Publication uncertainty returns 409 with code 010 and a recorded unknown effect; a later authorized read can report its reconciliation result.

### What the runner uses each collection for

These APIs connect the trusted runner supervisor to the daemon during an existing lease.
The daemon owns accepted task state; the coding model proposes work through the agent loop.
The user continues messaging the fleet without typing task identities.

| Runner collection under L | Responsibility and example |
|---|---|
| `tasks/bindings` | Associate an admitted request with new or existing DTs. One message assigning PR X creates its task; independent PR Y work queues another. |
| `tasks/inputs` | Read eligible human messages at a safe boundary, such as preserving the existing title fallback. Reading does not claim that instruction was applied. |
| `tasks/input-receipts` | Save the accepted message, interpretation and task association once. Recovery can replay its meaning without applying the same effects twice. |
| `tasks/decisions` | Reserve and record physical decision-provider calls, including typed answers, model identity, usage and native disposition. |
| `tasks/effects` | Record and perform supported external writes, such as appending a tested repair to the approved PR. Lost acknowledgements require readback before another mutation. |
| `tasks/checkpoints` | Save and restore compatible task files, Git state and conversation coverage across worker replacement. Live process memory is excluded. |

### Existing mechanisms to reuse

Reuse current runner authentication, lease fencing and vault/credential resolution for storage requests.
`afd_fleet/src/lease/fence/` already proves the live holder; `afd_credential` already scopes credential requests to native workspace identity.
Storage authorization applies those checks before issuing exact-object transfer URLs; it is not another permission service.

Local toolbox retention already protects mounted images through `Arc` ownership in `afr_sandbox/src/toolbox/holds/`.
Those local references cannot protect R2 objects from a collector on another daemon or survive host replacement.
M236 adds durable object references and transfer pins for that storage boundary.

The existing snapshot architecture already describes presigned URLs as a target, explicitly marked unbuilt in this checkout.
Reuse the selected storage library's signing support; implement only its native ownership, bounds and recovery integration.

### Rust abstractions before implementation

Indy requested this boundary review before new DT abstractions are written.
Structs own data and dependencies; behavior lives on the narrow traits below.
Wire records remain validated data rather than gaining a trait for each field.

| Owner and planned files | Data structs and behavior |
|---|---|
| `afd_fleet/src/task/{mod,admit,input,wait,effect,limits}.rs` | `Tasks` implements `TaskLifecycle`: bind work, accept input, settle, wait/wake, publish/reconcile and control state. Reuse native admission staging and lease fences with real database tests. |
| `afr_providers/src/decision/{mod,client,wire,types}.rs` | `DecisionClient` implements `DecisionEvaluator::evaluate`, returning typed `DecisionResult` and measured usage through four native wire adapters. |
| `afr_supervisor/src/recovery/{mod,capture,restore,manifest}.rs` | `Recovery` implements `WorkspaceRecovery::capture` and `restore`, handling writer quiescence, verified manifests and private workspace reconstruction. |
| `afd_fleet/src/task/storage.rs` | `R2CheckpointStore` holds scoped storage configuration and existing SDK clients. Task checkpoint behavior stays under `TaskLifecycle`; use existing `object_store::ObjectStore` and signing support. |
| `afd_wire/src/task/`, `afd_wire/src/decision/` | `TaskContext`, `TaskInputReceipt`, `PublicationIntent`, `TaskCheckpointManifest`, `DecisionProfile` and `TransferProgress` carry bounded typed data. |

Use trait objects at existing runtime composition boundaries, following `AgentRun` and its `Executor`, `LeaseVerbs`, `EventSink` and `Clock` dependencies.
An `Arc<dyn DecisionEvaluator>` can share the configured client; the loop borrows it through the current run context.
Object-safe async methods use existing boxed-future conventions and each owning crate's `Result`.

Concrete native task services use static dispatch unless a caller actually needs runtime substitution.
Use generics such as `R: Read` and `W: Write` for streaming chunk transforms with different concrete readers/writers.
Reuse current checked identifiers, fences and clocks; add newtypes only where different generations, cursors or digests must remain distinct.

A local `FnMut(TransferProgress)` can coalesce progress in one coordinator before emitting existing activity frames.
Use `Fn` or `FnOnce` only for a concrete predicate or one-shot local callback; durable mutations remain named trait operations.
Do not duplicate the existing storage SDK, database transaction layer, credential broker or metrics registry behind another generic interface.

Extend the owning crate's private `ErrorKind`, existing `error_shell!` error and `error_lifts!` conversions.
Preserve causal sources with `?`; contextual mapping adds native identifiers or operation names without exposing secrets or private provider bodies.
Keep native HTTP refusal and wire validation types consistent with their existing crate boundaries.

### Observability aligned to native outcomes

Reuse `afd_observability` producers, typed metric families, closed label sets, tracing and `docs/metrics.census.tsv`.
Instrument asynchronous futures rather than holding an entered span across an await.
Task rows and canonical billing lines remain the source of truth; emit signals from validated native outcomes.

| Operator question | Recorded signals and intended measurement |
|---|---|
| Is work advancing? | `dt_transition`; counts by bounded state/reason and current-state gauges through existing census machinery. |
| Are steers accepted and waits waking? | `dt_input_accepted`, `dt_wake_claimed`; accepted/replayed/stale outcomes and due-wake delay. |
| What do decisions cost? | `dt_decision_recorded`; provider/purpose/outcome counts and latency. Existing billing records hold per-model tokens, cache categories and cost exactly once. |
| Are writes confirmed? | `dt_effect_uncertain` plus native effect outcomes; uncertain/reconciled/refused/confirmed state and reconciliation latency. |
| How expensive is save/restore? | `dt_checkpoint_published`; capture/upload/verify/restore duration, unique uploaded/reused/readback bytes and quota/integrity failures. |
| What does the person see? | `task_decision_started`, `task_decision_completed`, `task_transfer_progress`; current provider, stage and acknowledged progress through existing CLI/browser activity. |
| Is work stranded? | Ready backlog/oldest age, overdue wait age and maintenance freshness distinguish queue starvation, expired ownership and delayed dependency reads. |

Logs and traces carry approved task, admission, lease, decision-call, effect and checkpoint identities for correlation.
Metrics use bounded enums; never label a series with those identities, private paths, URLs, prompts or arbitrary model output.
Signal tests verify replay accounting, stage ordering, asynchronous context and sensitive-field exclusion alongside each relevant Dimension.

### Failure proof and recovery visibility

Each scenario checks accepted state, the useful outcome, forbidden side effects, retained recovery and matching diagnostic signals.
Use independent expected values at production owners; shared fixtures own setup and cleanup, while scenario modules own distinct failure boundaries.
OpenClaw's [test audit](https://github.com/openclaw/openclaw/blob/68f6775adfe02ab5577ae75133c0834e81476ad6/.agents/skills/test-audit/SKILL.md) informs test value; repository gates still govern validation.

| Boundary | Fault and required observation |
|---|---|
| Decision call | No response, connection reset, explicit provider failure, oversized response and late completion cannot imply success or release unknown spend twice. |
| Remote publication | A committed write with a lost response remains uncertain until readback; replacement cannot send the same mutation blindly. |
| Checkpoint capture | Disk-full or partial writer failure keeps the previous ready manifest; staged bytes never become a saved generation. |
| Checkpoint transfer | Timeout, failed PUT, interrupted upload and missing object prevent manifest commit; retries preserve acknowledged-byte accounting. |
| Restore | Missing/corrupt bytes, disk-full and memory-limit death leave no attached writable partial workspace; the prior checkpoint remains recoverable. |
| Worker death | An isolated memory-limited child or existing kernel fixture supplies the real exit signal; recovery retains task identity and cumulative allowances. |
| Scheduling | Queued work behind another DT, expired leases, duplicate wakes and overdue waits either advance once or show an attributable blocked reason. |
| Diagnostics | Closed outcome/failure labels, duration/age signals and task/lease correlation identify failure without exposing goals, credentials or private paths. |

Prefer channels and barriers over shared mutable test state; bound every wait and join, release owned resources on failure and abort owned children.
Use deterministic failing I/O for routine disk-full handling, plus an existing bounded filesystem fixture when kernel behavior needs proof.

Never exhaust the development host's memory or disk to simulate a workload failure.
Kernel-only proof remains explicitly separate when its required host capability is unavailable.

### Durable performance investigations

A later DT can investigate an `agentsfleet` performance regression through bounded Grafana reads and authorized repository work.
Keep this evidence available while implementing M236; a duration total alone cannot explain where work slowed.

| Investigation question | Evidence to retain and compare |
|---|---|
| Was admission slow? | Admission-to-lease and first-output latency, ready backlog/oldest age, lease expiry/reclaim and eligible runner capacity. |
| Was useful work slow? | Active work time separately from queued, externally waiting, blocked, paused and restoring time; completed action groups and interruption reasons. |
| Were models slow or expensive? | Coding versus decision protocol, reviewed model/profile and price revision, physical request count, latency, token/cache categories, refusal, timeout and unknown spend. |
| Were tools or the database slow? | Existing tool spans and database/pool signals, bounded operation latency, contention/refusal and maintenance lag; no duplicate duration for nested spans. |
| Did repository recovery dominate? | Exact base/head, initial checkout duration/bytes, cold versus warm base, scanned/logical bytes, unique acknowledged upload/readback bytes and reuse. |
| Did infrastructure differ? | Runner/build revision, sandbox/toolbox format, resource limits and memory/disk/transport failures, carried as bounded native evidence or trace attributes. |
| Is a comparison valid? | Baseline/candidate revisions, workload/query/window, runner configuration, model/profile, cache state, repetitions, comparable sample counts and missing-data flags. |

Task/admission/lease, physical-call, effect and checkpoint identities join logs/traces to native records; these identities never become metric labels.
Metrics carry closed operation, stage and outcome labels; add each implemented family to `docs/metrics.census.tsv` with its actual producer and cardinality bound.
Reuse existing exporters, tool/model instrumentation, lease delivery metrics and Grafana data sources before introducing another measurement.

Publish queue/wait gauges from native maintenance snapshots; failed sampling withdraws the reading rather than publishing a false zero.
Record completed stage durations once at their owner, with monotonic elapsed time and correlated failure outcomes.

For example, Indy can ask: `Investigate why PR repair slowed between baseline B and candidate C; identify the dominant stage and propose a measured fix`.
The DT records the Grafana queries/windows and comparable baseline/candidate evidence, then requests clarification when workload or cache differences prevent a defensible comparison.

An authorized repair follows the same exact-head publication and verification path; diagnosing a regression alone does not authorize merge or deployment.
Distinguish observed evidence, estimates and unavailable signals; missing telemetry cannot support a claim that performance improved.

### Workspace checkout and recovery terms

A fresh checkout supplies the initial repository at its verified base.
Task restoration reconstructs that task's saved files, local Git state and conversation coverage before coding continues.
The toolbox remains a separately shared read-only image.

| Storage term | Meaning |
|---|---|
| Manifest page | One bounded part of the saved-state index: paths, modes, deletions, chunk mappings and Git/history coverage. It is metadata, not another task or running agent. |
| Reusable object | An immutable encrypted chunk already verified within the authorized workspace/repository scope. Unchanged content can be referenced again instead of uploaded again. |
| Authorization | Short-lived permission to transfer exact registered objects. The daemon retains bucket credentials and the master key; the trusted supervisor receives scoped transfer material. |
| Pin | A temporary native in-use record protecting an object set from collection during capture or restore. Once a save commits, retained manifest references provide durable protection. |

```text
First run:      verified repository base -> private checkout -> edits
Save:           manifest + newly changed encrypted chunks -> R2
Replacement:    authorize + pin -> read manifest pages
                                 -> reuse/fetch base and chunks
                                 -> verify -> restore private workspace
                                 -> continue the same DT under a new lease
```

The lease authorizes the current worker; a storage pin protects the bytes that worker is transferring.
A pin does not permit coding, extend the task budget or grant repository write authority.

### URL conventions and replay

Follow the repository's [HTTP rules](../../.orly/standards/domains/http.md): versioned plural resources, explicit owning scope and typed identifiers.
`tasks` is the plural lease-scoped namespace; its `bindings`, `input-receipts` and `decisions` collections name protocol records.
The namespace does not create another task processor or another lease claim path.

Runner URLs are lease-authorized views of the bound DT, including earlier effects needed during recovery.

The path supplies `lease_id`, `task_id` or an effect identity only once; do not repeat those path identifiers in request bodies.
Internal runner mutations use their declared operation identity and native replay guards.

List responses use `starting_after` and `{items, total, next_cursor}`; `ordinal_after` is an independent semantic watermark filter.
Person chat uses existing `F/messages` and its body `operation_id`; do not add a parallel task-message send endpoint.

Use `PATCH F/tasks/{task_id}` for paused, ready or cancelled state under current version checks.
Detail reads return ETag; PATCH requires `If-Match`, with 428 for absence and 412 for a stale tag.
Native lifecycle, lease-fence and generation conflicts remain 409.

A selected task is an optional native message association in the existing chat path.
Public side-effecting POSTs honor the existing `Idempotency-Key` rule unless the message-send rule expressly applies.

### Recoverable effect and checkpoint operations

The daemon assigns a monotonically increasing effect ordinal under the task lock before any remote publication.
Its stable operation identity derives from the saved source item and offered native effect cursor, not an uncheckpointed tool-call counter.
Save the canonical request and expected head/tree with that reservation; changed replay under the same identity returns 409.

Recovery lists all effects beyond the checkpoint watermark before requesting another publication.
Only one prepared/issued/unknown publication may be outstanding per task; a new publication first requires that effect to be reconciled.
Loss of the first response therefore cannot hide an already-issued write from its replacement worker.

Never advance a checkpoint's confirmed-effect watermark past an unresolved effect.

The same POST checks authority and stop generation before committing issued state, then performs one bounded physical send outside the transaction.
The issuance commit is the cancellation cutoff; a previously issued request can still complete.
Disable automatic mutation retries, keep lease renewal independent and bound the send to ten seconds and remaining task/lease allowance.

Save confirmed, refused or unknown outcome even if stop or lease expiry occurs during the request.
A repeated confirmed request returns its receipt; an unknown result blocks another send until authoritative reconciliation.
Reconciliation may read remote evidence through supervised maintenance; it cannot autonomously dispatch a queued write.

This follows the existing synchronous authoring model-call pattern in `afd_authoring/src/model/dispatch/`.
No client `/dispatch` request, background publisher queue or generic operations service is proposed.
The POST finishes with a confirmed result or named failure; it does not return early and publish later.

Publishing holds an HTTP request, so source delivery must measure its latency against HTTP-PERF and record the required exception before landing.
No endpoint latency compliance is claimed by this design; normal reads and mutations retain their declared targets.

Task-bound runner credentials and egress rules deny direct GitHub ref/PR mutations and unrestricted GraphQL writes; only the native publisher can advance the approved branch.
The legacy one-request repair mode retains its own separately bounded path.

A checkpoint plan expires after 30 minutes unless its live writer pin renews it.
The per-operation transfer/capture deadline remains unchanged; refreshing URLs cannot turn a bounded transfer into an indefinite one.
Commit validates exact scope/base availability, complete page count, ordered page-root digest, object states and current pins in the guarded manifest transaction.

Retain-base plans publish an immutable native base record only after required Git objects and exact repository/commit identity are verified.
Incremental plans cannot become recoverable without that referenced base and every required object.
A deployment round trip proves connectivity, not the availability of a particular task's base.

Restore begins with the read-purpose authorization request against its complete checkpoint/base.
That request acquires or adopts one restore pin and returns the bounded manifest root and ordered page identities.
GET each page, verify its digest and root, then obtain the registered object reads/keys and reconstruct files, index, Git state and history.

Page and object GET requests only check the existing pin; they do not create a restore session or renew its lifetime.
Renew the logical restore pin through its PATCH while transferring; expiry blocks attachment and preserves the prior recovery pointer.
An expired owner can be replaced only through a new live lease's atomic adoption after generation, scope and object-state checks.

Complete manifests remain readable under authorized handoff; incomplete plans never become recovery pointers.

## Proposed limits and maintenance

These are proposed resource limits, not measured speed or availability guarantees.
Smaller fleet budgets, lease limits and host capacity always win.
Changing these defaults requires reviewed configuration and boundary tests.

| Bound | Proposed value |
|---|---|
| Unfinished DTs | 16 per fleet; reject excess new objectives with a clear limit response. |
| Wall deadline | Seven days including waits; blocked/paused time does not reset it. |
| Cumulative paid-model budget | USD 5 per DT, capped by current fleet/credit limits; never refreshed on wake. |
| Running time and attempts | Two hours cumulative running time; 12 execution leases; eight repair publications maximum. |
| Decision calls | 128 physical calls per DT, including retries; pre-task calls remain charged to their source admission. |
| Human input boundary | Four messages/four action items per message; check after each completed action group. |
| Fallback wait | First read after 60 seconds, then 120, 240 and 300-second capped spacing; no more than 24 fallback reads per task. |
| Maintenance | Existing supervised-daemon framework, ten-second scan interval, 64 due rows per pass with `SKIP LOCKED`; no new scheduler service. |

Unchanged fallback reads retain the wait generation without adding an agent admission.
Changed eligible evidence consumes the current wait and creates at most one deduplicated continuation.
Task deadline, cancelled state or exhausted cumulative bounds prevent another fallback admission.

The supervised worker also advances abandoned input claims, uncertain effects and checkpoint cleanup in bounded passes.

Provider/model reservations and unknown physical call outcomes are tracked separately from task ready/wait state.

Implement maintenance in `agentsfleetd/src/sweepers/tasks.rs` using the current supervision/shutdown pattern.
Do not spawn detached recovery loops or a recurring user-visible cron for each DT.
Workspace authorization and repository grants are checked again on every continuation, even when stored files remain available.

### Event delivery and fallback timing

Sixty seconds is a proposed backup interval, not a provider requirement, measured optimum or delay imposed on signed events.
A matching signed completion makes the DT eligible immediately; the next run still needs the fleet slot.
After arming a wait, make one authoritative status read to close the event-before-registration race.

Subsequent fallback checks read only the exact recorded PR/head/check status through trusted daemon code.
They do not acquire an agent lease, restore files or call a decision model when evidence remains unchanged.
Only changed evidence, a head advance or a named blocker creates a deduplicated continuation or native blocked outcome.

Each native read claims the current wait for 30 seconds and rechecks task generation, deadline, workspace ownership and current GitHub grants.
Mint only repository-scoped read permissions within those grants; the wait checker admits no arbitrary URL or repository write.
Keep network requests outside database transactions and reject a stale result before changing state or adding an admission.

Allow at most four concurrent native wait checks per daemon and ten seconds per request, with no immediate same-pass retry.
Honor provider rate-limit cooldowns even when they exceed the normal 300-second spacing.
After 24 fallback reads, stop autonomous polling and show its exhausted state; valid signed completion or authorized refresh may still advance the live wait.

The cap of 16 unfinished DTs bounds stored work and intent candidates, not simultaneous workers or expected everyday workload.
It is a proposed initial product limit and needs workload evidence before any claim that it is optimal.

### User-visible decisions and transfer progress

Show activity in browser chat and the attached command-line interface (CLI), with the same native task/admission identities.
Decision dispatch emits a started state; validated routing, failure or goal assessment emits the corresponding completed state.
Use labels such as `Checking your request (<provider/model>)` and `Checking goal progress (<provider/model>)` with the selected profile.

Show the resulting action or uncertainty after completion, not private reasoning or a fabricated model explanation.
Expandable details can show the decision purpose, resolved model, typed assessments, duration and accounted usage.
Fast activity remains in history; prolonged activity updates the existing row rather than posting repeated chat messages.

Repository preparation, change scanning, encryption/upload, manifest verification and restoration have distinct stages.
Upload progress counts acknowledged unique bytes against known missing bytes; retries cannot count the same content twice.
State reused content separately and use counts or an indeterminate state when the total is unknown.

```text
Checking your request (Jev)...
Repairing Linkwarden PR X
Saving changes: 18 / 40 MiB uploaded; repository base reused
Upload complete; verifying saved state...
Work saved. Waiting for checks on current head.
```

Only confirm `Work saved` after the guarded manifest commit succeeds.
While attached, coalesce transfer updates to at most one per second; machine-readable output carries the same bounded typed states.
Waiting stays visible without repeated unchanged notices; a closed CLI does not stop an admitted DT.

## Initial activation and acceptance

Setup readiness requires an assigned decision profile, its authorized vault reference, reviewed rates/version policy and the task-capable runner protocol.
Coding recovery additionally requires the R2 bucket/endpoint, bucket-scoped credential, configured master-key versions and a verified storage/encryption round trip.
Missing setup leaves the fleet's DT capability unavailable with a named prerequisite; ordinary conversation retains its existing path.

Each decision profile evaluates its selected model/version on frozen cases before autonomous write decisions are enabled.
Actual-model and R2 calls belong to bounded development verification after implementation approval; this proposal runs none.
R2 cannot enable coding recovery until create-only writes, stopped-lease rejection, ciphertext tamper detection and cross-workspace refusal pass.

Use one same-repository Linkwarden PR submitted by Indy with a controlled failing regression and validation-only workflows.
The fleet independently diagnoses and writes its repair; the test operator supplies no patch or diagnosis.
Replace its worker during exact-head CI wait, then prove current authority, compatible files/history and one continuation.

Exercise a human head advance, duplicate/early completion, uncertain publication, consumed-input crash and collector/publisher race through served endpoints.
Use a six-GiB synthetic repository for size/restore checks rather than claiming a particular public repository has that size.
UI tests cover ordinary fleet chat, queued work, two unfinished DTs, clarification and reconnect without duplicate submission.

For Grafana, verify the actual development source before freezing the query and symptom threshold.
Freeze service, environment, deployed revision, ten-minute observation window and recovery threshold in the acceptance ledger before the controlled run.
Missing telemetry blocks that demonstration with its missing evidence; it does not count as recovery or authorize a different production test.

Development repair approval does not authorize merge, deployment or production changes.
Existing deployment authority remains separate even when the proposed DT tracks post-deploy observation.
