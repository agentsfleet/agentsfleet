# Three durable repairs across CI and worker replacement

Read this scenario to follow three hypothetical repairs through ordinary chat, shared worker capacity, lost publication acknowledgements, later checks and controlled incident verification.

| Scope | Current status |
|---|---|
| Source | M236 contains native task, decision, publication, recovery, CLI and browser implementations. |
| Combined demonstration | Unfinished. The sequence below describes required observations, not a completed run. |
| Live evidence | The [acceptance ledger](../../../playbooks/operations/acceptance/baselines/M236_001-autonomy.md) owns actual bindings, measurements and remaining proof. |
| Authority | One reviewed fleet setup, explicit existing-PR targets, bounded task limits and configured notification destinations. |
| Exclusions | Fleet merge/deployment, arbitrary repository writes and an unreviewed monitoring-to-repair handoff. |

## Before the demonstration

Indy supplies three actual pull request (PR) URLs from a controlled Linkwarden development repository.
The names, defects and commit letters below are hypothetical.
They establish no diagnosis of real Linkwarden code.

Each PR needs an unchanged regression that exposes its assigned defect.
The fleet needs current repository grants and reviewed permission to update each exact existing PR.
The PR author may use Codex, Claude Code or another tool; authorship grants the fleet no authority.

| Task name | Accepted objective | Target |
|---|---|---|
| Import repair | Fix a missing-title import crash while retaining the existing fallback behavior. | PR X, same-repository branch, initial head A. |
| Search repair | Remove pagination duplicates without weakening the regression. | PR Y and its distinct branch. |
| Export repair | Correct Unicode export while preserving the accepted format. | PR Z and its distinct branch. |

The accepted completion questions require the unchanged regression and required Continuous Integration (CI) checks at the current head.
[Question ownership](../goal_evaluation.md#who-writes-and-stores-the-questions) defines setup review and later revision adoption.
The example uses Jev; [decision models](../goal_evaluation.md) owns provider selection and assessment rules.

Indy uses authenticated browser chat or the command-line interface (CLI) through a coding agent.
Both clients send ordinary fleet messages through the existing admission path.
A machine credential cannot impersonate a human approval.

## Admit and steer work through ordinary conversation

```text
Indy                    Fleet message admission          Current fleet lease
 |                                |                               |
 | Repair import PR X             |                               |
 |------------------------------->| save source message A1        |
 |                                |------------------------------>| interpret request
 |                                |<------------------------------| bind Import repair
 | Repair search PR Y             |                               |
 | and export PR Z                |                               |
 |------------------------------->| save source message A2        |
 |                                |------------------------------>| bounded decomposition
 |                                |<------------------------------| save immutable items
 |                                | queue Search and Export       |
 | Keep import's fallback title   |                               |
 |------------------------------->| save source message A3        |
 |                                |------------------------------>| bind accepted steer
 |                                |<------------------------------| save meaning and cursor
 |                                |                               | apply at safe boundary
```

Indy names the work through its purpose or PR URL, without entering an internal task identifier.
An ambiguous instruction such as “cancel that” requires clarification.
A new request cannot inherit an older repair's publication authority.

A2 creates two stable task items.
Replaying A2 must return those items instead of creating duplicate repairs.
A3 changes the import method within the accepted objective; it grants no new repository access or completion criteria.

The human-input view reads messages in source order while machine work waits.
A steer for Search remains queued while Import owns the lease.
Import cannot write Search's PR or attach Search's writable files.

A greeting receives a brief greeting without restarting an investigation.
A status question uses recorded evidence and states its freshness.
An explicit request for fresh status may read current evidence within the existing authority and limits.

[Human input](../durable_tasks.md#human-input-while-work-runs) owns input claims, accepted receipts and safe boundaries.
[Intent routing](../goal_evaluation.md#intent-routing) owns classification and uncertainty.
Neither a sidebar selection nor a new chat mode supplies task authority.

## Publish, lose a worker and recover the same task

The fleet runs one active lease at a time.
The [lease flow](../lease_flow.md) and [task scheduling](../durable_tasks.md#proposed-input-claims-and-scheduling) own serialization and fair switching.
Waiting work releases the slot so another assigned task can run.

```text
Import lease             Native publication          Saved state / GitHub
     |                            |                            |
 read PR X at A                   |                            |
 reproduce -> patch -> test       |                            |
 save compatible generation G1 ------------------------------>|
 publish tested candidate B ----->| record effect E1           |
                                  | append only if head = A -->| B accepted
     |<--- acknowledgement lost --|                            |
 accept steer A3 beyond G1 cursor |                            |
 worker disappears               |                            |
     |                            |                            |
 replacement lease               |                            |
 fresh authority + restore G1 <-------------------------------|
 read E1 and exact PR history --->| confirm B already exists    |
 replay accepted A3              |                            |
 test correction C -> save G2     |                            |
 publish C, record CI(C) wait ----|--------------------------->|
 settle report and release       |                            |
 Search lease -> own files -> head Y1 -> wait -> release       |
 Export lease -> own files -> head Z1 -> wait -> release       |
```

The lost acknowledgement and worker replacement are deliberate failure conditions for the controlled run.
The replacement reconciles E1 before attempting another publication.
Saved input A3 remains available beyond G1's covered-input cursor.

A human push from A to another head must reject a publication prepared against A.
The fleet keeps saved work and reads fresh evidence instead of overwriting that push.
A completed remote write retains its reconciliation record even if its acknowledgement disappears.

[Existing-PR publication](../durable_tasks.md#existing-pull-request-publication) owns expected-head checks, reviewed workflows and effect receipts.
The initial write scope excludes fork heads, protected/base targets, workflow edits, force pushes, merge and deployment.
A push with privileged or deployment effects needs separate review before admission.

[Workspace recovery](../sandbox_evaluation.md#coding-sessions-and-worktrees-across-leases) owns compatible files, Git state, history and confirmed outcomes.
[Incremental checkpoints](../sandbox_evaluation.md#large-repositories-and-incremental-checkpoints) owns retained bases, encrypted chunks, transfer bounds and collection pins.
The repository's size must come from measurement; this example assumes no fixed checkout size.

Failed saves retain the previous recoverable generation and expose possible unsaved work loss.
An unsettled worker kill grants no fresh capture authority.
The replacement reconciles uncertain remote effects before resuming local work.

Search and Export restore their own compatible state or start fresh checkouts.
A shared fleet policy never makes Import's writable checkout theirs.
API-only tasks need no mandatory checkout, and recovery grants no incidental GitHub access.

## Continue from exact-head checks

A signed completion or bounded fallback read can wake the matching wait.
The wake requests fresh evidence; it does not certify success.
[Event matching and fallback timing](../durable_tasks.md#event-delivery-and-fallback-timing) owns deadlines, duplicate handling and competing transitions.

```text
CI(Y1) passes -> one Search continuation -> current-head proof -> done
CI(C) fails  -> one Import continuation -> correction D -> wait for CI(D)
CI(Z1) event missed -> bounded fallback -> pending: retain wait, no inference
                       later changed evidence -> Export continuation -> proof -> done
CI(D) passes -> Import continuation -> accepted goal + native proof -> done
```

Each continuation rechecks current workspace access, repository grants, target identity and remaining task limits.
Duplicate events, old heads and replaced wait generations cannot complete newer work.
An unchanged fallback observation retains the wait without restoring files or starting another model call.

Jev proposes a goal result against the accepted questions and current evidence.
Native proof checks decide whether the task may finish.
[Goal evaluation](../goal_evaluation.md#goal-evaluation) owns malformed, unavailable, contradictory and stale judgments.

A pause, cancellation, deadline or revoked grant must prevent prohibited new effects and later restarts.
An already issued publication still needs reconciliation.
Cancelling Import while Search and Export finish is a required negative case, not the successful outcome shown above.

## Verify the controlled Grafana and Slack outcome

A controlled symptom connects the repair to an observable development outcome.
Before the run, pin the service, environment, deployed revision, Grafana data source, query, time window and recovery threshold.
Verify whether that source supplies logs, metrics or both.

```text
Existing schedule -> bounded Grafana observation
                          |
                quiet ----+---- failing symptom
                  |                  |
             no incident       correlated Slack report
                                     |
                          human-approved PR repair scope
                                     |
                          three-task repair sequence above
                                     |
                          separate human merge/deploy approval
                                     |
                          fresh scheduled Grafana observation
                                     |
                     recovered / still failing / insufficient evidence
                                     |
                          configured Slack destination
```

The scheduled observer uses a configured Slack destination and stable incident correlation.
Repeated failing observations must not create duplicate notices or repair tasks.
Detection alone grants no permission to repair, and setup capability does not imply unlimited standing work approval.

This scenario requires an authorized human to approve the repair scope.
[Workspace approval](../sandbox_evaluation.md#workspace-approval-in-chat-or-slack) owns that boundary.
Separate observer and repair fleets cannot impersonate one another or attach another incident's workspace.

Green CI establishes repair validation only.
A separately authorized development deployment precedes the fresh Grafana observation that can establish symptom recovery.
Missing or inconclusive telemetry must produce an explicit unknown result.

[Scheduler flow](../data_flow.md) owns due delivery, busy fleets, overlapping ticks and cancelled queued work.
[Slack incident response](slack-incident-responder.md) owns notification routing and reply threads.
Elasticsearch and a new scheduler are not prerequisites for this controlled Grafana demonstration.

## What Indy sees

The source includes CLI decision/transfer progress and browser task readback.
Their presence does not prove the combined live scenario.
The acceptance ledger records verification status separately from screenshots and illustrative transcripts.

| Surface | Expected observation |
|---|---|
| Ordinary chat | Brief action/results, saved status with freshness, natural-language steering and clarification only for ambiguous targets. |
| PR badge | Standard pull-request icon and actual PR number, linked through one explicit native task identity. |
| Durable tasks | Task purpose and recorded action or wait reason; accepted objective, criteria and controls behind a disclosure. |
| Outputs | Native PR targets, including bounded history from saved task activity; local edits do not prove publication. |
| Changes | Recorded file writes with expandable details; no new diff service. |
| Sources | Saved fleet repository policy, without claiming fresh remote grants. |
| CLI | Native task identity and decision/transfer observations through the existing attached event path. |
| Recovery | Restoration, uncertain publication readback and confirmed resumed work remain distinguishable. |

A waiting summary can name the actual CI run and head when native evidence supplies them.
A timed-out publication must remain uncertain until readback confirms its effect.
Neither interface invents completed work from a status label or model prose.

The composer remains reachable while evidence expands.
Typing and reading older messages must preserve the draft and reading position.
A late PR badge stays on the sender line without moving the visible reply.

## Evidence needed to close this scenario

The combined demonstration remains unfinished.
Unit tests, browser fixtures and successful provider calls cannot replace the linked run evidence.
M236 completion depends on the [active acceptance ledger](../../../playbooks/operations/acceptance/baselines/M236_001-autonomy.md).

| Proof | Evidence to retain |
|---|---|
| Three assigned repairs | Actual PR URLs, accepted objectives/questions, stable task identities and one active fleet lease. |
| Conversation and steering | Greeting with a complete empty tool-call inventory; saved status; accepted A3; ambiguous-target clarification; duplicate A2 replay. |
| Publication and recovery | Exact tested trees/heads, unknown E1 readback, worker replacement, compatible G1/G2 coverage and accepted-input replay. |
| CI continuation | Signed, duplicate, stale, fast and missed completion cases; one effective wake; final current-head checks and unchanged regressions. |
| Authority and limits | Human advancement, revoked grants, closed PRs, pause/cancel, deadlines and exhausted limits stop prohibited effects. |
| Grafana and Slack | Quiet/failing/recovered windows, deployed revision, bounded queries, stable incident identity and configured destination receipts. |
| User surfaces | Actual CLI capture and browser evidence for native task attribution, reconnection, readable progress, preserved drafts and stable scrolling. |

Mechanism pages own API collections, Rust interfaces, provider limits and checkpoint formats.
This page owns their combined worked example and its remaining proof.
