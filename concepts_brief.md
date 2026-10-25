# Firecracker sandbox informed by SPACE

* **Author:** Aiwa, based on Kishore's inputs.
* **Date:** Oct 25, 2026
* **Status:** Working hypothesis refined after the completed six-lens Elephant-Goldfish Model (EGM) review. First-build scope is narrowed; runtime proof remains outstanding.

## Overview

Build a small `agentsfleet`-owned Firecracker sandbox using SPACE (`Sandboxed Platform for Agentic Code Execution`) design and useful source.
Reuse the existing runner and command executor.
Prove the repository and incident scenarios on that backend, then adopt startup optimizations that improve measured results.

```text
Existing fleet admission and agent loop
                    |
Existing sandbox engine interface
                    |
SPACE-derived Firecracker driver
                    |
Read-only guest tools + private task files
                    |
Existing executor -> commands -> tested diff
```

The [first-build plan](docs/architecture/sandbox_evaluation.md#first-firecracker-wrapper) owns the source map, build order and evidence requirements.
The owner authorizes adapting SPACE source and wants the simplest design that can test the agreed scenarios.
No SPACE code or native engine has been implemented by this brainstorm.

## Analysis

### Reuse what already solves the problem

Start with SPACE's jailer launch, Firecracker API client, process ownership and cleanup behavior.
Keep useful upstream tests and record the inspected revision and copied dependencies.
Our existing executor, agent loop, repository authority and fleet admission remain the integration foundation.

The adversarial question is whether extraction saves work after accounting for guest startup, transport, storage, networking and updates.
A launcher file alone does not supply a working agent sandbox.
The [reuse map](docs/architecture/sandbox_evaluation.md#reuse-before-adding-another-subsystem) explains where existing code fits and where proof is still needed.

### The smallest useful build

Boot one pinned guest image on a qualified Linux host, connect the executor, run a command and stop the guest completely.
Then prepare a private repository disk, edit code, run tests and return the exact diff.
The trusted runner owns repository credentials and verifies the selected commit.

Add save and restore of declared task files, Git state and conversation progress across a fresh lease.
The first recovery proof starts fresh guest compute and verifies interrupted command outcomes before retry.
It does not require restoring running-process memory.

Compare cold boot with SPACE's clean-template snapshot approach, then consider an idle pool if measurements justify its resource cost.
Keep reusable startup templates separate from task checkpoints.
A completed task save protects declared bytes; a hard crash can lose newer unsaved work.

### Keep the agreed scenarios

| Scenario | Required behavior |
|---|---|
| Laptop push after review | Fetch and verify the new head and base, retain earlier reasoning, and recheck findings against the new code. |
| Production outage | Distinguish deployed, failed-workflow and current-main revisions; diagnose, propose a fix, obtain approval, test and produce a draft. |
| Chat or Slack approval | Anyone with current workspace access can approve, including invited members. Both channels resolve the same proposal; Slack needs verified account identity. |
| Later approved fix | Continue the same incident's existing draft with fresh permissions, verified remote state and tests. Preserve laptop changes and reject stale publication. |
| Overlapping outages | Accept queued alerts, keep incident context and writable files separate, and release the responder while approval waits. |
| Several monitoring fleets | Preserve one active run per fleet; separately configured fleets can run concurrently under existing capacity rules. |
| Several leases or worker loss | Restore task A after serving B, including saved local commits and declared uncommitted files, without importing B's workspace or authority. |
| Long conversations | Keep shared compaction application-owned and independent of the guest backend; retain original history. |

The [revision walkthroughs](docs/architecture/sandbox_evaluation.md#repository-revisions-across-events) retain the details and current application gaps.
A working guest does not complete review-posting authority, proposal handoff, Slack identity or post-deployment verification by itself.
Run the full scenarios after their required integration is implemented; report partial results explicitly.

### Test speed and failure behavior

Measure guest readiness, repository readiness and the first useful command separately on the same host and workload.
Compare actual cold and snapshot startup modes, including fallback, failed starts, cleanup and save/restore cost.
Firecracker alone does not establish a performance improvement over the current engine.

Use real guests for cancellation, resource-limit, network-denial, command-disconnection and worker-loss proofs.
Protect each retained task's referenced save; fleet-wide cleanup must not evict another unfinished incident's only generation.
The [review decisions](docs/architecture/sandbox_evaluation.md#decisions-raised-by-the-independent-concept-review) retain unresolved production limits and compatibility obligations.

## Goals vs. Non-Goals

* **In-Scope Goals:** A small SPACE-derived Firecracker adapter, existing executor integration, private repository work, declared file recovery and measured startup behavior.
* **In-Scope Goals:** Use the agreed scenarios and injected failures as acceptance evidence, with provenance and maintenance ownership for copied source.
* **Out-of-Scope Non-Goals:** Operating the full SPACE platform, creating another scheduler, redesigning workspace membership or building another agent protocol.
* **Out-of-Scope Non-Goals:** Claiming process-memory recovery, production readiness or a measured speed winner before the relevant proof exists.

## Key Decisions & Staging for Next Steps

The first build is a small Firecracker wrapper informed by SPACE source, behind the existing runner interfaces.
Workspace access controls approval, including invited members.
Later approved fixes for the same incident continue its existing draft under current permission and code checks.

Build in the [recorded order](docs/architecture/sandbox_evaluation.md#build-and-test-in-this-order): guest command, repository edit/test, task recovery, snapshot startup, complete scenario acceptance.
Choose exact source extraction and dependencies during implementation review.
Numerical production budgets and retention remain open; collect baseline measurements without inventing service guarantees.

Shared compaction remains independent, and compaction must finish before Billing.
This concept review does not reopen or replace the terminal implementation review recorded for pull request 784.

The owner requested this documentation in pull request 784 and sandbox implementation in a separate stream after it merges.
The [design handoff](docs/reviews/space-sandbox/README.md) retains the completed concept review and both agent restart prompts.
Detailed design drafting, a dedicated implementation spec and their EGM reviews remain pending.
