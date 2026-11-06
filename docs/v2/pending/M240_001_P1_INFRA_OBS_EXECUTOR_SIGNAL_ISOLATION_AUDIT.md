# M240_001: Audit tenant-to-executor signal isolation

**Prototype:** v2.0.0
**Milestone:** M240
**Workstream:** 001
**Date:** Nov 04, 2026
**Status:** PENDING
**Priority:** P1 — determine whether tenant commands can stop their executor
**Categories:** INFRA, OBS
**Branch:** specification in existing PR #803; runtime mechanism remains unapproved
**Baseline revision:** 1920ff345
**Test Baseline:** no kernel signal probe has been run
**Baseline evidence:** source references below; no claimed exploit or isolation proof
**Depends on:** the existing Linux runner kernel lane and trusted sandbox cleanup
**Provenance:** Indy's explicit audit request; the causal explanation remains a hypothesis
**Canonical architecture:** docs/architecture/runner_execution.md

## Overview

**Goal (testable):** A real tenant shell cannot stop or signal the trusted executor while ordinary tenant process control still works.
**Problem:** Shared namespace and credential assumptions may allow tenant-to-control-plane signalling; source configuration alone does not prove reachability.
**Solution summary:** Measure actual process identities and signal permissions through the existing Linux kernel lane, then review the smallest demonstrated isolation mechanism.

The runner ending deadline mitigates an unresponsive connection.
It does not establish why the executor stopped or prove signal isolation.
This pending specification authorizes design and inventory; runtime changes require mechanism review.

## PR Intent & comprehension handshake

- **Current deliverable:** This separate audit specification stays in existing PR #803; no new PR is opened.
- **Future intent:** Reproduce or disprove signalling from a tenant command to its own executor, then preserve the required separation.
- **Handshake:** Never infer a successful signal solely from syscall return, and never claim isolation from a mocked executor.

## Implementing agent — read these first

1. `afr_sandbox/src/bubblewrap/mod.rs` — PID namespace, sandbox UID/GID arguments and executor launch.
2. `afr_sandbox/src/harden/` and the executor entry path — exact signal-related seccomp and Landlock rules at each process boundary.
3. `afr_executor/src/server/launch/{mod.rs,tenant/}` — inherited credentials, process groups and tenant cgroup placement.
4. Existing runner kernel tests and `make test-runner-kernel` — privileges, supported-kernel checks and cleanup ownership.
5. `afr_executor/src/client/` and `afr_tools/src/sandbox/oneshot/` — the response-time mitigation, whose proof remains separate.

Rust paths share `rustd/crates/`.
Inspect runtime PID mappings, effective credentials, capabilities and confinement before choosing a remedy.
Namespace PID 1 signal rules must be measured rather than assumed.

## Files Changed (proposed blast radius)

The initial audit belongs in the existing `afr_sandbox` Linux kernel-test owners and their fixtures.
Only after the probe may it change bubblewrap identity mapping, tenant launch placement, or child-specific hardening.
Update `runner_execution.md`, applicable kernel evidence and this specification with observed results.

No credential store, model client, scheduler or Durable Task API changes belong here.
No host-wide filter or broad process-kill command is authorized by this document.

## Applicable Rules

Use the repository error shell and typed native failures.
Use existing namespace/cgroup helpers and kernel-lane Make targets.
Record actual capabilities and unsupported environments; an unrun probe cannot establish success.

## Applicable Gates

`make test-runner-kernel`, changed-crate tests, `make lint-rustd` and the owning milestone's final gates.
No new test harness, privileged bypass or inferred passing baseline.

## Prior-Art / Reference Implementations

Reuse the existing trusted host's sandbox process handle and cgroup cleanup.
Evaluate kernel signal permissions, separate credentials and child-specific confinement using the actual deployed launch shape.
Select among mechanisms only after observing the failed boundary and required process-control behavior.

## Sections (audit and implementation decisions)

### §1 — Establish the process and confinement facts

Capture the executor and tenant shell's namespace/host PID mapping, real/effective user and group IDs, capabilities and cgroup placement.
Identify where each process receives seccomp and Landlock restrictions.
Prove that every target belongs to this test's sandbox before signalling it.

- **Dimension 1.1** — Kernel evidence binds both actors to the intended sandbox → `executor_signal_probe_records_trusted_process_identity`.

### §2 — Run the controlled stop probe

1. The trusted host starts the sandbox and retains its stable process handle.
2. A tenant shell identifies its own executor through the verified namespace mapping.
3. That shell runs the requested `kill -STOP <executor pid>` probe.
4. The host independently observes whether the executor stopped and whether its native channel stopped answering.
5. An external watchdog always resumes the executor and destroys the test sandbox on failure or cancellation.

The host must remain outside the target's stoppable group.
Never substitute a host PID into the tenant namespace without proving the mapping.
Record syscall result, actual process state and connection behavior separately.

- **Dimension 2.1** — A tenant stop attempt has an observed kernel outcome → `tenant_stop_of_executor_is_observed_and_always_cleaned_up`.
- **Dimension 2.2** — Timeout or failed assertions leave no stopped sandbox → `executor_signal_probe_cleanup_survives_interruption`.

### §3 — Inventory equivalent routes and review the remedy

Inspect `kill`, process-group signals, `tkill`, `tgkill` and `pidfd_send_signal`, including capabilities and inherited descriptors.
Check whether separate identity, child-specific signal confinement or another existing kernel boundary closes the demonstrated routes.
Do not deploy an ABI upgrade or a global syscall denial merely from the original hypothesis.

The chosen mechanism must retain ordinary shell jobs, cancellation, process groups, terminals and executor-driven termination.
Bring the measured mechanism and exact source scope to Indy before implementation.

- **Dimension 3.1** — Required tenant process control remains available → `tenant_job_control_survives_executor_signal_isolation`.
- **Dimension 3.2** — Equivalent tenant-to-executor signal routes stay blocked → `tenant_signal_variants_cannot_control_the_executor`.

## Interfaces

Reuse the current sandbox/executor interfaces and stable process handles.
No new public endpoint or permission grant is proposed.
Any kernel requirement or identity change requires an explicit compatibility and activation decision.

## Evidence and open decision

No runtime signal probe has been run for this specification.
The current source establishes launch configuration, not the alleged causal path.
The isolation mechanism remains open until the controlled probe and process inventory are reviewed.

## Discovery

> Indy: "The root cause is still open."

> Indy: "Running kill -STOP <executor pid> from a shell call in the Linux kernel lane would settle it, and that belongs in its own spec."

> Indy superseding the earlier separate-PR instruction: "no new PR"

## Out of scope

Claiming that the ending deadline fixes signal isolation, production probing, broad host process control and unreviewed kernel-policy changes.
