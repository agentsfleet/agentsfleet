# Restart the SPACE-derived sandbox work

Work in the `agentsfleet` repository after pull request 784 has merged.
Treat this as a separate sandbox implementation with its own spec, branch and pull request.
Do not add runtime implementation to pull request 784 or resume its terminal implementation review.

## Establish the starting point

Read the repository's `AGENTS.md`, `.orly/LOCAL.md` and `.orly/AGENTS.md` first.
Verify pull request 784's merge and its merge revision through GitHub before starting implementation.
If it remains open, report that dependency and continue only independent document preparation.

Start from the merged code and verify the current development deployment.
Preserve unrelated dirty work and follow the repository's normal spec and worktree lifecycle.
Do not rewrite history, weaken gates or assume a historical passing check covers the new candidate.

Read these inputs:

- `concepts_brief.md` for the current direction and accepted owner decisions.
- `docs/architecture/sandbox_evaluation.md` for source reuse, first-build order, recovery and incident scenarios.
- `docs/reviews/space-sandbox/README.md` and `review-evidence.json` for completed concept review and its frozen inputs.
- `docs/architecture/runner_execution.md` and `runner_fleet.md` for the existing runner, toolbox and leases.
- `docs/architecture/context_lifecycle.md` for shared compaction and checkpoint ownership.
- The existing M230_003 spec, including its compaction verification record and review boundary.
- The relevant repository, Slack incident and production-deploy scenarios under `docs/architecture/scenarios/`.

The Elephant-Goldfish Model (EGM) brainstorm and six fresh concept reviews are complete.
The detailed draft, dedicated sandbox implementation spec and their design reviews are still required.
Do not claim that the later narrowed brief received a completed implementation-readiness review.

## Complete the design handoff

Use `egm:egm-draft` with the existing brief and preserve its required owner decisions.
Use `orly-spec-new` for one dedicated implementation spec with exact files, prerequisites, invariants and behavioral proofs.
Use the design for mechanisms and the Orly spec for implementation scope, test mapping and delivery evidence.

Submit both documents to `egm:egm-review` before runtime implementation.
Give fresh reviewers the companion document, current code, relevant architecture and the completed concept-review findings.
Use the installed skills' real workflow and resolve findings without duplicating competing plans.

Separate the first runnable backend from broader incident-application changes that the backend alone cannot supply.
Do not silently expand the first sandbox spec into a new scheduler, approval service or incident platform.
Keep later scenario dependencies explicit and report partial coverage honestly.

## Implement the smallest useful backend

Use SPACE, `Sandboxed Platform for Agentic Code Execution`, as prior art and reuse useful source and behavior tests.
The inspected upstream revision was `20d321385dac0855b39f86cd93c008ff5ec4ce3c`.
Recheck that revision, its license, copied dependencies and attribution before extraction.

Prefer SPACE's jailer launch, Firecracker API client, process ownership and cleanup where they fit.
Audit the dependency closure instead of importing the full platform or recreating its services under new names.
Keep maintenance ownership and an upstream comparison record for copied or adapted source.

Preserve the Rust runner's existing `Engine`, `Sandbox` and `Executor` boundaries.
Keep model calls, compaction, admission, credentials and permission checks outside tenant code.
Retain bubblewrap for its existing supported environments while testing Firecracker as another backend.

Build and prove this order:

1. Boot a pinned guest, connect the existing executor, run a command and prove full cleanup after success, failure and cancellation.
2. Prepare private repository files, verify the chosen revision, edit code, run tests and return the exact tested diff.
3. Save declared files and Git state, replace guest compute and restore under fresh authority, including task A to B to A.
4. Compare cold startup with SPACE's clean-template snapshot startup, recording actual boot mode and fallback.
5. Test the agreed repository and incident scenarios after their required application integration exists.

Keep reusable startup templates separate from each task's saved work and credentials.
Do not promise running-process memory recovery or survival of bytes written after the last completed save.
Verify uncertain command outcomes before retrying side effects.

## Preserve the agreed scenarios

- A laptop pushes new commits after a fleet review: fetch exact head and base, then recheck prior findings against those commits.
- An outage identifies deployed, failed-workflow and current-main revisions before proposing and testing a patch.
- Current workspace members, including invited members, can approve through chat or a verified Slack identity.
- Later approved fixes continue the same incident's existing draft with fresh permission, revision and publication checks.
- Alerts arriving during diagnosis remain queued; separate incidents retain separate context and files.
- Each fleet has one active lease; separately configured monitoring fleets can run concurrently within capacity limits.
- Returning to incident A after B restores A's saved files, conversation coverage and confirmed command outcomes without importing B's authority.
- Private setup chat works without GitHub, a Git worktree or a runtime sandbox.
- Runtime work with no bound repositories uses private scratch files where needed and performs no repository checkout.
- Long conversations use application-owned shared compaction, independently of the selected sandbox engine.

## Test the real backend

Firecracker requires Linux with Kernel-based Virtual Machine (KVM) access through `/dev/kvm`.
The checked local OrbStack machine lacked that device; adding a container does not establish KVM support.
Use a qualified Linux test host and integrate it with existing acceptance services when that path is proved.

The existing toolbox supplies userland tools; a working guest still needs its kernel, startup entry, disks and command transport.
Use existing image provenance and signature checks where applicable.
Do not claim the current bubblewrap kernel lane proves Firecracker support.

Measure request-to-repository-ready and first useful command, including image staging, checkout and dependency setup.
Use the same hardware, workload, limits and cache policy for comparisons.
Report cold, snapshot and fallback results separately; add an idle pool only if measurements justify its resource cost.

Prove cancellation, resource limits, denied network access, disconnects, worker loss and cleanup on real guests.
Map every spec Dimension and failure case to the repository's required verification lanes.
Keep source inspection, design review, local runtime proof and production support as separate claims.

Finish the normal repository checks and review process, push the separate implementation pull request and hand over its measured evidence.
Request the required merge and release authorization only after the concrete result is ready for review.
