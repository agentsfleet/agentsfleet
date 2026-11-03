<!--
SPEC AUTHORING RULES (load-bearing; the one comment that survives):
- Body order is the implementing agent's read order. The orly-spec skill fills
  this template under the spec standard (spec.md), then deletes every "tpl:"
  guidance comment.
- An EGM (Elephant-Goldfish Model) review checks the structure: leftover tpl
  comments, unfilled fill slots, missing or empty sections and the spec
  standard's rules. orly's gate checks no spec structure.
- No time or effort estimates anywhere: no effort columns, complexity ratings,
  percentage complete, implementation dates or assigned owners.
- Priority (P0/P1/P2/P3) is the only sizing signal; Depends on is the only
  sequencing signal. A section that contradicts these rules loses; delete it.
-->

# M237_001: A lease's sandbox builds behind its first model call, and its disk is released after the report

**Prototype:** v2.0.0

**Milestone:** M237

**Workstream:** 001

**Date:** Nov 02, 2026

**Status:** IN_PROGRESS

**Priority:** P1, every lease that offers a sandbox tool waits for sandbox setup and checkout before the model starts

**Categories:** API, DOCS, INFRA, OBS

**Batch:** B1, lands before M236_001 builds; M237_002 is independent of it

**Branch:** `feat/m237-sandbox-lease-path`

**Baseline revision:** 2f8c878c09e9c4048da1724385ec58746d348ed4

**Test Baseline:** pending, measured before the Pull Request

**Baseline evidence:** `playbooks/operations/acceptance/baselines/M237_001-lease-waits.md`, measured on the local kernel lane at `937c8d34d`

**Depends on:** none. M236_001 (pending) builds its §5 restore inside this spec's background build; see Discovery.

**Provenance:** LLM-drafted (Claude Opus 5.5, Nov 02, 2026) from Indy's Nov 02, 2026 sandbox review and dev-host checks that day

**Canonical architecture:** `docs/architecture/runner_execution.md` §"One lease, end to end" and §"Code walkthrough"

---

## Overview

**Goal (testable):** `test_first_model_call_starts_before_the_sandbox_is_ready`: a lease offering `shell` sends its first model request before its sandbox answers, and its first `shell` call still runs inside that sandbox.

**Problem:** A lease that offers any sandbox tool resolves its egress, builds its sandbox and checks out every bound repository before the model starts. The user's first words wait for all of it. After the run, a sandbox that is not held is unmounted and its image deleted before the report is posted, so the answer's completion waits for disk work too.

**Solution summary:** After memory loading, the lease runs the build beside the model call, in the same task. The first sandbox tool call waits for the build's executor, and supervisor tools never wait. At lease end, the sandbox's processes are killed before the report and its disk is released after the report. §1 first measures today's waits so the gain is shown, not assumed.

**Design flow:**

```
today                                            after
claim ─► memory ─► size ─► egress ─► prepare     claim ─► memory ─► size, policy, egress
      ─► checkout ─► model call ─► tools               ├─► model call ─► tool call (waits once)
                                                       └─► prepare ─► checkout ───────────┘
end: keep(kill, egress, cgroup, umount, rm)      end: keep(freeze | kill) ─► settle(report)
     ─► settle(report)                                ─► destroy(egress, cgroup, umount, rm)
```

## PR Intent & comprehension handshake

- **PR title (eventual):** perf(runner): start the sandbox behind the first model call
- **Intent (one sentence):** A user's first answer stops waiting on sandbox setup, and a finished answer stops waiting on disk cleanup.
- **Authoring handshake:** Indy asked to make the toolbox and bubblewrap path faster, approved five fixes, and asked that this pull request land before M236_001 builds. Indy chose that a build failing while the run is live leaves the run going. ASSUMPTIONS I'M MAKING: egress resolution stays before the model call; no wire, schema or public interface changes.
- **Implementer handshake:** recorded in the Pull Request Session Notes at PLAN, before any edit. A restatement that differs from the Intent stops the work until the two are reconciled.

Terms used below: `startup_posture` is the failure class a lease reports when it is refused before its turn runs. The boot sweep is the engine's removal, at start, of every lease directory a crashed runner left (`bubblewrap_engine/sweep.rs`).

## Implementing agent — read these first

1. `docs/architecture/runner_execution.md` — §"Code walkthrough" Parts B, C and G trace every call this spec moves, with file references.
2. `rustd/crates/afr_supervisor/src/lease_loop/mod.rs` — the lease's run, its cut and `settle_and_drain`; §2 and §3 change their order.
3. `rustd/crates/afr_supervisor/src/lease_loop/hold.rs` — `revive`, `keep` and `destroy`, which the build and the release reuse.
4. `rustd/crates/afr_supervisor/src/lease_loop/segments.rs` — the timeline whose `sandbox` segment stops tiling the lease once it overlaps `model`.
5. `docs/v2/pending/M236_001_P0_API_DOCS_INFRA_OBS_UI_GOAL_DRIVEN_DURABLE_REPAIR.md` — its §5 restore joins the build this spec moves.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `rustd/crates/afr_supervisor/src/lease_loop/{mod,workspace,hold,checkout,drive,settle,segments}.rs` | EDIT | Build beside the run, typed build outcome, kill before report, release after it, timers |
| `rustd/crates/afr_supervisor/src/lease_loop/build.rs` | CREATE | The build future and its outcome, kept out of `mod.rs`, which is near the length cap |
| `rustd/crates/afr_telemetry/src/{labels,families,record,testing}*`, `docs/metrics.runner.census.tsv` | EDIT | The `sandbox_wait` and `sandbox_release` families, declared in the census both directions read |
| `rustd/crates/agentsfleetd/{Cargo.toml,src/lib.rs}`, `rustd/crates/afr_executor/Cargo.toml` | EDIT | A stalled engine in the daemon suite needs `async-trait`; the slot's tests need paused time |
| `rustd/crates/afr_supervisor/src/lease_loop/tests/` | EDIT | Ordering, failure, cancellation and timing proofs |
| `rustd/crates/afr_supervisor/src/test_support/{mod,sandbox,resolver,rig}.rs` | EDIT | Fake engine gains a stall, a failure point and a recorded kill; a stalled resolver times the egress bind |
| `rustd/crates/afr_supervisor/src/holds/{mod,keeper}.rs` and tests, `heartbeat/tests.rs`, `drainer/tests.rs` | EDIT | A park the keeper refuses comes back to the lease, which kills it now and releases it after the report |
| `rustd/crates/afr_sandbox/src/{cgroup,engine}/` | EDIT | `cgroup.kill`, then a bounded wait on `populated 0`; the `Killed` record |
| `rustd/crates/afr_sandbox/examples/kernel_lane/kill.rs` | CREATE | The kernel trial: `setsid sleep 600` is gone after a settled kill, and the disk waits for the release |
| `rustd/crates/afr_executor/src/api/mod.rs` and a new `slot` module with tests | EDIT, CREATE | `ExecutorSlot`: set once by the build, awaited by the router |
| `rustd/crates/afr_agent/src/{engine,loop,router}/` and tests | EDIT | A run takes the slot; the router awaits it; run end reads it without waiting |
| `rustd/crates/afr_providers/tests/support/mod.rs`, `rustd/crates/afr_tools/src/testing.rs` | EDIT | Test builders of `AgentRun` pass a slot |
| `rustd/crates/afr_sandbox/src/engine/mod.rs` and tests | EDIT | `Sandbox::kill`, returning a `Killed` proof |
| `rustd/crates/afr_sandbox/src/bubblewrap_engine/{mod,parts}.rs` and tests | EDIT | Kill and release split; `Parts` owns disk paths before any host program runs |
| `rustd/crates/afr_sandbox/src/{unsandboxed,warm_slots}/mod.rs` | EDIT | Implement `kill`; warm slots stay unwired |
| `rustd/crates/afr_sandbox/examples/kernel_lane/{hold,tools,git,files}.rs` | EDIT | Kernel trial for §3 and callers of the changed run input |
| `rustd/crates/afr_telemetry/src/{families,record}/` | EDIT | `sandbox_wait` and `sandbox_release` duration families |
| `rustd/crates/agentsfleetd/tests/integration_rust_runner.rs` | EDIT | End-to-end proof through the daemon |
| `docs/architecture/runner_execution.md` | EDIT | Walkthrough and lease order after the change |
| `docs/v2/active/M237_001_P1_API_DOCS_INFRA_OBS_SANDBOX_START_HIDES_BEHIND_MODEL.md` | EDIT | This spec, opened at CHORE(open); moves to `done/` at close |

## Applicable Rules

- **`languages/rust.md`** — ERR-RS, RS-FROM, RS-MAPERR, RS-SOURCE for the build outcome and kill errors; RS-RACE for the slot and its waiters; RS-BORROW for the slot the run borrows.
- **`tests.md`** — TEST-REDGREEN, TEST-OUTCOME, TEST-ERROR, TEST-RESIDUAL for leftovers after a late release, TEST-ACKLOSS for a report spooled but not acknowledged.
- **`domains/architecture.md`** — ARCH-LAND and ARCH-HOMES: lease order changes in `runner_execution.md` in the same Pull Request.
- **`domains/logging.md`** — every new event is a named constant with an `event` field and no secret.
- **`docs.md`** — DOC-02, DOC-03, DOC-05, DOC-14b and DOC-33 for changed architecture prose.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| `make lint-all` | yes, Rust changes | Run before each commit |
| `make test-unit-all` coverage gates | yes | New branches carry unit tests through the fake engine |
| `make test-runner-kernel` | yes, §3's no-survivor proof is kernel behaviour | One lane run, recorded with its kernel version |
| `make test-integration-rustd` | yes, lease order crosses the daemon | The new end-to-end test runs in this lane |
| File & Function Length (file ≤350 lines, function ≤50, method ≤70) | yes, `lease_loop/mod.rs` and `parts.rs` are near the cap | The build goes to `build.rs`; release steps stay in `parts.rs` |
| `orly gate verify` docs language | yes | Short sentences, no banned words in changed prose |

## Prior-Art / Reference Implementations

- **Reference:** `rustd/crates/afr_supervisor/src/lease_loop/mod.rs` — the renewal cut already ends a live run from a concurrent future in the same task; a build failure uses the same cut.
- **Reference:** `rustd/crates/afr_sandbox/src/cgroup/mod.rs` — `DRAIN_POLL` and `DRAIN_TRIES` bound today's wait for an empty cgroup; the kill reuses that bound.
- **Reference:** `docs/architecture/runner_execution.md` §"The agent loop runs outside the sandbox" — names a model call starting while the workspace is still being built as the intended shape.

## Sections (implementation slices)

### §1 — Today's waits are measured

Before PLAN, the implementing agent records today's lease waits as Baseline evidence. Egress and checkout gain their own timers, so `sandbox` splits into `egress`, `prepare` and `checkout` on the `lease_completed` line. The evidence holds the dev host's recent `sandbox_ms` and `report_ms`, and the kernel lane's budgets trial on a disk-backed state directory.

Under §2's owner decision the egress bind stays ahead of the model call. Its share of `sandbox` is therefore the setup wait this spec leaves in place.

- **Dimension 1.1** — `lease_completed` carries `egress_ms`, `prepare_ms` and `checkout_ms` apart → Test `test_lease_completed_splits_prepare_and_checkout` — DONE
- **Dimension 1.2** — The baseline file exists before PLAN with both hosts' figures → Test `manual_baseline_recorded_before_plan` — DONE

### §2 — The sandbox builds beside the first model call

Checks whose failure would repeat on every lease stay before the model call: `sized`, the `checkouts(policy)` refusal, the workspace identifier parse and the egress bind. Their refusals stay `startup_posture`, as today. After memory loading succeeds, `work` runs two futures in one task: the build and `drive`. The build revives or prepares the sandbox and checks repositories out. It never runs on `tokio::spawn`, since it borrows the lease.

The build ends by filling the slot: with the executor, or with `Unavailable` naming why. The classes are `prepare`, `checkout`, and `stopped` for a build the run's end stopped. Only a ready sandbox moves into the lease's cell; a stopped or failed build destroys what it made first. A `CancellationToken` stops it only between steps, never by dropping the future, so a hold taken by `revive` always finishes its thaw.

The run receives `&dyn ExecutorSource` in place of `Option<&dyn Executor>`, and the lease's `ExecutorSlot` is its one implementation. The slot borrows the executor from the cell, so the run's lifetimes stay free of the slot's. The router awaits it on a call whose runtime is `Sandbox`, and child loops route through the same slot. Run end reads it without waiting.

**Owner decision (Indy, Nov 02):** a `Failed` build while the run is live leaves the run going. The slot resolves to nothing and carries the failure's class: prepare or checkout. Every sandbox tool call, already waiting or later, answers `sandbox_unavailable` with a message naming that class and no host detail. Whatever the build made is killed and destroyed at once, and nothing is held. The run ends however its model ends it, billed as usual.

End rules, in order of precedence:

1. A run that ended processed and uncut waits for the build outcome. An answer that arrives mid-checkout therefore still leaves a complete sandbox, held by `keep`'s existing conditions.
2. A cut, refused or failed run cancels the build and awaits its outcome. A `Partial` or `Failed` outcome is killed and destroyed, never held. The report carries the run's own ending.
3. A build that fails after the run already ended keeps the run's ending in the report. Nothing is held, and `sandbox_build_failed` is logged.

A processed ending proves that the answer was given, not that the workspace was ready. After a `Failed` build, live or late, no files are left for a later lease to continue. A build that a cut stopped is not a failure and logs no `sandbox_build_failed`.

- **Dimension 2.1** — The first model request leaves before the sandbox answers → Test `test_first_model_call_starts_before_the_sandbox_is_ready` — DONE
- **Dimension 2.2** — A sandbox tool called mid-build waits, then runs in that sandbox → Test `test_sandbox_tool_waits_for_the_build` — DONE
- **Dimension 2.3** — A build failure mid-run leaves the run going, and each sandbox call answers `sandbox_unavailable` naming the class → Test `test_build_failure_leaves_the_run_going` — DONE
- **Dimension 2.4** — A size, policy or egress refusal still happens before any model request → Test `test_static_refusals_precede_the_model_call` — DONE
- **Dimension 2.5** — A run cut mid-checkout destroys the partial sandbox and holds nothing → Test `test_partial_checkout_is_never_held` — DONE
- **Dimension 2.6** — A cut during a thaw finishes the thaw, then destroys → Test `test_cut_during_thaw_settles_the_hold` — DONE
- **Dimension 2.7** — A supervisor-only lease starts no build → Test `test_supervisor_only_lease_starts_no_build` — DONE
- **Dimension 2.8** — Through the daemon, a chat's first stream frame precedes sandbox ready → Test `test_chat_streams_before_sandbox_ready`
- **Dimension 2.9** — A memory load refusal starts no build → Test `test_memory_refusal_starts_no_build` — DONE
- **Dimension 2.10** — A build failing after the run ended leaves the run's ending in the report → Test `test_late_build_failure_keeps_the_run_ending` — DONE

### §3 — The report posts before the disk is released

At lease end the order becomes: `keep` freezes a held sandbox or kills any other one. `settle` then posts or spools the report. Only after `settle` returns does `Sandbox::destroy` remove egress rules, the cgroup, the mount, the image and the directory. `run_lease` returns after the release, so a worker never holds two leases' disks.

`Sandbox::kill` runs `cgroup.kill`, ends bwrap and waits for `cgroup.events` to read `populated 0`, bounded by the cgroup drain bound. It always returns a `Killed` record: `Settled`, or `Unsettled` with its reason when a write failed or the bound passed. Every path that drops a sandbox before the report kills it first, including a failed build and the hold park fallback.

Only `Settled` proves the sandbox's process tree is empty. `Unsettled` still lets the report post the run's ending, and a `sandbox_kill_unsettled` warning names the lease. It proves no writer has stopped, so nothing may save the workspace on its strength.

- **Dimension 3.1** — The order is kill, report, then release → Test `test_report_posts_between_kill_and_release` — DONE
- **Dimension 3.2** — After a `Settled` kill, no sandbox process runs once the report is posted → Test `test_no_sandbox_process_outlives_the_report` — DONE
- **Dimension 3.3** — A kill that never settles still posts the report, warns, and claims no empty tree → Test `test_unsettled_kill_still_reports` — DONE
- **Dimension 3.4** — A release that fails after the report leaves its directory for the boot sweep → Test `test_late_release_failure_leaves_work_for_the_sweep` — DONE
- **Dimension 3.5** — The worker polls again only after the release → Test `test_worker_polls_after_release` — DONE

### §4 — The architecture page tells the new order

`runner_execution.md` carries §"Code walkthrough" from the spec's own pull request. This Section rewrites Parts C and G to the built order at merge, and records §1's before and after figures.

- **Dimension 4.1** — Every walkthrough call exists in that order at the merge commit → Test `manual_walkthrough_matches_code`

## Interfaces

```
afr_executor::ExecutorSource        (new trait, internal)
  async get()   -> Result<&dyn Executor, Unavailable>   awaited by the router for Runtime::Sandbox;
                   Unavailable names the class and maps to ToolErrorCode::SandboxUnavailable
  peek()        -> Option<&dyn Executor>   run end and session close
  is_filled()   -> bool                    the router records a wait only when it waited

afr_executor::ExecutorSlot<'a>       (new, internal; the one ExecutorSource)
  fill(Ok(&'a dyn Executor) | Err(Unavailable)) -> bool   once, by the lease's build
  held()        -> Option<Duration>        first waiting call to the fill: sandbox_wait_ms
  Unavailable:  NoSandbox | Prepare | Checkout | Stopped

afr_agent::AgentRun.executor        Option<&dyn Executor>  ─►  &dyn ExecutorSource

afr_sandbox::Sandbox                (internal trait)
  + async fn kill(&self) -> Killed            Settled | Unsettled(afr_sandbox::Error); disk, cgroup and
                                              egress stay; only Settled proves an empty process tree
    async fn destroy(self: Box<Self>) -> Result<()>   unchanged signature; kills first if not killed

afr_supervisor::holds::Holds::park  -> Parked   Held(lapse) | Refused(sandbox) | Gone; a refused
                                              sandbox comes back for the lease to kill and release

Wire, schema, OpenAPI and CLI: no change.
```

## Failure Modes

| Mode | Kind | Cause | Handling (system response + what the caller observes) | Test |
|------|------|-------|--------------------------------------------------------|------|
| Build fails mid-run | failure | Disk, cgroup, bubblewrap or checkout fails after the model started | Run continues; sandbox calls answer `sandbox_unavailable` with the class; sandbox killed and destroyed at once; `sandbox_build_failed` logged | `test_build_failure_leaves_the_run_going` |
| Static refusal | failure | Size out of bounds, a binding with no checkout, or an egress host that will not resolve or is private | Refused before any model request, as today | `test_static_refusals_precede_the_model_call` |
| Memory refused | failure | Memory load refused before the build | No build starts; lease refused as today | `test_memory_refusal_starts_no_build` |
| Late build failure | failure | Build fails after the run ended | Report keeps the run's ending; whatever was built is destroyed; nothing held; `sandbox_build_failed` logged | `test_late_build_failure_keeps_the_run_ending` |
| Partial checkout | failure | Run cut or refused while a repository was still copying | Build stops at the next step; sandbox destroyed, never held | `test_partial_checkout_is_never_held` |
| Cut during thaw | failure | Renewal refused while a hold thaws | Thaw finishes, sandbox destroyed once, nothing held | `test_cut_during_thaw_settles_the_hold` |
| Turn fails mid-build | failure | The engine fails while the build runs | Build stopped at its next step; sandbox destroyed once, never held; report carries the failure | `test_a_failed_turn_stops_its_build_and_holds_nothing` |
| Waiter at failure | failure | A tool call waits on the slot when the build fails | The call answers `sandbox_unavailable` naming the class | `test_build_failure_leaves_the_run_going` |
| Process outlives report | security | Kill skipped on some drop path | Every pre-report drop kills first; report waits, bounded, on `populated 0`; only `Settled` claims an empty tree | `test_no_sandbox_process_outlives_the_report` |
| Kill never settles | capacity | A process stuck in uninterruptible sleep | Bounded wait, report posts, warning names the lease; the record claims no empty tree, so no save reads the disk | `test_unsettled_kill_still_reports` |
| Release fails late | failure | Unmount refused after the report | Logged with its duration; directory left for the boot sweep | `test_late_release_failure_leaves_work_for_the_sweep` |

## Invariants

1. Nothing runs unsandboxed in a sandbox's place — the router holds no executor until the slot yields one, enforced by the slot's type.
2. No report posts before its sandbox's kill was issued and awaited — `settle` takes the `Killed` record or the hold's lapse time, so the compiler rejects a call without one. An `Unsettled` record posts and warns, and proves no empty tree.
3. A worker holds one lease's disk at a time — the release future is `#[must_use]` and `run_lease` awaits it before returning.
4. The build is never dropped mid-step — it runs to a typed outcome, and the lease awaits that outcome on every ending.

## Metrics & Observability

| Metric / event | Audience | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|----------|------------|--------------------|---------------|------------|
| `lease_completed` fields `egress_ms`, `prepare_ms`, `checkout_ms`, `sandbox_wait_ms` | operator | A lease concludes | lease id, milliseconds | no tool arguments | `test_lease_completed_splits_prepare_and_checkout` |
| `agentsfleet_runner_sandbox_wait_duration_seconds` | operator | A sandbox tool call waited on the slot | outcome label only, no lease id | none needed | `test_sandbox_tool_waits_for_the_build` |
| `agentsfleet_runner_sandbox_release_duration_seconds` | operator | Release ends after the report | outcome label only | none needed | `test_late_release_failure_leaves_work_for_the_sweep` |
| `sandbox_kill_unsettled` | operator | Kill wait hits its bound | lease id, error code | none needed | `test_unsettled_kill_still_reports` |
| `sandbox_build_failed` | operator | The build fails, while the run is live or after it ended; never for a build a cut stopped | lease id, class, error code | no host, address or path | `test_build_failure_leaves_the_run_going`, `test_late_build_failure_keeps_the_run_ending` |

The `segments.rs` comment that segments tile the lease changes: `prepare` and `checkout` now overlap `model`.

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit | `test_lease_completed_splits_prepare_and_checkout` | Fake resolver 10 ms, engine 30 ms, checkout 50 ms → fields near 10, 30 and 50 |
| 1.2 | manual | `manual_baseline_recorded_before_plan` | Baseline evidence file names the local kernel lane, its kernel version and p50/p95; Indy moved the measurement local |
| 2.1 | unit | `test_first_model_call_starts_before_the_sandbox_is_ready` | Engine stalled 500 ms → provider request time before ready time |
| 2.2 | unit | `test_sandbox_tool_waits_for_the_build` | `shell` first, engine stalled → call runs on that engine's executor |
| 2.3 | unit | `test_build_failure_leaves_the_run_going` | Engine fails after first request → waiting and later `shell` calls answer `sandbox_unavailable` naming `prepare`; the run answers; no hold |
| 2.4 | unit | `test_static_refusals_precede_the_model_call` | Disk size past bound, or an allowlisted host at a private address → zero provider requests, `startup_posture` |
| 2.5 | unit | `test_partial_checkout_is_never_held` | Renewal refused mid-checkout → `Partial`, sandbox destroyed, no hold; a processed answer mid-checkout → `Ready`, held |
| 2.6 | unit | `test_cut_during_thaw_settles_the_hold` | Renewal refused during thaw → thaw completes, destroy once |
| 2.7 | unit | `test_supervisor_only_lease_starts_no_build` | Policy `http_request` only → engine `prepare` never called |
| 2.8 | integration | `test_chat_streams_before_sandbox_ready` | Daemon plus runner, stalled engine → first frame before sandbox ready |
| 2.9 | unit | `test_memory_refusal_starts_no_build` | Memory load refused → engine `prepare` never called, `startup_posture` |
| 2.10 | unit | `test_late_build_failure_keeps_the_run_ending` | Answer processed, then build fails → report `processed` path kept, sandbox destroyed, no hold, `sandbox_build_failed` logged; a cut-stopped build logs none |
| 3.1 | unit | `test_report_posts_between_kill_and_release` | Recording sandbox and plane → kill, report, destroy |
| 3.2 | kernel | `test_no_sandbox_process_outlives_the_report` | `sleep 600` running, lease ends → kill `Settled`, cgroup empty when report posts |
| 3.3 | unit | `test_unsettled_kill_still_reports` | Tree never empties → `Unsettled` with its reason, report posted, `sandbox_kill_unsettled` logged, nothing asserts the tree empty |
| 3.4 | unit | `test_late_release_failure_leaves_work_for_the_sweep` | Release fails → report already posted, directory kept |
| 3.5 | unit | `test_worker_polls_after_release` | Release stalled → no lease poll until it returns |
| 4.1 | manual | `manual_walkthrough_matches_code` | Reviewer follows each Part's file references at the merge commit |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Model starts before the sandbox; failures keep today's refusals (§2) | `make test-unit-rustd-runner` | exit 0 | P0 | |
| R2 | Through the daemon, the stream precedes sandbox ready (§2) | `make test-integration-rustd` | exit 0 | P0 | |
| R3 | After a settled kill, no sandbox process outlives its report (§3) | `make test-runner-kernel` | exit 0 | P0 | |
| R4 | Baseline and after figures recorded (§1, §4) | `test -s playbooks/operations/acceptance/baselines/M237_001-lease-waits.md` | exit 0 | P1 | |
| S1 | Lint | `make lint-all` | exit 0 | P0 | |
| S2 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | |
| S3 | Version | `make check-version` | exit 0 | P0 | |
| S4 | No secrets | `gitleaks detect --redact --no-banner` | exit 0 | P0 | |

The spec standard (`spec.md`) defines grading, the ship rule and transfers to a successor spec (`MOVED` rows) in SPEC-GRADE, SPEC-SHIP and SPEC-MOVE.

## Dead Code Sweep

**1. Orphaned files — deleted from disk and git.**

N/A: no files deleted.

**2. Orphaned references — zero remaining imports/uses.**

N/A: no symbol is removed; `AgentRun.executor` changes type in place.

## Out of Scope

- Toolbox direct I/O, the executor socket pair and an in-process disk mount: M237_002.
- Wiring warm slots: once §2 lands the build hides behind the model call. Not planned.
- Constant-time checkout through a read-only mirror bind and git alternates: it breaks "a copy, never a link" in `workspace_clone`. A separate security spec, if `checkout_ms` dominates.
- Workspace save and restore to Cloudflare R2: M236_001 §5.

---

## Product Clarity (authoring record)

1. **Successful user moment** — A user asks a repair fleet a question and its first words stream while the repository is still being checked out.
2. **Preserved user behaviour** — Tools, results, holds, egress and every refusal message stay as they are.
3. **Optimal-way check** — Hiding setup behind the model call is the direct route. Removing setup altogether needs workspace snapshots, which M236_001 owns.
4. **Rebuild-vs-iterate** — Iterate: the engine interface already separates prepare, hold and destroy.
5. **What we build** — A wait timer split, an executor slot, a typed build outcome, a kill step and a later release.
6. **What we do NOT build** — Warm-slot wiring and alternates checkout; reasons in Out of Scope.
7. **Fit with existing features** — Compounds with held sandboxes; must not weaken the rule that nothing runs unsandboxed.
8. **Surface order** — N/A: no user surface; the runner reorders steps inside a lease.
9. **Dashboard restraint** — N/A: one new duration family and two log fields, no screen.
10. **Confused-user next step** — An operator reads `sandbox_wait_ms` on `lease_completed` to see whether setup still delays a tool call.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** this Workstream changes lease order in the supervisor; M237_002 changes one engine's internals. M236_001 needs only this one.
- **Alternatives considered:** a lazy build on first sandbox tool call skips work for leases that never use one, but puts the whole build into that call. `tokio::spawn` for the build needs owned lease state and a drop-safe engine; one task with two futures needs neither.
- **Patch-vs-refactor verdict:** this is a **patch** because no interface outside the runner changes and each refusal keeps today's failure class.

## Discovery (consult log)

- **Consults** — Nov 02, 2026, revision `77147e0d2`. Indy asked when bubblewrap runs and how to make the toolbox path faster. Indy: "1-5 seemed good fixes to do" and "Yes i am happy with it, along with the spec".
- **Dev host (Nov 02, `zombie-dev-worker-ant`)** — Debian 13.7, bare metal, kernel `6.12.111+deb13-amd64` running and `6.12.113-1` installed. Seven days of runner logs hold three `sandbox_prepare_completed` events.
- **Owner decision** — Asked on Nov 02 with timelines for both choices, Indy chose B: a build failing mid-run leaves the run going. The agent then moved the egress bind back before the model call, so configuration errors still refuse the lease.
- **M236_001 interaction, restore** — M236_001 edits `afr_supervisor/src/{holds,lease_loop}/`, and this spec merges first. M236 §5's restore then runs inside the build after prepare, in checkout's place for each repository it restores. Checkout never runs over restored work: `check_out` creates its destination and fails on one that exists (`workspace_clone/checkout.rs:60`).
- **M236_001 interaction, build steps** — The build therefore keeps prepare and each repository's checkout as separate steps, and sets the slot only after the last one lands. Under §2's owner decision, a failed restore leaves the run going without its workspace. M236 §5 decides whether a repair task waits instead.
- **M236_001 interaction, save** — A save fits §3 only after a `Settled` kill and before destroy, since the disk stays mounted until after the report. An `Unsettled` kill proves no writer stopped, so M236 saves nothing from it. This spec itself saves and uploads no workspace.
- **Workspace snapshots and the toolbox** — A Cloudflare R2 snapshot carries the workspace disk's contents: local commits, uncommitted edits and untracked files. It never carries the toolbox, which every host of a release holds as one read-only image by digest. The toolbox mount adds no copy on the host and holds no tenant data, so fleets sharing it is safe.
- **Restore under each engine** — Under bubblewrap the host mounts the lease disk, so M236's restore writes there through `Sandbox::workspace`. Under Firecracker `Sandbox::workspace` returns none, so restore and checkout must run in the guest or write into the image before attach. M236 §5 picks one mechanism that serves both engines.
- **Model reviews** — EGM review, Claude Opus 5.5, Nov 02: zoom-out 8, comprehension 8, readiness 10, design 10 findings. Accepted: split into two Workstreams; static checks before the model call; build after memory loading; typed outcome with no drop-cancel; one failure mechanism; slot in `afr_executor`; bounded kill; baseline first; metric names on existing conventions. Owner question kept: §2's failure rule.
- **Greptile on PR #797** — two P2 findings, both fixed: test 2.5 contradicted end rule 1, and a timed-out kill had no report path. `Killed` is now a record, `Settled` or `Unsettled`.
- **Codex review** — Codex reviewed baseline `2f8c878c0` on Nov 02, before decision B landed. Its five findings are wording, each folded in above:
  - Only a `Settled` kill proves an empty process tree (§3, R3).
  - A processed ending proves the answer, not the workspace (§2, end rule 3).
  - Restore takes checkout's place, so checkout never overwrites restored work (Discovery, `runner_execution.md`).
  - The build and the lease end keep places for M236's restore and save (Discovery).
  - M237_002 §1's cache drop is advice, so its test measures resident pages.
- **Codex against decision B** — Codex read the text in which egress ran inside the build. Under B the egress bind stays ahead of the model call, so §1 now times it as `egress_ms`. No other finding depends on the failure rule.
- **Codex's separate findings** — Six proposals stay outside M237, each a spec of its own if wanted:
  - a boot-time trial sandbox, since the probe checks only that bwrap is executable (`afr_sandbox/src/probe/mod.rs:165`);
  - the deploy window in which the new toolbox is staged before the restart (`deploy/baremetal/deploy.sh:300-304`);
  - Landlock socket scopes for `allow_all` leases, which share the host network namespace (`egress/mod.rs:149`);
  - an egress resolution cache, network-namespace churn and the checkout copy, each waiting on §1's figures.

  A reflink checkout cannot help: ext4 has no reflink, and the copy crosses file systems. Asked Nov 02 which become specs; Indy has not answered yet.
- **Built shape (§2)** — The spec's `Arc<dyn Executor>` became a borrow behind the `ExecutorSource` trait. A slot holding the borrow is invariant in its lifetime, so `AgentRun` could not name it; the trait object hides it, and `Sandbox::executor` keeps its signature. The typed outcome is the slot's fill, because nothing reads a separate enum. `sandbox_build_failed` replaces `sandbox_refused`, which no dashboard or alert names, because a failed build no longer refuses the lease.
- **Built shape (§3)** — `keep` answers a `Fate`: held until a time, killed and waiting for release, or gone. `settle` takes the `Fate`, so no report settles before its kill. The release runs beside the live tail's drain, so neither stalls the other, and the lease returns only after both. The park fallback no longer destroys inside the hold keeper: `park` hands a refused sandbox back as `Parked::Refused`, with its release already counted. The unsandboxed engine answers `Unsettled`, since no cgroup proves its processes gone.
- **Measured locally (Indy, Nov 02)** — "if you need to measure do it in local? in the docker compose using make targets". The baseline uses the OrbStack kernel lane's budgets trial and the supervisor's paused-clock tests, not the dev host, whose logs held three prepares in seven days.
- **Dev host kernel** — The dev host stays on `6.12.111` until M237 ships, so before and after figures share one kernel. Indy chose this on Nov 02; the quote is below.
- **PLAN (Nov 02, 2026: 10:38 PM)** — Intent restated: the first model request stops waiting on sandbox setup, and the report stops waiting on disk cleanup. A failed build leaves the run going. ASSUMPTIONS I'M MAKING:
  1. §1 lands as its own commit before any §2 code: three timers, then the baseline file.
  2. `sandbox_ms` stays on `lease_completed` as the whole setup, beside the three new fields.
  3. Seven days on the dev host hold three prepares, too few for a p95. The kernel lane's budgets trial carries the distribution.
  4. The kernel lane's OrbStack machine is recreated from `docs/development.md` before §3's kernel test.
  5. M233_001 also edits `segments.rs`; whichever lands second rebases.
  6. The Mac unit lane's `base64` failure needs Indy's override at the Pull Request: "Pass for now".
- **Open blockers** — none.
- **Metrics review** — `egress_ms`, `prepare_ms`, `checkout_ms` and `sandbox_wait_ms` added to `lease_completed`; two duration families added.
- **Skill-chain outcomes** — pending until VERIFY and REVIEW.
- **Deferrals and transfers** — none.

> Owner (Indy, Nov 02, 2026 09:52 PM): "B: model carries on" — note: "i assume they (setup -> sandbox) and model are not connected? they can be independent?" — context: M237_001 §2, a build that fails while the run is live.

> Owner (Indy, Nov 02, 2026 10:38 PM): "Reboot after M237 ships" — context: when `zombie-dev-worker-ant` moves from kernel `6.12.111` to the installed `6.12.113`.
