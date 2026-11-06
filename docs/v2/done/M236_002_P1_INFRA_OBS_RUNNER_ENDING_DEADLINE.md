# M236_002: Bound command endings and give up unresponsive executor links

**Prototype:** v2.0.0
**Milestone:** M236
**Workstream:** 002
**Date:** Nov 04, 2026
**Status:** DONE
**Priority:** P1 — a connected executor can leave a runner waiting indefinitely
**Categories:** INFRA, OBS
**Branch:** feat/m236-durable-tasks
**Baseline revision:** 1920ff345; source is integrated into PR #803
**Test Baseline:** pending; earlier component measurements used the M236 worktree and do not establish this branch's full baseline
**Baseline evidence:** command results and revisions will be recorded in the PR
**Depends on:** current main; no Durable Tasks dependency
**Provenance:** Indy's runner-hang report and four requested corrections; source inspected on current main
**Canonical architecture:** docs/architecture/runner_execution.md

## Owner closure — Nov 06, 2026

Closed with M236 under the [owner closure decision](M236_001_P0_API_DOCS_INFRA_OBS_UI_GOAL_DRIVEN_DURABLE_REPAIR.md#owner-closure--nov-06-2026).
Source is in existing PR #803. Automated merge checks remain required; live acceptance is parked for the separate NATS revamp.

## Overview

**Goal (testable):** Shell and Git calls return when a connected executor withholds its ending, preserving partial output and fencing further calls on that link.
**Problem:** The native client bounds spawn replies, but the later process-ending wait can remain open indefinitely.
**Solution summary:** Bound that wait, cooperatively await notifications, take finite output snapshots and invalidate the existing native connection on expiry.

This mitigates missing executor responses.
Tenant-to-executor signalling remains an unverified hypothesis requiring its own kernel audit specification.
No privilege, namespace, signal filter or sandbox identity change belongs here.

## PR Intent & comprehension handshake

- **PR:** Existing M236 implementation PR #803; this component bounds unresponsive executor links.
- **Intent:** A missing command ending cannot hang this or every later tool call in the lease.
- **Handshake:** Preserve confirmed endings and ordinary caller cancellation; an expired ending deadline closes the shared executor link.

## Implementing agent — read these first

1. `AGENTS.md`, `.orly/LOCAL.md`, `docs/RUST_ERROR_STANDARD.md` and the Rust, logging and test standards.
2. `afr_tools/src/sandbox/{oneshot/mod.rs,output/mod.rs}` — existing shared shell/Git execution and output ownership.
3. `afr_executor/src/{client/,events/,server/process/mod.rs}` — native connection loss, process abandonment and existing termination/drain allowances.
4. `afd_core/src/timing.rs`, `afr_supervisor/src/{renew/mod.rs,lease_loop/drive.rs}` — renewable grants, hard runtime and turn shutdown grace.

Paths above are relative to `rustd/crates/` unless rooted elsewhere.

## Files Changed (blast radius)

| Folder → files | Change |
|---|---|
| `rustd/crates/afr_executor/src/` → `timing.rs`, `lib.rs`, `server/process/mod.rs` | Share the existing kill/drain constants without copied values. |
| `afr_executor/src/events/` → `mod.rs`, snapshot/cooperation tests | Owned finite snapshots, cooperative notification waits, empty-frame filtering and native link invalidation handle. |
| `afr_executor/src/client/` → `mod.rs`, `link.rs`, existing fixture helpers and invalidation tests | Attach the existing lost token, prioritize link loss, refuse later calls immediately and interrupt pending calls/processes. |
| `afd_core/src/timing.rs`; `afr_supervisor/src/lease_loop/drive.rs` | Share the existing engine shutdown allowance for compile-time relationships. |
| `afr_tools/src/sandbox/` → move `oneshot.rs` to `oneshot/mod.rs`; `oneshot/tests/`, `output/`; crate manifest | Bound ending waits, name reply slack, retain partial output, invalidate only on expiry and exercise real client sockets. |
| `docs/architecture/runner_execution.md`; this specification and done destination | Record actual timing, truthful uncertainty, diagnostics and evidence limits. |
| Companion docs repository → `fleets/tools.mdx`, `changelog.mdx` | Explain interrupted command recovery and the pending release. |

Crate paths without a root above share `rustd/crates/`.
No admission, task, scheduler, database schema or new error vocabulary is added.

## Applicable Rules

- TIM: derive ending allowance from shared kill/drain constants and named reply slack; assert timing relationships at compile time.
- ERR-RS and RS-FROM: reuse existing executor errors and tool interruption vocabulary; introduce no handwritten error type.
- TEST-REDGREEN, TEST-OUTCOME and RS-RACE: assert public results, retained data, pending-call interruption and post-loss refusals.
- LOG-HOIST and AUTH-NOLOG: safe identities and durations only; no command text or output in warnings.
- FLL and NLG: native module layout, bounded functions and updated callers without compatibility aliases.

## Applicable Gates

Use `orly gate work`, `orly gate verify`, `orly gate pr` and the declared Make targets.
Run one Rust lane per machine with the cloud resource settings.
Preserve the existing coverage floors, authentication and native client path.

## Prior-Art / Reference Implementations

Reuse the native client's existing lost token and its pending-call/process interruption.
Use standard owned iterators over the moved buffer; add no custom iterator type or new mutex.
Use Tokio's public cooperative budgeting facility rather than a polling thread or another timer around synchronous draining.

## Sections (implementation slices)

### §1 — Bound wait and output consumption

The executor remains responsible for process-group termination.
The runner waits for command time plus kill grace, output-drain grace and named reply slack.
Unknown endings return `interrupted` and no exit code; available output remains readable.

- **Dimension 1.1** — Shell and Git silence returns partial output after the bound → `silent_endings_interrupt_commands_and_fence_later_calls`.
- **Dimension 1.2** — A confirmed ending during cleanup remains authoritative → `an_ending_during_cleanup_preserves_the_confirmed_timeout_without_a_second_kill`.
- **Dimension 1.3** — Later producer writes cannot extend one read → `later_output_and_ending_wait_for_the_next_snapshot`.
- **Dimension 1.4** — Notification loops allow timers and cancellation to run → `ending_waits_cooperate_on_output_and_leave_bytes_and_endings_readable`.

### §2 — Give up the connection once

Each native process event handle can invalidate its existing shared link token.
Expiry invalidates that link; it does not queue another command or claim a successful kill.
Pending calls and sibling processes interrupt, and later calls refuse without awaiting the frozen peer.

- **Dimension 2.1** — One invalidation interrupts pending calls and all attached processes → `invalidating_one_process_interrupts_the_shared_executor_link`.
- **Dimension 2.2** — Later calls fail immediately without another socket request → the same native invalidation scenario and public shell/Git scenario.
- **Dimension 2.3** — Ordinary caller cancellation preserves existing per-process cleanup → `caller_cancellation_keeps_native_cleanup_and_the_connection_usable`.

### §3 — Preserve timing and diagnostics

The default command wait must fit a lease grant; the maximum must fit the hard runtime ceiling.
Ending allowance must fit the existing engine shutdown grace.
Lease cancellation can stop any command earlier; no independent whole-turn wall timeout exists on the inspected main.

- **Dimension 3.1** — Timer relationships remain compile-time assertions → `make lint-rustd` compilation and native timeout scenarios.
- **Dimension 3.2** — Expiry warnings expose safe durations and identity, while tool duration records failure → structured assertions in the shell/Git scenario.

## Interfaces

The existing `Executor` trait remains unchanged.
`Events::take_ready` returns an owned iterator and the captured channel-finished flag.
`Events::disconnect` gives up its native connection; the native client alone attaches that handle.

No HTTP endpoint, CLI flag or customer prompt changes.

## Verification evidence

Earlier component runs are scoped evidence from this worktree, before the final link-loss corrections.
The silence regression failed before the guard: 260 tools tests passed and one failed at its missing-cleanup deadline.
The first corrected tools selection passed 261 tests, before the later snapshot and link-loss corrections.

The cooperative-budget regression failed before budgeting: 118 executor tests passed and one failed at `(129, 128)` versus `(128, 128)`.
Source review found and closed indefinite draining and unbudgeted notification loops.
Astra found no remaining material production issue in the final native-link changes.

Its test-proof correction now checks subsequent-call refusal in one poll.
The final runner shard passed 1,602 tests after the logging-field cleanup.
Clean integration passed 1,513 regular and three exclusive tests on the integrated source.

Final checkpoint verification passed workspace Clippy, formatting, dependency checks, prose and version synchronization.
No kernel signal-isolation proof is claimed.

## Discovery

> Indy: "Mark the executor link lost when the deadline fires."

> Indy: "Use KILL_GRACE plus a named reply slack, and const-assert that it fits inside the lease and turn bounds (rule TIM)."

> Indy initially: "Ship it as its own PR."

> Indy superseding that instruction: "I want all this in this PR" and "no new PR".

All source and documentation changes stay in existing implementation PR #803 and companion docs PR #241.
The temporary checkout only isolated local edits; it created no additional PR.

> Indy: "The root cause is still open."

The earlier description incorrectly treated post-expiry connection reuse as desirable.
The corrected requirement invalidates the shared link on expiry while preserving reuse after confirmed endings or ordinary caller cancellation.

## Out of scope

Durable Tasks, schedule activity and tenant/executor signal isolation.
Signal-isolation changes require the separate kernel audit and a reviewed mechanism.
