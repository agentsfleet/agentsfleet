# Durable repair across CI results and worker replacement

## Status and scope

This is a proposed development-autonomy scenario, not shipped behavior.
It brings goal evaluation, later-event continuation, existing-Pull-Request authority and worker recovery into one implementation-spec scope.
The detailed design, storage changes, Jev integration and runtime acceptance remain pending.

The existing manual path can investigate a GitHub Actions failure and open or correct its own event-derived repair draft during one active request.
It does not supply arbitrary existing-Pull-Request writes, durable cross-request repair ownership or recoverable coding workspaces.
See [current lease behavior](../lease_flow.md) and [sandbox evaluation](../sandbox_evaluation.md).

Production launch is deferred until the M23x specifications are written and full autonomy is demonstrated in development.
M224 remains the separate manual-first evidence campaign; its reduced closure scope does not make these proposed capabilities available.
Elasticsearch is not required for the proposed Grafana-only demonstration.

## One Linkwarden task, three distinct requirements

Use a controlled Linkwarden development repository and an unchanged regression that fails for a known code defect.
A person using Codex opens PR X; the authoring tool is irrelevant to authority.
The fleet needs explicit reviewed permission to update that exact existing PR, rather than permission derived from having created its own draft.

```text
Indy + Codex open Linkwarden PR X at head A
                         |
                   CI(A) fails
                         |
        Authorized repair task T targets PR X
                         |
       Worker 1 independently reads evidence
       Reproduces -> patches -> tests candidate B
                         |
       Save compatible history/workspace generation G
       Record expected CI(B)/wait -> publish B -> reconcile
                         |
          Request finishes; worker 1 is replaced
                         |
        CI(B) event or bounded fallback wakes T
                         |
       Worker 2 checks authority, target and head
       Restores G; rereads exact-head CI evidence
                    /                \
                 failing             passing
                    |                   |
          Correct same PR X       Jev evaluates goal
          Publish C; await CI(C)   + native proof checks
                                        |
                               Report verified outcome
```

Later CI is a trigger requirement: the task survives the original request and resumes on matching evidence.
An existing PR is an authority requirement: another author created the target and may keep editing it.
Worker replacement is a recovery requirement: another worker reconstructs compatible state without duplicating uncertain writes.

One scenario must prove their combination; passing each in isolation is insufficient.
A fleet-created draft remains a supported target variant, not proof of the externally created PR variant.

## Goal-driven inference and Jev decisions

### Human conversation and task steering

Observed development failure: after a CI diagnosis, the user typed `hello` and the responder made two GitHub requests before repeating the prior incident report.
That is an unwanted reinvestigation, not successful task continuation.
Loading standing instructions on each inference does not by itself explain the behavior; both runtimes keep instructions in model context.

Current `afr_agent` combines installed instructions with trusted repair/workspace context (`src/prompt/mod.rs`).
It appends earlier user/answer turns and the newest event (`src/loop/mod.rs`).
It does not supply the proposed task-aware intent/goal lifecycle below.

[Codex's turn loop](https://github.com/openai/codex/blob/181b4a853d396f009a3a02b9c36eb393e6acbe75/codex-rs/core/src/session/turn.rs#L483) drains pending user input into history before eligible inference.
[Its request client](https://github.com/openai/codex/blob/181b4a853d396f009a3a02b9c36eb393e6acbe75/codex-rs/core/src/client.rs#L1033) separately prepares instructions, tools and transport input.

Treat this comparison as a source reference, not a claim that Codex has a hidden classifier we can copy.
The inspected Codex source is commit `181b4a853d396f009a3a02b9c36eb393e6acbe75` from `openai/codex`; no server-side behavior is established by that source.
The implementation spec must trace current `agentsfleet` admission/history/lease boundaries and preserve their provenance.

```text
New human message + current task state + standing instructions
                         |
             Interpret the user's current intent
        /                |                |               \
   conversation       question          steer        new task request
        |                |                |               |
  brief response   use known evidence   bind/update    validate scope,
  no task tools    label its freshness  task input     authority, goal
        |                |                |               |
        +----------------+         task-aware action/Jev loop
```

Pure social input must not restart, finish or reset a repair.
It makes no investigation or Jev completion call.
Reply briefly without recapping the diagnosis, naming the old run or suggesting a repair.

A status question can answer from recorded evidence while stating its age; an explicit refresh request can read current status within current authority.
An ambiguous instruction asks a short clarification rather than selecting an old incident from history.

A new request must not inherit an old task's publication authority.
An explicit steer targets a trusted current task; changing its objective or completion questions follows reviewed revision adoption, never an untrusted quoted message.
Cancellation, pause and scope changes require native lifecycle handling, not only a model promise to stop.

Define queued versus consumed steering and what a follow-up may change while a tool runs.
Preserve the existing one-active-lease rule; do not present existing queued events as implemented mid-run steering.
Initially, a greeting can be answered after the active run reaches its permitted boundary; prompt interrupt and cancellation semantics require explicit implementation proof.

### Goal evaluation

The user defines the desired outcome and completion questions through onboarding chat.
Review/Apply saves them in a versioned `instructions_md`; model text cannot silently alter the accepted objective.
For Linkwarden, questions include whether the unchanged regression passes at the current PR head and whether all required exact-head checks have concluded successfully.

```text
Accepted goal/questions + current trusted task/evidence
                         |
                    Agent action
                         |
               Tool result and fresh evidence
                         |
                 Jev decision proposal
       CONTINUE       WAIT        DONE       BLOCKED
          |            |            |            |
     next action   durable wake  native proof   named gap
                    and release  validation    / escalation
```

Jev is the proposed decision model, not an implemented runtime integration.
Its input includes the accepted questions, current task/target identities, bounded evidence references and the remaining limits.
Its structured result must bind the assessed setup revision, task generation and exact evidence/head, with reasons for unresolved questions.

Native code enforces current grants, write targets, stale-generation rejection, budgets and the mandatory proof requirements of the admitted task.
A Jev `DONE` without current required evidence cannot close a task.
Invalid, timed-out, unavailable or contradictory judgments consume bounded attempts and end in a named blocked result rather than default success.

The spec must define the Jev API/model, output schema, call placement, timeout, cost accounting and any live-provider authorization before implementation.
Jev calls share the task's cumulative money/time/attempt bounds; a new admission cannot reset them.
Raw logs, repository files and summaries are untrusted evidence and cannot change completion questions or authority.

## Durable ownership and event matching

The daemon must own trusted task/attempt identity, workspace and fleet, approved repository/base/PR/head, notification destination, current wait, compatible saved generation and cumulative limits.
The detailed design must place these records in the existing stores and select transaction/fencing boundaries; prose and semantic memory are not the authority store.
Every run uses the existing admission and lease path with one active lease per fleet.

Validate completion-event signatures and installation scope before matching the exact task wait.
Match current PR head, workflow/check identity, run attempt and task generation; a wake only requests a fresh read, never certifies success.
Duplicate, out-of-order, fast-completion and fallback wakes compete through one durable transition.

Register expected validation before publication and reconcile uncertain publication against the same target.

If a completion arrives before the write acknowledgement, reconcile it without dropping the wake or inventing a new repair attempt.
Keep the wait's deadline and delayed-check ownership in the existing supervised worker framework; a new recurring cron is not a substitute for task continuation.

## Existing-PR writes and human edits

Use a distinct reviewed existing-PR write mode and recheck current workspace access and repository grants on every admission.
Bind the exact open PR, same-repository head branch and approved base; initially refuse fork heads and protected/default/base target branches.
Retain workflow-edit, force-push, merge and deploy exclusions.

Before authorizing an existing head, review its repository workflows, secret exposure and environment protections to establish that the admitted push has validation-only effects.
A push that can deploy or trigger a privileged workflow is outside the initial demonstration; refusing explicit deployment calls alone does not enforce this boundary.

Read the remote head before preparing a patch and enforce the expected-head precondition at publication.
On another writer's advance, preserve saved work, stop publication and re-evaluate under fresh evidence; never overwrite a laptop push.
The publication mechanism and response to an unrelated advance require detailed design, not an assumed safe read-then-write sequence.

After a timeout, inspect the recorded branch and PR before retrying a write.
Closed or merged PRs, revoked access, cancellation and exhausted limits stop further effects and late wakes.
Reviewing the PR is a separate capability: the initial reviewer posts findings against an exact head; it does not become an independent branch writer.

## Worker and workspace recovery

```text
Worker 1: history H + files/Git W + confirmed outcomes O
                         |
                 Save generation G
                         |
                 Worker disappears
                         |
Worker 2: trusted task T -> compatible G -> fresh authority
                         |
             Reconcile remote PR/head and effects
                         |
             Resume from known outcomes only
```

Restore only compatible conversation, file/Git and confirmed-outcome coverage; after unsaved loss, expose the recoverable generation and reconcile remote effects.
The [coding-session recovery proposal](../sandbox_evaluation.md#coding-sessions-and-worktrees-across-leases) owns save/restore mechanics, retention and isolation; this scenario owns their effect on the repair task.

Full workspace recovery is required only for tasks that use a workspace.
An API-only repair must not acquire a mandatory checkout just to resume its trusted task.
A fleet without repositories must not acquire GitHub access as an incidental consequence of recovery.

The implementation spec must resolve the linked recovery decisions and demonstrate the task-level outcomes below.
Keep task recovery independent of the sandbox backend; replacing bubblewrap with Firecracker is separate.
[Compaction](../context_lifecycle.md) manages model input and does not save files or prove remote effects.

## Grafana monitoring and scheduler variant

Use `agentsfleet` development telemetry with a configured Grafana data source and bounded queries against a controlled reproducible symptom.
Grafana is the query surface; verify whether the configured source supplies logs, metrics or both instead of assuming log storage exists.
Pin service/environment, deployed revision, query/window and recovery threshold before the run.

```text
Existing scheduler -> observer queries Grafana
                           |
              quiet -------+------- failing symptom
                |                         |
         no incident notice       correlate incident + Slack
                                          |
                             approved repair task/proposal
                                          |
                             GitHub repair and CI loop
                                          |
                             human merge/deploy boundary
                                          |
                           scheduled fresh Grafana query
                                          |
                           recovered / persistent / unknown
```

A scheduled observer needs a configured notification destination; model-generated channel names are not routing authority.
Use stable incident correlation so recurring sweeps do not open duplicate tasks for the same symptom.
Keep monitoring detection separate from repair authorization: current proposals require an authorized person's [proposal approval](../sandbox_evaluation.md#workspace-approval-in-chat-or-slack), while unattended repair requires a separately reviewed standing-authority design.

Setup grants cap capability but do not alone establish unlimited standing work approval.
The spec must settle when an initial human command, proposal approval or bounded standing policy admits a task; no automatic cross-fleet handoff exists today.
Monitoring and repair may use separate fleets, but neither may impersonate the other's authority or recover another incident's workspace.

Exercise due-time delivery, duplicate ticks, busy fleets, overlapping sweeps, independent due fleets, pause/resume, cancellation/deletion and stale queued work against current scheduler semantics.
Choose measured timing tolerances and deduplication identities in the detailed spec; no new scheduler is proposed.
CI green ends preview repair validation only; after separately authorized development deployment, a fresh scheduled observation must verify the actual symptom or report insufficient data.

## Concise progress and tool evidence

Show what changed, what the agent is doing next and any decision or blocker the person needs to act on.
Use short action/purpose/result summaries such as `Read the failed job log`, `Published head B` or `Waiting for CI on B`; keep raw arguments, counts and full output in expandable evidence.
Only confirm an action after its native outcome is known; a timed-out push reads `Checking publication status`, not `Published`.

After worker replacement, retain confirmed progress and visibly distinguish restoration, reconciliation and resumed work without replaying the entire incident report.
Avoid repeated no-change updates, long metadata inventories and raw internal reasoning in the normal conversation surface.
A status answer based on saved evidence labels its freshness instead of silently spending tools to refresh it.

## Recoverable failure notices

Show an inline, accessible notice beside the affected request, preserving the submitted message and any partial response.
Use the known failure stage rather than raw provider exceptions: connection failure, interrupted inference, or uncertain tool effect.
The browser losing its stream does not prove the inference or task stopped; reconnect/read back task state before offering to start another run.

| Observed state | Proposed copy and action |
|---|---|
| Provider connection failed before usable response | "Couldn't connect to the model." Offer a bounded retry only after checking native attempt state. |
| Inference stream interrupted | "The response was interrupted." Reconnect or read back the existing attempt; start another inference only through its native retry rules. |
| Tool write outcome unknown | "The action's result isn't confirmed yet. Checking its status." Reconcile the remote effect before retrying it. |
| Jev unavailable after allowed retries | "Couldn't verify whether the goal is complete." Preserve progress and show a blocked state. |

Do not display "Opening handshake" unless that technical detail helps the user act.
Use a labelled status/error icon and text, with keyboard-accessible recovery actions; avoid toast-only failures or an indefinite spinner.
Keep browser reconnection, inference retry and tool-effect reconciliation distinct, with retries charged to the same task limits.

## Acceptance and unresolved design decisions

The reviewed spec must map every row below to exact current-code changes, named checks and retained evidence.
No review score or synthetic fixture marks a live row passed.

| Proof | Required observation |
|---|---|
| Combined Linkwarden flow | Externally opened authorized PR; exact-head failure, fleet repair, worker replacement during CI wait, same-task/head wake and final regression proof |
| Event races | Duplicate/out-of-order/fast completion and fallback produce one effective transition; stale heads cannot complete a newer attempt |
| Publication races | Human push, lost acknowledgement and retry preserve remote work and one target identity |
| Authority/terminal state | Revocation, pause/cancel/delete, closed/merged PR, deadline and budget stop prohibited effects and late restart |
| Recovery | Compatible history/files/Git/outcomes restored; unsaved loss and uncertain effects surfaced; no cross-task or cross-workspace attachment |
| Jev | Current-evidence result, stale result, injected evidence, malformed output, timeout and contradictory/unsupported success |
| Human conversation | `hello` after diagnosis gets one brief greeting without incident recap/run citation/repair suggestion and a present complete empty call inventory; status uses labelled recorded evidence; explicit refresh checks current state; ambiguous/new requests cannot reuse old task authority |
| Scheduler/Grafana | Real scheduled query; quiet and failing windows; deduplicated incident/notification; independent fleets; fresh post-deploy recovery observation |
| Progress and failure recovery | Short confirmed action/result and waiting/blocked summaries survive worker replacement; connection loss, interrupted inference, uncertain write and unavailable Jev show distinct notices and recovery actions without duplicate inference/publication |
| Audit and notifications | Complete call inventory, retained evidence references, visible thread result and reconciled delivery retries |

Before implementation, resolve durable store/schema ownership, publication transport/precondition, Jev integration, instruction-revision adoption, incident grouping, standing authority, retention/quotas and scheduler timing.
Assign the implementation spec number from current remote state; no number is reserved by this scenario.
Production and the new sandbox backend are outside this development-autonomy implementation scope.
