# Coding sandbox evaluation

> Read this to evaluate SPACE (`Sandboxed Platform for Agentic Code Execution`) approaches and source for an `agentsfleet`-owned coding sandbox. The [working concept brief](../../concepts_brief.md) records the brainstorming scope. [Runner behavior](runner_execution.md) owns implemented runtime behavior; [conversation compaction](context_lifecycle.md) owns model context. 

Native sandbox changes and durable workspace recovery remain proposals.

## Recommendation and decision criteria

Build an `agentsfleet`-owned sandbox using selected SPACE approaches and source where they solve an observed problem.
The owner requires control over sandbox performance and future changes.
Finish shared compaction independently of sandbox implementation.

Use SPACE as a studied reference, with each code selection reviewed against our existing runner boundaries.
Keep its service as a comparison baseline; operating that service is outside the current direction.
The first integration target is a small Firecracker wrapper using SPACE's runtime mechanisms and our existing executor.

Exact copied files and their dependency set still need implementation review; no SPACE code has been copied by this brainstorm.

Use isolation, recoverable work, authorization, end-to-end latency and operational ownership as the decision criteria. Isolation and authorization are pass/fail requirements. Compare performance only after each option satisfies the same recovery behavior. 

No local SPACE runtime benchmark has been run for this evaluation.

| Option | Benefit | Cost or gap | Recommendation |
|---|---|---|---|
| Improve the existing engine | Uses the existing runner and directly addresses repeated context work and repository contention. | The implemented bubblewrap engine shares the host kernel; durable remote workspace recovery remains a target. | Baseline and preparation work. |
| Operate SPACE behind its HTTP API | Reuses its Firecracker lifecycle, storage and process transport. | Adds a platform to operate; requires authorization mapping, remote checkout and recovery integration. | Comparison baseline, outside the current direction. |
| Adapt selected SPACE approaches and source | Keeps sandbox behavior and performance under our control within the existing runner. | We own integration, updates and every dependency brought in with copied code. | Owner-selected design direction; evaluate components individually. |
| Copy its lifecycle/control plane broadly | Reuses more implementation at once. | Creates overlapping scheduling, tenancy and recovery ownership; effectively maintains a platform fork. | Outside the current direction. |

The current implementation and missing capabilities are recorded in [runner behavior's Facts table](runner_execution.md#facts).
SPACE records desired state in PostgreSQL and reconciles workers directly or through Kubernetes; Kubernetes is optional.
Its service compatibility policy excludes Rust crate interfaces, so internal source reuse carries a separate maintenance burden.

See upstream [request flow](https://github.com/perplexityai/SPACE/blob/20d321385dac0855b39f86cd93c008ff5ec4ce3c/docs/architecture.md#request-and-execution-flow) and [versioning](https://github.com/perplexityai/SPACE/blob/20d321385dac0855b39f86cd93c008ff5ec4ce3c/docs/versioning.md).

## First Firecracker wrapper

The owner wants a small sandbox that can run the agreed scenarios and provide measured startup improvements.
Use SPACE as the starting reference for low-level Firecracker control and reuse source where its behavior fits.
Keep the existing runner's agent loop, executor, permissions and fleet admission.

```text
Fleet admission and agent loop
             |
Existing engine interface
             |
SPACE-derived Firecracker driver
             |
Verified read-only guest tools + private task disk
             |
Existing executor -> commands, file edits, test results
```

Start on one qualified Linux host with Kernel-based Virtual Machine (KVM) access and a pinned guest image.
Use the existing [engine boundary](../../rustd/crates/afr_sandbox/src/engine/mod.rs) for sandbox ownership and the [executor](../../rustd/crates/afr_executor/src/lib.rs) for commands.
The planned guest entry and separate writable disk already belong to [runner behavior](runner_execution.md#crates).

### Reuse before adding another subsystem

| Need | Starting source | Local integration |
|---|---|---|
| Launch and stop safely | SPACE [jailer launch](https://github.com/perplexityai/SPACE/blob/20d321385dac0855b39f86cd93c008ff5ec4ce3c/crates/libs/node/runtime/core/src/launch/os/linux.rs) and [owned launch/cleanup](https://github.com/perplexityai/SPACE/blob/20d321385dac0855b39f86cd93c008ff5ec4ce3c/crates/libs/node/runtime/supervisor/src/service.rs) | Adapt launch, limits, process ownership and partial-failure cleanup behind our engine. Preserve our lease authority. |
| Control Firecracker | SPACE [API client](https://github.com/perplexityai/SPACE/blob/20d321385dac0855b39f86cd93c008ff5ec4ce3c/crates/libs/node/runtime/core/src/fc_api.rs) and [behavior tests](https://github.com/perplexityai/SPACE/blob/20d321385dac0855b39f86cd93c008ff5ec4ce3c/crates/libs/node/runtime/core/src/fc_api/tests.rs) | Adapt the needed operations and tests; inspect imported types and map errors into our existing error model. |
| Run commands and edit files | Existing [executor server and client](../../rustd/crates/afr_executor/src/lib.rs) | Carry the existing protocol across the guest boundary. Prove its connection handshake, output bounds, cancellation and file behavior on a real guest. |
| Prepare repository contents | Existing [trusted checkout](../../rustd/crates/afr_supervisor/src/lease_loop/checkout.rs) and [workspace design](runner_execution.md#workspace-between-leases) | Add private guest-disk preparation with exclusive host/guest ownership. Fetch credentials remain outside the guest; validate the exact selected revision. |
| Reduce startup work | SPACE [base snapshots and optional local pools](https://github.com/perplexityai/SPACE/blob/20d321385dac0855b39f86cd93c008ff5ec4ce3c/docs/base-snapshots.md) | Compare cold boot with a clean template snapshot. Evaluate an idle pool only after measuring its benefit and resource cost. |

Importing SPACE's runtime crates directly brings shared service, runtime and generated protocol dependencies.
The [runtime manifest](https://github.com/perplexityai/SPACE/blob/20d321385dac0855b39f86cd93c008ff5ec4ce3c/crates/libs/node/runtime/core/Cargo.toml) makes that dependency set visible.
Inspect the actual extracted set before choosing direct reuse or adaptation; a full dependency audit is not complete.

### Build and test in this order

1. Boot the pinned image through the adapted launcher, connect the existing executor, run a command and confirm complete shutdown.
2. Prepare a private repository disk, edit code, run its tests and return the exact diff through the trusted runner.
3. Save declared task files, release compute and restore those bytes under a fresh lease, including an A-to-B-to-A task switch.
4. Add clean-template snapshot startup, record the actual boot mode and compare the first useful command against cold boot.
5. Run the [coding walkthrough](#walkthrough-to-build-and-test) and [repository and incident scenarios](#repository-revisions-across-events), recording remaining application integration gaps explicitly.

For the first recovery proof, retain conversation progress and declared Git/file state, then start fresh guest compute.
Interrupted commands require outcome verification before retry; resuming their memory is not required by this initial proof.
Snapshot startup uses a clean template, separate from the saved work of an incident.

Preserve staged edits, local commits, declared untracked files and necessary Git data through the [session save rules](#coding-sessions-and-worktrees-across-leases).
Use bounded synthetic repositories and explicit cleanup for experiments; production retention and numerical service guarantees remain open.
Those open budgets must not turn the first engine experiment into a new incident-management platform.

### Challenge the speed claim

Firecracker's isolation does not establish faster startup than the current engine.
Measure request-to-repository-ready and the first useful command, including image staging, checkout and dependency setup.
SPACE's base-snapshot path can fall back to cold boot; record the actual mode rather than reporting every request as a snapshot start.

Cloning a template snapshot can duplicate guest identities and cached application state.
Keep tenant files and credentials out of reusable templates, refresh per-instance identity and reconnect guest transport after restore.
Firecracker documents these limits in [snapshot uniqueness and transport reset](https://github.com/firecracker-microvm/firecracker/blob/main/docs/snapshotting/snapshot-support.md#snapshot-security-and-uniqueness).

No Firecracker startup benchmark has been run for this wrapper.
Running real guests needs Linux KVM access, as the [Firecracker prerequisites](https://github.com/firecracker-microvm/firecracker/blob/main/docs/getting-started.md#prerequisites) specify.
The existing `make test-runner-kernel` lane proves bubblewrap behavior; the selected guest engine needs its own real-backend evidence before support is claimed.

The [design handoff](../reviews/space-sandbox/README.md) records the owner's delivery order and the separate implementation restart after pull request 784.
It preserves the completed concept review without treating it as detailed design approval or runtime proof.

## What the Claude session reference establishes

The owner supplied an inside-the-guest report from Oct 22–24, 2026. It is useful evidence about one environment. Keep its observations separate from proposed explanations.

| Evidence supplied | Supported reading | Still unproved by that observation |
|---|---|---|
| Hypervisor flag and a container detector reporting Docker | Virtualized execution and a detected container environment. | The hypervisor implementation, host placement and session-to-machine mapping. |
| Kernel build name includes `fc` | Firecracker is a plausible explanation. | A kernel build label does not identify the running virtual machine monitor. |
| New boot/kernel, same repository files | Files were preserved or reconstructed across that transition. | Disk reattachment versus snapshot restoration, synchronization or another mechanism. |
| Proxy variables and a certificate bundle | Applications are configured to use a proxy. | Enforcement against direct sockets, alternate protocols or modified environment variables. |
| Device size exceeds reported available space | Capacity accounting needs investigation. | The particular quota or storage layout. |
| A long-lived agent process and `--stdin` supervisor flag | A resident process and an input-related supervisor option. | Message framing, durable acknowledgement and interruption ordering. |

Anthropic's [cloud documentation](https://code.claude.com/docs/en/claude-code-on-the-web#security-and-isolation) separately describes isolated session virtual machines and a Git proxy that keeps credentials outside the guest. Its [expiry guidance](https://code.claude.com/docs/en/claude-code-on-the-web#environment-expired) restores conversation history into fresh compute while background work may have stopped. Neither identifies its hypervisor or disk recovery implementation.

A container inside a microVM can make userland reproducible. It is an optional packaging choice for this proposal. A guest image with a restricted executor may satisfy that need without operating Docker inside every guest. 

Test whichever packaging we select against actual tools and privileges.

## Proposed ownership

```text
 User: repository + request + follow-ups
                    |
         agentsfleetd: admission and current authority
                    |
       trusted runner supervisor, outside tenant code
          |                         |
   shared context lifecycle    coding workspace preparation
          |                         |
   model calls and notes       Engine / Sandbox / Executor
          |                      /                 \
   guarded checkpoint       bubblewrap       native guest engine
   publication                                      |
                                          Firecracker guest
                                          tools + private files

   Git credentials remain in a trusted broker/proxy.
   A sandbox handle never grants fleet or repository authority.
```

Keep the fleet as the durable runtime. A workspace handle is a runtime resource, not a second task scheduler. Preserve the existing one-active-lease rule. 

Independent coding tasks need independent writable checkouts; sharing an authorized read cache must not share mutable working trees.

The existing [engine interfaces](../../rustd/crates/afr_sandbox/src/engine/mod.rs) and [executor](../../rustd/crates/afr_executor/src/api/mod.rs) already separate runtime effects. However, [checkout](../../rustd/crates/afr_supervisor/src/lease_loop/checkout.rs) requires a host-visible workspace. A remote engine therefore needs a repository preparation path; implementing `Executor` alone is insufficient.

Use traits for effects with distinct implementations. Use generics for known adapters and trait objects where the backend is selected at runtime. Use `Fn` for a repeatable cancellation observation, `FnMut` for progress accumulation and `FnOnce` for consuming prepared publication. 

These choices express ownership; additional trait layers do not themselves improve concurrency.

## Walkthrough to build and test

Use a synthetic Git repository with a pinned base commit, a small build, an existing test and one requested behavior change. Keep real user repositories out of destructive recovery tests. Give each run its own namespace and retain only task-owned output.

| Step | User action or injected event | Required observable result | Proof |
|---|---|---|---|
| 1 | Bind a repository and ask for the change. | Record the exact base commit and current repository permission; create one private writable checkout. | Repository hash, clean initial diff and denied foreign repository access. |
| 2 | Start work and perform setup. | Report progress for checkout, environment setup and the first useful command separately. | Stage timings and a command result; a live machine alone is insufficient. |
| 3 | Send a follow-up while a command is running. | Acknowledge the queued steer event; finish the current run before leasing that event. Show queued versus consumed status. | A controlled command barrier proves the follow-up does not start a second active lease and runs after the first. Mid-run steering is a separate proposed behavior. |
| 4 | Grow both setup and runtime context through repeated compaction. | Continue with the goal, latest instructions and complete tool groups; retain original source history. | Forced host tests plus the unchanged full continuity journey. Turn count alone does not prove a compaction occurred. |
| 5 | Edit tracked files, create an untracked file and leave a local commit unpushed. | Report which checkpoint durably protects each item and its recovery boundary. | Hashes, Git status and the committed workspace manifest; no assumption that Git commit captures all files. |
| 6 | Suspend, then replace the worker after a completed durable save. | Restore the declared files and base commit under a new current lease. Reject stale writers. Report processes as restored or stopped according to the chosen mechanism. | Restore on a different worker; compare hashes and reject an old lease's mutation. |
| 7 | Drop a command attachment after its side effect, then reconnect. | Reattach to the existing command or report uncertain outcome. Never automatically start the side effect again. | Append a unique marker once; interrupt transport after acknowledgement and count markers after reconnect. |
| 8 | Review the diff and run verification. | Show only this task's changes and actual test output. Recheck current permission before any authorized push. | Exact diff, test result and a separate permission-revocation test. |

Steps 5–7 describe required future workspace/backend behavior, not capabilities proved by the current compaction work. A supervisor restart additionally requires application conversation recovery; restoring a guest cannot restore an agent loop running outside it.

[Existing steer semantics](runner_fleet.md#steer-kill-pause) queue a human message for the next run. The supplied Claude reference observes consumption during the current agent task. Adopting a sandbox backend does not change that product behavior. 

Test the existing ordering first; changing it requires a decision about partial tool effects, ordering, interruption and model-request cancellation.

Keep these recovery units distinct:

| Recovery unit | Authority and purpose | Limit |
|---|---|---|
| Conversation checkpoint | Application-owned summary and validated source coverage. | Existing runtime notes are lease/branch scoped. A replacement lease needs an explicitly authorized recovery path; an old checkpoint cannot grant access. |
| Workspace checkpoint | Proposed private file state, manifest, base commit and generation. | It protects only the last successfully published durable generation. A hard crash before that boundary can lose later writes. |
| Resident pause | Backend keeps stopped compute and local files. | It consumes resources and does not establish host-loss durability. |
| Machine snapshot | Backend saves memory, runtime state and disk according to its lifecycle. | It can contain stale credentials, sockets and ownership state; restore requires current authorization. |
| Git push or artifact export | Explicit publication outside the temporary execution environment. | A Git push excludes untracked files, dependencies and process state. |

SPACE distinguishes pause, which keeps a resident guest, from suspend, which saves guest state.
It warns that acknowledged file writes may be lost before its lifecycle saves complete.

Reflect those limits in our user-visible save status.
See upstream [saved state](https://github.com/perplexityai/SPACE/blob/20d321385dac0855b39f86cd93c008ff5ec4ce3c/docs/architecture.md#lifecycle-and-persistence).

### Conversation recovery working hypothesis

The owner's working expectation is to recover the conversation and continue from its context, as during compaction.
Treat conversation continuity as the primary recovery requirement while the remaining boundaries are clarified.

Worker loss also requires checking the files available to the replacement runner.
A summary can describe a completed edit without retaining its exact bytes.
A command may have changed an external service even when its result never reached the conversation.

The recovery design must distinguish conversation progress, available file state and confirmed command outcomes.
For the proposed coding scenario, retain saved uncommitted work and local commits across ordinary lease handoffs.
Conversation context alone cannot replace those bytes; exact save coverage, recovery frequency and retention duration still need a decision.

An uncertain external effect requires verification before any retry.

Current [runtime identity](../../rustd/crates/afr_agent/src/loop/context/mod.rs) binds notes to a lease and branch.
Cross-worker conversation recovery needs an authorized handoff and retained source history; compaction alone does not supply that path.

### Coding sessions and worktrees across leases

The proposed coding session owns a task's conversation and repository workspace across several leases.
A lease gives one runner temporary authority to work on that session; several model and tool turns can happen within one lease.
Renewing a lease or compacting context must preserve that session's working files.

Identify the session by its owning workspace, fleet, task or incident, and repository.
Resuming incident A selects A's saved workspace; diagnosing incident B selects a separate workspace when local code tools are required.
The session records work and recovery state, without assigning workers or replacing fleet scheduling.

```text
Incident A / coding session A / private Git workspace A
      lease 1 -> saved generation 1 -> approval wait
                                      |
      lease 2 <-----------------------+ restore or verified reattach
      lease 2 -> saved generation 2 -> tested patch

Incident B / coding session B / private Git workspace B
```

The current [workspace implementation](runner_execution.md#workspace-between-leases) keeps only a temporary fleet-scoped hold; durable save and restore are not built.
Its [hold key](../../rustd/crates/afr_supervisor/src/holds/mod.rs) names the fleet, workspace, limits and policy, without incident identity.
The proposed session identity must participate in attachment checks so a same-fleet incident cannot inherit another incident's writable checkout.

At a planned handoff, stop workspace writers before recording a consistent file checkpoint.
Save the base commit, local commits, staged and unstaged edits, declared untracked files and the metadata needed to reconstruct the repository.
Upload complete checkpoint content before publishing its manifest and current generation under the live lease's authority.

Linked Git worktrees refer to administrative data outside their working directory, as the [Git worktree details](https://git-scm.com/docs/git-worktree#_details) explain.
Copying that directory alone can leave broken references after a host change.
The save format must retain required Git objects and state, or reconstruct a private checkout with equivalent contents and index state.

Record which conversation progress and command outcomes correspond to the saved workspace generation.
A new lease checks current authority and resumes that recorded point; it cannot reuse expired credentials or another lease's write permission.
Use current lease fencing and workspace generation checks to refuse stale writes and checkpoint publication.

The proposed daemon-owned recovery record selects the task, source history, saved files and confirmed command results together.
Fleet memory can retain shared lessons, but cannot select another incident's files or grant repair authority.
Keep current lease-scoped compaction checks until a verified handoff supplies the continuing task's recovery state.

The runner may reuse compatible compute or release it after a confirmed durable save.
Ending a lease, serving incident B or waiting for approval must not delete the only recoverable copy of A's saved work.
Warm attachment is an optimization; recovery must also work after the old worker disappears.

SPACE's resident pause and durable suspend distinction is useful here; select its save, publication and cleanup mechanisms individually.
An adopted mechanism must satisfy our file recovery rules, regardless of whether it saves a filesystem or a complete guest snapshot.
Restoring files does not promise that background commands are still running.

If a planned save fails, mark the session as not durably saved and do not report a recoverable handoff.
Retain the working copy where possible; a hard crash can still lose changes newer than the last completed checkpoint.
Show the saved generation and any recovery loss instead of claiming the latest work survived.

An authorized deletion or a declared retention policy removes durable session data.
An idle compute timeout can release compute while retaining the session's saved work.
Retention duration, retained file coverage and checkpoint frequency remain open; an unfinished task must not silently become an expired cache entry.

The existing [three-snapshots-per-fleet target](runner_execution.md#workspace-between-leases) cannot protect several unfinished sessions without further rules.
The proposed policy protects each retained session's referenced generation, with explicit session, byte, generation and time limits.
Those limits, refusal behavior and expiry notices require owner decisions; this proposal does not amend the existing target silently.

Extend the [coding walkthrough](#walkthrough-to-build-and-test) with an A-to-B-to-A handoff after hold expiry and replacement of the worker.
Verify local commits, index state, file hashes and untracked files; inject a failed save and a stale lease attempting publication or deletion.
These are required acceptance proofs for the proposed integration, not capabilities already supplied by the current runner or compaction work.

## Repository revisions across events

These proposed sandbox walkthroughs extend existing scenarios; their current behavior remains owned by those pages.
The [pull request reviewer](scenarios/github-pr-reviewer.md) and [production deployment repair](scenarios/production-deploy-repair.md) provide the starting points.
[Repeated review](lease_flow.md#5-the-pr-is-fixed-and-pushed-again) already describes a laptop push waking the same fleet through a new event.

Fleet identity, conversation continuity and repository revision have separate lifetimes.
A continuing conversation must identify the exact code revision it describes.
A cached sandbox cannot establish that its files match a new event.

### Review a pull request after another laptop push

```text
Branch A at commit 1 -> review commit 1 -> findings
           |
Author edits locally and pushes commit 2
           |
Signed synchronize event -> next fleet run -> review commit 2
```

Keep the existing same-fleet queue ordering; the next event waits for the running lease to finish.
Before review, resolve the currently open pull request and its authorized repository, base and exact head commit.
Record the event's advertised head separately from the commit actually selected for review.

For review requiring local tools, the trusted supervisor fetches and materializes that exact commit in a private checkout.
Verify the checked-out commit before any tool runs; never merge a remote branch into an older review workspace.
An HTTP-only review can read pinned code and diffs without starting a sandbox.

Treat previous findings as review history scoped to the repository and pull request, with the reviewed commit recorded.
Recheck each finding against the new code and inspect the complete current change.
An incremental comparison may help, but cannot substitute for a full review after rewritten history or a changed base.

Before posting a result, reread the current head and label which commit the findings cover.
A result for an older commit cannot approve a newer commit.
Read remote review state after an ambiguous post before repeating it.

**Current gaps:** [Review posting is refused](lease_flow.md#4-how-agent-bob-01-can-reply), and prior review continuity across events is not automatic.
The [ingress digest](../../rustd/crates/afd_api_ingress/src/handler/webhook/github/mod.rs) carries the head commit but omits the base commit.
The proposed revision selection and publication checks need implementation and narrowly scoped posting authority.

### Turn production evidence into a patch

```text
Scheduled Grafana scan or failed deployment workflow
           |
Diagnosis + verified environment and relevant commits
           |
Repair proposal in fleet chat or a configured Slack thread
           |
Workspace user approves that proposal
           |
Fresh authority/source checks -> repair run -> private repair checkout
           |
Reproduce -> bounded patch -> tests -> draft pull request
           |
Human merge -> existing deployment pipeline
```

The existing [incident scenario](scenarios/production-deploy-repair.md#1-start-one-incident) keeps the responder and repairer independent.
A diagnosis does not automatically wake the repairer; a failed workflow or human steer starts its run.
The owner proposes an explicit human handoff: show the completed diagnosis and repair proposal, then wait for approval before producing a patch.

For this proposed incident flow, a failed workflow supplies evidence and starts diagnosis; it does not approve patch generation.

The repairer rereads evidence and verifies the repository, environment, workflow and relevant commit before changing code.
A failed workflow commit may never have reached production.
The latest `main` commit may differ from both the failed build and the deployed revision.

Reproduce against the affected revision when evidence allows, then compare it with a freshly verified `main` revision.
If the issue remains, create a private repair branch from that recorded target revision and test the fix there.
Ambiguous deployment identity or a failure outside code ends diagnosis-only, following the existing scenario.

Keep the approved repository, target base and daemon-issued repair branch as publication boundaries.
Preserve the existing [remote reconciliation](scenarios/production-deploy-repair.md#3-diagnose-and-open-one-draft-pr) before retrying a branch or draft creation.
Existing repair publishes Git objects through bounded HTTP requests; publishing a locally tested sandbox patch still needs a trusted implementation.

If `main` advances during work, compare the patch against its new head before presenting it as applicable to current `main`.
Keep any validated patch and its original base available; do not silently overwrite local work.
A changed base requires renewed testing and current permission checks.

Human merge and the repository's deployment pipeline retain their existing roles.
[Incident linkage and post-deployment verification](scenarios/production-deploy-repair.md#4-keep-preview-evidence-without-calling-it-production) remain unimplemented.
A successful workflow alone cannot certify that the original production symptom is resolved.

### Continue an existing repair draft

Current write leases can correct their own event-derived branch with an exact, non-force PATCH.
That same-event operation does not implement the later-admission behavior below.
See [the current write boundary](lease_flow.md#3-github-pr-reviewer-end-to-end).

**Owner-selected behavior:** Later approved fixes for the same incident continue its existing draft pull request, with fresh permission and code checks.
Keep the incident, coding session, repair attempt, repository, branch and draft relationship in trusted application state.
Each new admission receives current authority for that recorded attempt; conversation text cannot select a publication target.

```text
Incident A -> approved repair -> saved workspace A -> branch A -> draft A
                                                      ^           ^
Later approved fix -> fresh lease -> restore A --------+-----------+

Incident B -> separate workspace, branch and draft
```

Before continuing, verify that the recorded draft remains open in the approved repository and uses the recorded branch and target base.
Read the remote branch head and compare it with the saved workspace generation and last confirmed publication.
Preserve local work while incorporating any verified remote changes, then test the exact result intended for publication.

If a person updates that branch from a laptop, fetch those changes before preparing the next patch.
Reject an update if the remote head changed after the publication precondition was established.
The trusted publication path must enforce that precondition without overwriting another writer's work; choosing the transport remains detailed design work.

Use current repository grants and narrowly scoped authority for the recorded branch and draft on every continuation.
If the evidence, repository or intended change falls outside the approved proposal, refresh the proposal before patch work.
For a closed or merged pull request, stop continuation and request an explicit next action.

After an uncertain publication, inspect the recorded branch and draft before retrying.
Repeated channel responses resolve to the same approved action and admission.
Each later approved action remains distinct while continuing the same repair attempt.

**Current implementation gap:** [Repair branches](../../rustd/crates/afd_gate/src/policy/repair.rs) derive from an admission's event.
The [write rules](../../rustd/crates/afd_gate/src/policy/egress/write.rs) allow creation and non-force updates only on the current event-owned repair branch.
The selected design therefore needs a trusted continuation mapping and bounded update authority; restoring files alone cannot supply either.

Required proof includes a new admission resuming the same draft after worker replacement and a laptop push racing publication.
Also test duplicate approvals, lost update acknowledgements, revoked access, changed proposal scope and a merged or closed draft.
Incident B must retain its own workspace and publication targets throughout those cases.

### Workspace approval in chat or Slack

Present the diagnosis, evidence references, affected environment, repository, target revision, intended fix and planned tests before asking to start patch work.
The proposal identifies the repair fleet and whether its scope includes opening or continuing the recorded draft pull request.
Keep that proposal available after the diagnosis run ends; waiting for the owner must not keep a worker or guest occupied.

Anyone with current access to the fleet's workspace can approve its repair proposal.
This includes the account owner and invited members whose access has been granted, under existing capability and workspace checks.
Use the existing [workspace authorization](../AUTH.md#memberships-and-roles); no separate approver role or per-fleet list is required.

An authorized person can approve in fleet chat or address the repair fleet in its configured Slack thread.
An illustrative Slack reply is `@agentsfleet repairer fix this`, with the actual bot and fleet names supplied by that workspace.
The [existing mention routing](scenarios/slack-incident-responder.md#4-routing-a-mention) already requires an addressed request for a write-bound fleet.

A diagnosis started from Slack uses its recorded reply thread.
A scheduled diagnosis needs a configured notification destination before its proposal can appear in Slack.
The [existing reply path](scenarios/slack-incident-responder.md#6-how-the-answer-comes-back) keeps that destination outside model control.

Both channels must resolve the same recorded proposal and admit at most one repair run for that approval.
Repeated clicks, retried Slack events and a second answer through the other channel must return the existing decision.
Record approval and repair admission together, so an acknowledgement cannot leave approved work without a queued run.

Resolve a typed approval or `fix this` against the exact proposal, fleet and conversation or Slack thread.
If several proposals could match, request the intended proposal instead of guessing.
Quoted thread content, a model summary or an unrelated mention cannot approve work.

Verify the responding person's current access to the proposal's workspace before recording approval.
Recheck that access and current repository grants before queued repair starts.
Record who answered and which proposal and workspace the decision covers.

The [mention handler](../../rustd/crates/afd_api_ingress/src/handler/mention/mod.rs) resolves team, channel and fleet; routing alone cannot establish workspace access.
Slack approval needs a verified link from the responding Slack user to the person whose workspace access is checked.
Channel presence, display names and quoted messages cannot substitute for that identity check.

The Slack identity link and shared repair proposal record remain proposed work.
Keep existing repository grants as the access ceiling; this proposed work approval cannot widen tools, repository reach or publication rights.

On approval, recheck the diagnosis and selected revision before preparing a fresh repair run under current authority.
If changed evidence or code invalidates the approved plan, refresh the proposal and request a new approval before patch work.
Denial, expiry, deletion or revoked authority must prevent a new repair admission.

The approved work is the stated patch, tests and any explicitly included draft creation or continuation.
Merging and deployment retain their separate authorities.
The proposal is a work handoff and does not add a new scheduler or sandbox control plane.

### Several incidents under the same fleets

Keep the configured responder and repairer fleets across outages.
Each distinct outage gets a proposed incident record and its own conversation, evidence, repair proposal, approval and result.
Approval for one incident cannot start work for another.

One monitoring fleet handles diagnosis runs serially.
If diagnosis A occupies its active run, diagnosis B waits until that run finishes.
Separate incident conversations preserve context but do not add concurrent capacity to that fleet.

The fleet can receive more work while its diagnosis run is active.
Existing [admission](lease_flow.md#admission-commits-the-queue-row) records each accepted job before acknowledging it; pickup waits for the fleet's active slot.
The [backlog budgets](datastore_scaling.md#the-admissions-ledger-is-the-queue) bound acceptance, with a retryable refusal when capacity is exhausted.

```text
Diagnosis A running + alerts B and C arrive -> A keeps running; B and C queue
Diagnosis A finishes                       -> B can start; C stays queued
```

Queued work does not interrupt diagnosis A or create another active run for that fleet.
Durable arrival handling already exists; grouping those alerts into incidents and recovering each incident's conversation remain separate proposed capabilities.

Further evidence from the same ongoing outage updates that incident.
Use provider alert identity, service, environment and deployment evidence when deciding whether symptoms belong together; matching error text alone cannot merge incidents.
Keep uncertain cases separate, and link a recurrence after confirmed resolution to the earlier incident.

Logs continue growing in the observability provider.
Each bounded monitoring sweep reads scoped evidence windows and saves progress after recording the relevant evidence.
Keep evidence references and bounded excerpts in each incident conversation instead of appending the whole log stream.

After recording a diagnosis and proposal, finish the responder admission and release its lease.
Waiting for approval must not park the responder's queue head, which can [block later admissions](../../rustd/crates/afd_fleet/src/lease/sql/lease/select_ledger_candidates.sql).
The saved proposal waits independently; approval admits the configured repairer's work.

```text
Responder sweep -> incident A -> proposal A -> member approves -> repairer run A
Responder sweep -> incident B -> proposal B -> member approves -> repairer queue B
More A evidence -> incident A
```

The responder can handle another outage while the separate repairer works on the first.
The [one active lease per fleet](data_flow.md#one-active-lease-per-fleet--the-ownership-model) rule still bounds each fleet.
A busy responder delays its next sweep; scheduled monitoring does not promise continuous log tailing.

For this proposal, another approved repair queues while the same repairer is busy.
Separate, explicitly configured repairer fleets can work concurrently; an outage does not create a fleet by itself.
Every repair needs its own checked source revision and private writable workspace.

Evidence grouping is separate from repeated webhook delivery detection and does not replace existing admission ordering.
Incident records, grouping rules and incident-scoped recovery remain proposed work, alongside the shared repair approval record.
Monitoring delay, evidence limits and repair queue capacity still need explicit budgets before implementation.

First set the maximum acceptable delay from an accepted critical alert to the start of its diagnosis.
If serial diagnosis cannot meet that bound, revisit fleet concurrency explicitly before choosing a sandbox mechanism.

### Several monitoring bots and their triggers

Several monitoring fleets are a supported arrangement; one global monitoring fleet is not required.
Each separately saved monitoring bot is a fleet with its own setup, triggers, permissions, budget and queue.
The [existing two-fleet scenario](scenarios/two-fleets-github-slack.md#6-pickup-two-runners-two-leases) shows independent fleets running concurrently when worker capacity and permission checks allow.

| Monitoring arrangement | Work admitted and run |
|---|---|
| Several alerts target one fleet | Separate accepted jobs enter that fleet's queue; diagnosis runs happen one at a time. |
| Different alerts target different fleets | Each fleet receives its own jobs and can run concurrently with the others. |
| The same alert targets several fleets | Each targeted fleet can receive a job; several diagnoses may describe the same outage. |

For example, a payments monitor and a database monitor can diagnose their respective outages at the same time.
If both subscribe to the same broad alert feed, both can wake for one outage.
Another outage does not create another fleet or notify every existing fleet by itself.

The [existing incident scenario](scenarios/production-deploy-repair.md#1-start-one-incident) describes scheduled Grafana reads.
Each due schedule wakes its own fleet, even when another monitoring fleet is already running.
Reading Grafana with a credential does not itself create an incoming alert subscription.

For an alert-driven setup, define which approved alert route targets which monitoring fleet before promising selective wakeups.
The [generic webhook route](../../rustd/crates/afd_api_ingress/src/handler/webhook/source_route.rs) names one fleet and source; it does not infer alert kinds from arbitrary payloads.
Grafana alert validation, routing and incident grouping need their own design and delivery proof.

The proposed default is to assign a responsible monitor for each service and environment.
Additional monitors can contribute evidence, but cross-fleet incident ownership must be explicit before their diagnoses can share one repair proposal.
Existing delivery retry detection does not establish shared incident identity across fleets.

### Checkout freshness and proof

Current [held sandbox reuse](../../rustd/crates/afr_supervisor/src/lease_loop/workspace.rs) skips checkout and retains the previous files.
Fresh [materialization](../../rustd/crates/afr_supervisor/src/workspace_clone/checkout.rs) selects a branch tip rather than an event-pinned commit.
The [sandbox Git tool](../../rustd/crates/afr_tools/src/sandbox/git/mod.rs) refuses remote operations, so trusted preparation must own refresh.

The proposed preparation path must verify the selected commit even when compute is reused.
Use a fresh private checkout or a verified reusable view; preserve or refuse an incompatible dirty workspace.
Reuse an authorized immutable source cache while keeping writable files private to the run.

Required proofs include a push during review, rewritten branch history, a changed base and a dirty held workspace.
Also inject duplicate deliveries, lost post acknowledgements, revoked repository access and a fork outside approved reach.
For repair, test a failed build that never deployed, `main` advancing, ambiguous evidence and an already-created remote draft.

Record the reviewed, affected, patch-base and published commits independently in every result.
For proposal approval, test simultaneous chat and Slack replies, unauthorized responders, ambiguous threads, expired proposals and a changed diagnosis.
Verify that account owners and invited members with workspace access can approve the same proposal through either supported channel.

Reject a person from another workspace, an unlinked Slack identity and a queued repair whose approver has lost access.
Restart after approval but before pickup and verify that the same repair admission remains available without creating another patch run.

For overlapping incidents, prove another outage remains discoverable while the first waits for approval or occupies the repairer.
Send several alerts while diagnosis A is active; verify accepted jobs remain queued and run within the chosen start-delay budget.

Inject repeated alerts, late evidence, an approval naming the wrong incident and worker loss between evidence recording and progress updates.
Verify bounded conversation input, separate incident recovery and progress through the repair queue.

## Decisions raised by the independent concept review

The Oct 25, 2026 Elephant-Goldfish Model (EGM) review examined the proposal through six independent lenses.
The review covered ownership, cost, usability, operations, migration and maintenance.
Its findings concern this concept; they do not replace the unresolved implementation review for pull request 784.

The following decisions and implementation gaps remain relevant.
The proposed daemon recovery owner above supplies a boundary, while its detailed storage and handoff design remains unwritten.

| Decision | Required behavior or unresolved choice | Evidence |
|---|---|---|
| Select and resume a task | Chat and Slack must identify incident A after B without guessing from conversation prose. Bind its conversation coverage, file generation and command results together. | [Current lease-scoped context](../../rustd/crates/afr_agent/src/loop/context/mod.rs), [fleet-scoped holds](../../rustd/crates/afr_supervisor/src/holds/mod.rs). |
| Bound retained work | Set saved file coverage, acceptable crash loss, save/restore deadlines, retention during approval waits and storage limits. Protect each retained session's referenced generation. | [Existing retention target](runner_execution.md#workspace-between-leases) and [proposed session lifecycle](#coding-sessions-and-worktrees-across-leases). |
| Continue an approved repair | Owner selected the same existing draft for later approved fixes to the same incident. The trusted attempt mapping, current authority and publication precondition still need implementation. | [Selected continuation behavior](#continue-an-existing-repair-draft), [event-derived branch](../../rustd/crates/afd_gate/src/policy/repair.rs) and [event-scoped write rules](../../rustd/crates/afd_gate/src/policy/egress/write.rs). |
| Identify the approver | Owner selected existing workspace access, including invited members, without a separate approver list. Verified Slack identity and adoption across direct requests, queued work and running leases still need design. | [Workspace roles](../AUTH.md#memberships-and-roles), [existing direct Slack request](scenarios/slack-incident-responder.md#7-four-stages-four-authorities), [proposed approval flow](#workspace-approval-in-chat-or-slack). |
| Discover and deliver proposals | Keep saved proposals discoverable when Slack notification fails. Track saved, delivered, uncertain and abandoned outcomes separately; retry the same proposal identity. | [Existing answer delivery](scenarios/slack-incident-responder.md#6-how-the-answer-comes-back) and [outbound abandonment](../../rustd/crates/afd_outbound/src/abandon.rs). |
| Bound monitoring delay | Set critical-alert diagnosis delay and repair queue capacity. Track successful sweep freshness separately from accepted-alert queue time, including failed provider reads and evidence gaps. | [Ordered queue without a waiting-time guarantee](datastore_scaling.md#the-admissions-ledger-is-the-queue), [serial diagnosis](#several-incidents-under-the-same-fleets). |
| Report recoverable progress | Show which files were saved, which tests cover that generation and which command outcomes remain unknown. Give a verification step before retrying an uncertain effect. | [Command reconnect limits](#selective-space-reuse), [proposed recovery units](#walkthrough-to-build-and-test). |
| Upgrade and roll back | Choose whether adoption starts with new sessions or imports verified in-progress work. Retained saves need compatible readers or a validated conversion before older workers resume them. | [Current lease payload](../../rustd/crates/afd_wire/src/lease/payload.rs), upstream [versioning](https://github.com/perplexityai/SPACE/blob/20d321385dac0855b39f86cd93c008ff5ec4ce3c/docs/versioning.md). |
| Choose bounded source reuse | Owner selected a small SPACE-derived Firecracker wrapper with the existing executor. Exact extraction, dependency closure and real Linux measurements still need proof. | [First wrapper](#first-firecracker-wrapper), [local atomic report publication](../../rustd/crates/afr_supervisor/src/report_spool/mod.rs), [reuse criteria](#selective-space-reuse). |

Guest resource limits do not bound supervisor compression, upload buffers or temporary storage.
The integration needs host-wide save/restore admission, transfer limits, deadlines and cleanup reserve alongside per-session storage limits.
Upstream [streaming upload](https://github.com/perplexityai/SPACE/blob/20d321385dac0855b39f86cd93c008ff5ec4ce3c/crates/libs/vol/mgr/server/src/executor/stream_push.rs) supplies a shared-buffer approach to evaluate.

Treat sandbox commands, durable task recovery and incident approval as separate delivery slices with explicit integration proofs.
Keep every scenario in the design, while selecting one bounded first integration.
File checkpoints and full guest snapshots carry different compatibility costs; running-process restoration remains an unresolved scope choice.

## Refactor order and files

| Slice | Files or existing area | Change and proof |
|---|---|---|
| Shared preparation | `rustd/crates/afd_context/src/{plan,ports}.rs`, [preparation work tests](../../rustd/crates/afd_context/tests/work.rs) and existing planner tests | Reduce cumulative estimates, retain a final complete provider estimate and exact output safety, and reject stale source. A blocked summary must not block another conversation. |
| Runtime history | `rustd/crates/afr_agent/src/loop/{mod,conversation}.rs`, `loop/context/{mod,source,estimate}.rs`; proposed history-index module | One owned encoded-history index and borrowed views; invalidate the affected tail when pending input changes. Preserve complete tool groups and native continuation rules. |
| Authoring preparation | `rustd/crates/afd_authoring/src/model/context/`, `model/dispatch/{mod,funding}.rs`, existing funding/source-change integration tests | Reuse prepared input only with an exact source/authority witness after funding. Funding and model access stay outside datastore locks. |
| Repository preparation proposal | `rustd/crates/afr_supervisor/src/{lease_loop/checkout.rs,workspace_clone/}`, related checkout and concurrency tests | Separate trusted repository acquisition from local or remote materialization. Preserve credential isolation; publish an immutable source before shortening mirror lock duration. |
| Native backend experiment | Existing `afr_sandbox` engine/boot and `afr_executor` boundaries; proposed guest engine informed by SPACE | Map readiness, process identity, bounded streams, cancellation, network policy, quota and cleanup. Unsupported semantics must refuse explicitly. |
| Recovery proposal | Existing `afr_supervisor` held-sandbox and lease paths, plus a manifest/storage owner to be selected with the persistence mechanism | Bind durable workspace generation to current fleet authority. Test host loss, source changes, deletion and revoked credentials. No new scheduler. |

Do not extract a checkpoint-storage crate solely to anticipate SPACE. The [current optional PostgreSQL feature](../../rustd/crates/afd_context/Cargo.toml) already excludes that dependency from runtime builds. Extract only if a concrete caller or dependency boundary improves.

## Selective SPACE reuse

Copying source makes us responsible for its behavior, dependencies and future fixes.
The [supervisor manifest](https://github.com/perplexityai/SPACE/blob/20d321385dac0855b39f86cd93c008ff5ec4ce3c/crates/libs/node/runtime/supervisor/Cargo.toml) already depends on shared service, runtime and core crates.
Reading one launcher file is insufficient to establish the cost of adopting it.

Each proposed reuse must name:

- The observed problem, selected source revision and intended local boundary.
- Every additional dependency and whether to retain, replace or remove it.
- The owner of admission, runtime state, cleanup and durable publication.
- The behavior and failure proofs that must survive adaptation.
- Preparation, launch, command, save and recovery measurements relevant to the change.
- Source provenance, applicable notices, local modifications and the upstream-fix review process.

Control requires measured behavior and maintainable boundaries; copying more code does not establish either.
Choose copying, adaptation or a fresh implementation separately for each mechanism after that review.

| Source at inspected revision | Useful mechanism | Integration edge |
|---|---|---|
| Upstream [execution identity](https://github.com/perplexityai/SPACE/blob/20d321385dac0855b39f86cd93c008ff5ec4ce3c/crates/libs/node/runtime/supervisor/src/service/execution.rs) and [durable store](https://github.com/perplexityai/SPACE/blob/20d321385dac0855b39f86cd93c008ff5ec4ce3c/crates/libs/node/runtime/supervisor/src/execution_store.rs) | Fenced execution identity, quiescence checks and durable atomic publication. | Keep our fleet lease fence as well; a sandbox execution generation is a different authority. |
| Upstream [command streams](https://github.com/perplexityai/SPACE/blob/20d321385dac0855b39f86cd93c008ff5ec4ce3c/crates/libs/data/plane/server/src/native/streams.rs) | Derive process identity from session identity and reconnect after an ambiguous start. | Completed-process retention is bounded. Missing retention cannot authorize an automatic rerun. |
| Upstream [Firecracker launcher](https://github.com/perplexityai/SPACE/blob/20d321385dac0855b39f86cd93c008ff5ec4ce3c/crates/libs/node/runtime/supervisor/src/service.rs) | Owned launch, bounded blocking work and cleanup after partial launch/publication failures. | The launcher depends on runtime, admission, process and storage support. Audit that dependency closure before copying. |
| Upstream [PostgreSQL contention classification](https://github.com/perplexityai/SPACE/blob/20d321385dac0855b39f86cd93c008ff5ec4ce3c/crates/libs/postgres-contention/src/lib.rs) | Central classification of serialization, deadlock and lock-availability errors. | Retry an eligible transaction with bounded policy; never blindly replay external side effects. |
| Upstream [benchmark methodology](https://github.com/perplexityai/SPACE/blob/20d321385dac0855b39f86cd93c008ff5ec4ce3c/scripts/benchmarks/README.md) | Cross-worker recovery checks, failure-inclusive latency and bystander measurements. | Extend the measured path through checkout, setup and useful tool work. |

Inspect each copy against upstream revision `20d321385dac0855b39f86cd93c008ff5ec4ce3c`. Record provenance, retain applicable notices and mark modifications under the upstream [license](https://github.com/perplexityai/SPACE/blob/20d321385dac0855b39f86cd93c008ff5ec4ce3c/LICENSE). The comparison has not copied SPACE code.

Keep session identity separate from attachment credentials: SPACE rotates attachment credentials on idempotent replay, and its stronger old-credential validity expectation remains unsettled. See [native recovery limits](https://github.com/perplexityai/SPACE/blob/20d321385dac0855b39f86cd93c008ff5ec4ce3c/docs/native-api-hardening.md).

## Performance and failure measurements

The fitting-history work test counts groups passed to estimators, not tokens, physical requests or actual elapsed time. For 40 pages of 64 groups, repeated cumulative estimates visit `64 × (1 + 2 + ... + 40) = 52,480` groups for 2,560 source groups. 

It exposes repeated work. It does not quantify a throughput gain or a provider bill.

Target bounded estimator visits in that fixture, then measure elapsed preparation time and allocations separately. Geometrically spaced probes must never send unchecked input; a complete provider estimate still precedes dispatch. An owned immutable prefix may reuse verified history evidence; an arbitrary stored watermark is insufficient.

Repository performance needs its own comparison. The [mirror lock](../../rustd/crates/afr_supervisor/src/workspace_clone/mod.rs) currently covers fetch and checkout; [materialization](../../rustd/crates/afr_supervisor/src/workspace_clone/checkout.rs) copies the mirror's objects. 

Benchmark many checkouts of one repository separately from independent repositories. Moving the lock without making the source immutable can introduce inconsistent checkouts.

Measure request-to-checkout-ready, first useful command, verified diff, durable save and cross-worker resume. Report model wait separately from local preparation, queueing, Git transfer, dependency installation and snapshot transfer. Compare cold and warm environments at concurrency 1, 10 and 100, with equal repository state, hardware, limits and cache policy.

Report median, 95th and 99th percentiles, failure counts, memory, processor time, disk bytes, storage transfer and bystander latency. Include failures in latency reporting rather than dropping them. Adopt a candidate only after the same correctness workload passes and its measured latency/cost fits the product budget; there is no measured adoption winner yet.

Inject cancellation, slow consumers, dropped acknowledgements, duplicate starts and worker death before and after save.
Also test revoked repository access, stale lease writes, full byte/inode quotas and cache poisoning.

Reserve capacity for cleanup and control records, then verify actual recovery at capacity. Cache identities must include the appropriate tenant scope and build inputs; untrusted setup must not publish trusted shared cache state.

## Evidence and implementation boundary

Run repository claims through existing Make targets: `make lint-all`, `make test-unit-all`, `make test-integration-rustd`, `make check-version`, and the unchanged `make acceptance-local`. Use `make test-runner-kernel` for the implemented Linux sandbox; it does not prove Firecracker or SPACE behavior. A selected remote backend needs a distinct real-backend workload wired into the appropriate Make lane before support is claimed.

Keep failure injection deterministic with barriers, controlled transport loss and synthetic repository contents. Preserve source hashes, process markers and sanitized stage timings. Clean up only the run's own resources.

A browser mock, scripted provider or successful service creation cannot substitute for a real cross-worker recovery proof.

The historical pre-optimization compaction regression failed while the independent blocked-summary proof passed. The [preparation record](context_lifecycle.md#preparation-and-future-sandbox-boundary) now describes indexed preparation and its estimator-work regression. Use the owning spec's measurements rather than the historical failure.

Full acceptance remains incomplete, so this evaluation makes no readiness or concurrent-throughput claim.

The [proposed original-Pull-Request continuation](lease_flow.md#original-pull-request-repair-proposed-follow-on) records the requested existing-PR flow and future recovery proofs.
M224 starts with the manually triggered fleet-owned draft demonstration; the broader runtime behavior remains unimplemented.
