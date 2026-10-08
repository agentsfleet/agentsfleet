<!--
SPEC AUTHORING RULES (load-bearing — the one comment that survives):
- Body order = the executing agent's read order. Fill via the orly-spec-new
  skill (authoring order lives there); after filling, DELETE every "tpl:"
  guidance comment — the SPEC TEMPLATE GATE blocks tpl residue, unfilled
  {slots}, and missing required sections (audits/spec-template.sh --staged).
- No time/effort/hour/day estimates anywhere. No effort columns, complexity
  ratings, percentage-complete, implementation dates, assigned owners.
- Priority (P0/P1/P2/P3) is the only sizing signal; Dependencies are the only
  sequencing signal. A section that contradicts these rules loses — delete it.
-->

# M211_004: A fleet's next lease runs in the sandbox its last lease left — held frozen on the same runner, which claims that fleet's next event first

**Prototype:** v2.0.0
**Milestone:** M211
**Workstream:** 004
**Date:** Oct 05, 2026
**Status:** DONE
**Priority:** P1 — a follow-up chat message to a code-running fleet starts from an empty workspace in a new sandbox, so the files, processes and caches its last message made are gone
**Categories:** API, INFRA
**Batch:** B2 — folds into M211_002 at that stream's CHORE(open); its Sections run after M211_003's
**Branch:** feat/m211-nested-loops-and-chat-continuity
**Folded-into:** `M211_002`
**Baseline revision:** bb007001545cb97f4dc27c9325235a6a0ebb4fb9
**Test Baseline:** unit=4212 integration=4929 — Rust unit 4212 passed, 0 failed, 879 ignored (runner 982 · daemon 132 · daemon libraries 3098); integration through the coverage shards 4929 passed, 0 failed (substrate 4158 · runner 550 · daemon 221); TypeScript app 3642, design-system 647, website 142 passed, cli 1779 passed and 17 skipped, at `bb0070015` via PR #732's identical tree. The branch at `886733be6`: Rust unit 4312 passed, 0 failed, 895 ignored (+100); integration 868 + 2 exclusive passed, 0 failed; kernel lane 34 passed.
**Baseline evidence:** `playbooks/operations/acceptance/baselines/M211-bb0070015.md`
**Depends on:** M211_001 (the sandbox-side tools; a lease with only supervisor tools builds no sandbox, `rustd/crates/afr_agent/src/engine.rs:139-141`) · M211_003 (its two cgroup leaves freeze together; its host disk reserve is deferred, so nothing reserves disk for a hold) · M213_001 (the Rust runner takes leases; `rustd/crates/agentsfleet_runner/src/main.rs:184-185` refuses them until then)
**Provenance:** LLM-drafted (Claude Opus 5.5, Oct 05, 2026) from a source trace of the chat path at `b0138d7b3`, recorded in Discovery
**Canonical architecture:** `docs/architecture/runner_execution.md` §"Workspace between leases", §Toolbox; `docs/architecture/runner_fleet.md` §"Per-lease renewal — how a long fleet keeps its lease", §"Memory continuity — durable fleet memory rides the trusted plane"

---

## Overview

**Goal (testable):** `test_frozen_sandbox_resumes_where_it_stopped` — a fleet's second lease, within the idle window, reads the file its first lease wrote, in the sandbox that lease left; `test_next_lease_reuses_the_held_sandbox` proves the runner builds that sandbox once and thaws it for the second lease. The promise is files, installed packages and caches; a process outlives its lease only if it detached into its own session (Discovery).
**Problem:** A user asks a fleet in chat to write a script and install its dependencies, then asks for a change. The second message lands in a new sandbox with an empty `/workspace`, on whichever runner claims it first, so the fleet rebuilds everything before it can answer, or answers about files that are gone.
**Solution summary:** When a lease ends processed and used a sandbox, the runner freezes that sandbox and holds it for its fleet for an idle window instead of destroying it. The report and every heartbeat tell the daemon what the runner holds, and the daemon lets that runner claim the fleet's next event first while it is alive. The next lease thaws the sandbox and continues where the last one stopped. Any mismatch, lapse or load falls back to today's fresh sandbox.

## PR Intent & comprehension handshake

- **PR title (eventual):** folded into M211_001's Pull Request
- **Intent (one sentence):** A fleet's follow-up message continues in the sandbox its last message left, its files intact, instead of starting over.
- **Handshake** (PLAN, Oct 07, 2026) — restated: after a fleet's lease ends cleanly, its runner keeps the sandbox frozen for that fleet's next message and the daemon steers that message back to it, so files and processes carry over; any doubt falls back to a fresh sandbox. Matches the Intent. `ASSUMPTIONS I'M MAKING:` (1) `held_until_ms` rides `afd_wire::report::ReportRequest` (`report.rs:130`): `ExecutionResult` moved into `afr_agent` and no longer crosses the wire (M215_001 §3). (2) The hold key is fleet, workspace, the lease's `Limits` (the size it named, or the runner's own, M211_003 §4), toolbox digest and network policy. (3) `VERSION` is 0.57.0, past the 0.30.0 anchor, so `held_until` lands as an additive migration, `schema/932_runner_affinity_held_until.sql`, never an edit to `630_runner_affinity.sql`. (4) The freeze writes `cgroup.freeze` on the lease cgroup, so both leaves stop, and waits for `frozen 1` in `cgroup.events`. (5) No host disk reserve exists (deferred in M211_003): holds are bounded by `worker_count` alone, and a host's disk may fill, as Indy accepted. (6) The runner's holds family is a row in `docs/metrics.runner.census.tsv` with an `afr_telemetry` producer; M214_001 is done. (7) This spec executes after M211_005, which needs neither the kernel lane nor a schema change. **Quality ceiling:** a microVM snapshot per conversation would keep processes across hosts too; the freezer is the bubblewrap-era answer, and the registry mirrors `WarmSlots` rather than adding a lock. **Surface checklist:** OpenAPI yes (report and heartbeat fields; regenerate) · the product CLI no · user docs yes (how long a sandbox is kept, docs repo branch) · release/version at close · schema yes (additive, SCHEMA GUARD) · spec vs rules: Interfaces, Failure Modes and Files Changed amended below.

## Implementing agent — read these first

1. `rustd/crates/afr_supervisor/src/lease_loop/workspace.rs` — where a lease's sandbox is prepared and destroyed (`sandboxed`); the hold's take and park go here.
2. `rustd/crates/afr_sandbox/src/warm_slots.rs` — the one existing owner of ready sandboxes: a single task behind a channel, no lock; the hold registry mirrors it.
3. `rustd/crates/afd_fleet/src/lease/sql/lease.rs` — the affinity claim, both releases and the candidate scan the hold changes.
4. `docs/architecture/runner_fleet.md` — line 507's single-live-holder invariant, which a hold must keep.
5. https://docs.kernel.org/admin-guide/cgroup-v2.html — `cgroup.freeze` and the `frozen` key of `cgroup.events`.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `rustd/crates/afr_supervisor/src/holds.rs`, `rustd/crates/afr_supervisor/src/holds/` | CREATE | The hold registry: one owning task, `HoldKey`, deadlines, the cap, `Release` |
| `rustd/crates/afr_supervisor/src/lease_loop.rs`, `rustd/crates/afr_supervisor/src/lease_loop/{hold,hold_tests,revive_tests,unheld_tests,workspace,workspace_tests,settle,settle_tests,checkout_tests}.rs` | CREATE / EDIT | Take the fleet's hold before preparing; park instead of destroying on a processed ending; reuse leaves the clone alone |
| `rustd/crates/afr_supervisor/src/{worker_pool,heartbeat,report,report_spool,client,drainer,identity,lib,test_support}.rs`, `rustd/crates/afr_supervisor/src/{client/tests,drainer/tests,heartbeat/closing_tests,heartbeat/tests,lib_tests,report/tests,report_spool/tests,worker_pool/tests}.rs`, `rustd/crates/afr_supervisor/src/test_support/`, `rustd/crates/afr_supervisor/src/lease_telemetry_tests.rs` | EDIT | One registry per runner; holds on the poll and the heartbeat; `held_until_ms` on the report; `Delivery::Superseded` releases what the refused report parked |
| `rustd/crates/afr_sandbox/src/{engine,cgroup,bubblewrap_engine,unsandboxed,error,lib}.rs`, `rustd/crates/afr_sandbox/src/cgroup/freezer.rs`, `rustd/crates/afr_sandbox/src/cgroup/freezer/`, `rustd/crates/afr_sandbox/src/bubblewrap_engine/parts.rs`, `rustd/crates/afr_sandbox/src/error/raise.rs`, `rustd/crates/afr_sandbox/src/engine/tests.rs`, `rustd/crates/afr_sandbox/src/warm_slots/tests/fakes.rs` | CREATE / EDIT | `freeze` and `thaw` on the `Sandbox` trait, over `cgroup.freeze` |
| `rustd/crates/afr_sandbox/examples/kernel_lane/{hold,main,trials}.rs` | CREATE / EDIT | The kernel proof |
| `rustd/crates/afd_core/src/timing.rs` | EDIT | `SANDBOX_HOLD_IDLE_MS` |
| `rustd/crates/afd_state/src/sql.rs` | EDIT | `FLEET_STATUS_ACTIVE` moves here so `afd_runner` reads it; `afd_fleet` re-exports it |
| `rustd/crates/afd_wire/src/{lease,lib,report,runner,runner/holds}.rs`, `rustd/crates/afd_wire/tests/{validation_holds,validation_lease,wire_suite}.rs`, `public/openapi.json` | CREATE / EDIT | The five wire fields in Interfaces, `resume_hold` among them; the holds items' 36-byte bound; the regenerated document |
| `rustd/crates/afd_fleet/src/lease/{affinity,assign,commit,pull,report}.rs`, `rustd/crates/afd_fleet/src/lease/assign/{measured,offer}.rs`, `rustd/crates/afd_fleet/src/lease/sql/lease.rs`, `rustd/crates/afd_fleet/src/lease/pull/tests.rs` | CREATE / EDIT | Scan, claim and unleased release honour a hold; the holder looks at its holds first; the report's release records one, clamped |
| `rustd/crates/afd_fleet/{Cargo.toml,src/error/{lift_tests,mod,tests}.rs,src/lease/{affinity_tests,answer,envelope,installed,test_dead}.rs,src/lease/assign/{diagnostics,tests}.rs,src/lease/report/tests.rs,src/lease/sql/mod.rs}` | CREATE / EDIT | A lease says whether it resumes its runner's hold (`resume_hold`); a holder counts live only while it can lease; a lapsed hold counts no held claim; the poll reads its held fleets' readiness at once; the commit and error tests split under the length cap |
| `rustd/crates/afd_fleet/tests/integration_held_sandbox.rs`, `rustd/crates/afd_fleet/tests/integration_held_sandbox/`, `rustd/crates/afd_fleet/tests/integration_runner_beat/recovery.rs`, `rustd/crates/afd_fleet/tests/fleet_suite.rs`, `rustd/crates/afd_fleet/tests/integration_*.rs`, `rustd/crates/afd_fleet/tests/support/` | CREATE / EDIT | The integration proofs; existing callers pass an empty hold list |
| `rustd/crates/afd_runner/src/heartbeat.rs`, `rustd/crates/afd_runner/src/heartbeat/{holds,verdict}.rs`, `rustd/crates/afd_runner/src/sql/{mod,holds}.rs`, `rustd/crates/afd_api_runner/src/handler/runner/{heartbeat,lease}.rs`, `rustd/crates/afd_http/src/services/leasing.rs` | CREATE / EDIT | Reconcile a runner's holds, answer the inactive ones; the poll body carries holds again |
| `rustd/crates/afd_api/tests/harness/{fleet,fleet_seams,stubs_runner}.rs`, `rustd/crates/afd_api/tests/{integration_runner_heartbeat,runner_plane_suite,runner_plane/satellites}.rs`, `rustd/crates/afd_bench/src/lane/lease/{drive,drain/runner}.rs`, `rustd/crates/afd_bench/src/lane/cardinality/probe.rs` | CREATE / EDIT | Callers of the changed poll and heartbeat; the heartbeat's `release_holds` and holds lists through the real router |
| `schema/932_runner_affinity_held_until.sql`, `rustd/crates/afd_db/src/migration.rs` | CREATE / EDIT | `held_until`, additive, since `VERSION` is past the 0.30.0 anchor |
| `docs/architecture/runner_execution.md`, `docs/architecture/runner_fleet.md` | EDIT | A sandbox may outlive its lease; holder-first claim; the stale `uq_runner_affinity_fleet_id` at lines 29 and 507 |
| `rustd/crates/afd_observability/src/{semconv,metrics/declared/fleet,metrics/label/fleet,producers/fleet}.rs`, `rustd/crates/afd_observability/src/producers/fleet/hold.rs`, `docs/metrics.census.tsv` | CREATE / EDIT | The reuse span attribute and the held-claim counter with its producer |
| `docs/metrics.runner.census.tsv`, `rustd/crates/afr_telemetry/src/{families,labels,record,testing}.rs`, `rustd/crates/afr_telemetry/src/families/tests.rs` | EDIT | The runner census gains the holds family and its producer |
| `docs/v2/done/M211_004_P1_API_INFRA_RUST_RUNNER_CHAT_SANDBOX_HOLD.md`, `docs/v2/pending/M211_004_P1_API_INFRA_RUST_RUNNER_CHAT_SANDBOX_HOLD.md` | CREATE / DELETE | This spec, moved from `pending/` at CHORE(open) and to `done/` at CHORE(close) and to `done/` at CHORE(close) |
| `rustd/crates/afd_fleet/src/lease/answer/tests.rs` | CREATE | A rendered lease carries `resume_hold` as its claim decided |
| `rustd/crates/afd_runner/tests/integration_hold_plans.rs`, `rustd/crates/afd_runner/tests/runner_suite.rs` | CREATE / EDIT | The hold reconcile clears through the partial held index under a generic plan |
| `rustd/crates/afd_runner/tests/integration_sweeps.rs` | EDIT | The sweep test drains the lane's backlog before its convergence check, since this branch's suites leave more than one liveness batch of runner rows |
| `rustd/crates/afr_supervisor/src/heartbeat/unanswered_tests.rs` | CREATE | A stop abandons a beat in flight, and an unanswered last beat gives up after its bound |
| `rustd/crates/afr_sandbox/src/bubblewrap_engine/tests.rs`, `rustd/crates/afr_sandbox/src/bubblewrap_engine/tests/freeze.rs` | EDIT / CREATE | A prepared sandbox freezes and thaws through its own lease cgroup, on Linux, where the engine compiles |

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — NDC, UFS (the idle window and the cap are named constants; timing lives in `afd_core::timing`), TGU (the release reason is one enum), LOG, ERR-RS, MSID, ARCH, FLL, TST-NAM.
- `dispatch/write_rust.md` + `docs/RUST_ERROR_STANDARD.md` — every new fallible path through `afd_core::error_shell!`; a thaw failure keeps its cause.
- `docs/LOGGING_STANDARD.md` — the three hold events carry ids and a reason, never paths or contents.
- `dispatch/write_sql.md` + `docs/SCHEMA_CONVENTIONS.md` — the `held_until` column follows the rebuild posture or the additive rule, whichever holds at CHORE(open).
- `dispatch/name_architecture.md` — the hold reverses `runner_execution.md` lines 15 and 158 ("one lease, then destroyed"); the doc edit lands in the docs commit with this spec.
- Indy's Rust bar (Oct 04 and Oct 05, 2026): the registry is a struct that owns its sandboxes behind one task and a `tokio::sync::mpsc` channel, the shape `WarmSlots` uses, so no `Mutex`; `take` and `park` move `Box<dyn Sandbox>` by value, never clone it; freeze and thaw are `Sandbox` trait methods every engine implements; the deadline comes from the supervisor's injected `afd_core::clock::Clock` (the one `renew.rs` already reads, so one process keeps one time source; no `Fn() -> Instant` exists in the workspace), so tests drive time with `FixedClock` without sleeping; the release reason is one enum; `afd_core::timing` holds the window; `afd_observability` carries the span attribute and both metric families; no hand-rolled time or channel code.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| SCHEMA GUARD | yes — a new column | No `DROP`; the posture `docs/SCHEMA_CONVENTIONS.md` names at CHORE(open) |
| UFS GATE | yes — Rust | Idle window, cap and release reasons named once; daemon and runner share `afd_core` |
| LOGGING GATE | yes — three runner events | `docs/LOGGING_STANDARD.md` fields; `error_code` on every warning |
| MILESTONE-ID GATE | yes | No milestone identifiers in code, tests or comments |
| Architecture consult | yes | Doc edits in the same commit as the code (landing b) |
| File & Function Length (≤350/≤50/≤70) | yes | `holds.rs` alone owns the registry; split by take/park/release if it nears the cap |

## Prior-Art / Reference Implementations

- **Reference:** `rustd/crates/afr_sandbox/src/warm_slots.rs` — sandboxes owned by one task and claimed through a channel; a hold is the same ownership, filled after a lease instead of before one.
- **Reference:** the Linux cgroup v2 freezer (kernel documentation above) — a frozen cgroup keeps its memory and runs nothing; thawing resumes every process where it stopped.

## Sections (implementation slices)

### §1 — Hold the sandbox a processed lease leaves — DONE

`sandboxed()` takes the fleet's held sandbox before it prepares one, and parks the sandbox instead of destroying it when the lease ends processed. Park freezes the lease cgroup and waits for `frozen 1`, then records the key: fleet, workspace, limits, and the policy (network policy plus repository binding). No credential enters a sandbox (`afr_supervisor/src/lease_loop/checkout.rs:1-3`), so there is nothing to empty first, and the toolbox is fixed for the runner's process (`bubblewrap_engine.rs:52-56`), so it is not part of the key. Take requires the lease's `resume_hold` and the whole key to match, thaws, and checks that the executor answers; anything else destroys the hold and prepares fresh. Holds end at their deadline, when the runner's last free worker takes a lease (every hold but the one reused, because a busy runner cannot serve them), at the cap (`Capped`), when leasing stops or the runner shuts down (`Shutdown`), when the report of the lease that parked one is rejected or lost (`Superseded`), and when the daemon names the fleet inactive (§2). A sandbox no longer running at park is destroyed, not held. At most `worker_count` holds; the oldest goes first. On reuse the host never touches the clone: a `.git` the tenant wrote, copied or created into by the privileged host, is a symlink escape, so the repository stays exactly as the last lease left it, uncommitted edits included. Exec sessions close at the end of every run (`afr_agent/src/loop/finish.rs:76`), so what a hold keeps is files; a process survives only if it left the shell's process group, which the executor kills when a command ends (`afr_executor/src/server/process.rs`). **Implementation default:** a ten-minute idle window because a chat follow-up usually lands within minutes, and the cap and the freeze bound what a hold costs; Indy may change it.

- **Dimension 1.1** — DONE — PARKED for production (M213_001 Dimension 1.4) — A processed lease parks its sandbox frozen, and no renewal is sent → Test `test_processed_lease_parks_its_sandbox_frozen`
- **Dimension 1.2** — DONE — PARKED for production (M213_001 Dimension 1.4) — The fleet's next lease takes the hold: the file reads back, and a detached process frozen with it made no progress and runs on once thawed → Test `test_frozen_sandbox_resumes_where_it_stopped`
- **Dimension 1.3** — DONE — PARKED for production (M213_001 Dimension 1.4) — Other limits or another policy destroys the hold and prepares fresh; another fleet's lease leaves it alone → Test `test_mismatch_destroys_the_hold`
- **Dimension 1.4** — DONE — PARKED for production (M213_001 Dimension 1.4) — A hold past its deadline is destroyed → Test `test_expired_hold_is_destroyed`
- **Dimension 1.5** — DONE — PARKED for production (M213_001 Dimension 1.4) — The last free worker's lease releases every other hold → Test `test_saturation_releases_holds`
- **Dimension 1.6** — DONE — PARKED for production (M213_001 Dimension 1.4) — A failed, interrupted or superseded lease destroys its sandbox, as today → Test `test_failed_lease_never_parks`
- **Dimension 1.7** — DONE — PARKED for production (M213_001 Dimension 1.4) — Holds never exceed `worker_count`; the oldest is released first, as `Capped` → Test `test_holds_capped_oldest_first`
- **Dimension 1.8** — DONE — PARKED for production (M213_001 Dimension 1.4) — Reuse leaves the clone exactly as the last lease left it: no fetch, no copy, no host write into `.git` → Test `test_reuse_leaves_the_clone_untouched`
- **Dimension 1.9** — DONE — PARKED for production (M213_001 Dimension 1.4) — A thaw that fails, or an executor silent after thaw, falls back to a fresh sandbox → Test `test_thaw_failure_falls_back_fresh`

### §2 — Tell the daemon what the runner holds — DONE

The report carries `held_until_ms` when its lease parked. The report's fencing-guarded release writes it to `fleet.runner_affinity.held_until` in the report's own transaction, so a superseded holder records nothing and destroys what it parked. The daemon clamps a reported `held_until_ms` to `now + SANDBOX_HOLD_IDLE_MS` and stores nothing for one already past. Every heartbeat carries the fleets the runner holds; the daemon clears `held_until` on that runner's rows the list omits, except a hold reported within the last beat interval, and a list it cannot read, out of bounds or empty holds nothing. It answers with the listed fleets that are not both active and last held by this runner, which the runner destroys: a stranger's fleet answers exactly as a deleted one, so the answer leaks nothing across tenants. A runner that releases holds for saturation, or because leasing stopped, heartbeats at once rather than on its tick. A lease carries `resume_hold`, true only when the slot's last lease ran on the claiming runner, its hold had not lapsed, and the event is not a reclaim.

- **Dimension 2.1** — DONE — A report with `held_until_ms` stores it, clamped, under the fencing guard; a stale token stores nothing → Test `test_report_stores_hold_under_fencing`
- **Dimension 2.2** — DONE — A heartbeat that omits a held fleet clears its hold → Test `test_heartbeat_clears_dropped_holds`
- **Dimension 2.3** — DONE — PARKED for production (M213_001 Dimension 1.4) — The answer names a held fleet that was halted or deleted, and the runner destroys that hold → Test `test_inactive_fleet_hold_destroyed`
- **Dimension 2.4** — DONE — A report or heartbeat without the new fields decodes as today → Test `test_hold_fields_are_optional_on_the_wire`

### §3 — The holder claims first — DONE

`SELECT_READY_CANDIDATES` and `CLAIM_AFFINITY_SLOT` skip a fleet another runner holds while `held_until` is in the future and that runner can lease: active, not degraded, and `fleet.runners.last_seen_at > now - RUNNER_OFFLINE_AFTER_MS` (`afd_core/src/timing.rs:55`). The lease poll carries the runner's holds, and the holder looks at those fleets first (`afd_fleet/src/lease/assign/offer.rs`, `held_first`, reading every held fleet's readiness token from Dragonfly at once, a failed read skipping that fleet, so an idle poll still touches no Postgres), before its partition scan, so the next message does not wait for the round-robin. `RELEASE_UNLEASED_SLOT` keeps the hint while the fleet is held; its starvation guard still covers every fleet not held. Another runner's claim clears `held_until`; the holder's own claim keeps it, so an empty claim by the holder (Dimension 3.5) leaves the hold standing. A hold is a head start at the slot, never the slot: the holder claims through the same fencing-guarded statement, so the single live holder stays one.

- **Dimension 3.1** — DONE — The holder claims its held fleet's next event → Test `test_holder_claims_its_held_fleet`
- **Dimension 3.2** — DONE — Another runner skips a held fleet while the holder is live → Test `test_other_runner_skips_held_fleet`
- **Dimension 3.3** — DONE — Another runner claims it once the holder misses `RUNNER_OFFLINE_AFTER_MS` or the hold lapses → Test `test_lapsed_hold_is_claimable`
- **Dimension 3.4** — DONE — The holder finds its held fleet's event on its next poll, whatever the partition → Test `test_holder_polls_its_holds_first`
- **Dimension 3.5** — DONE — An empty claim by the holder keeps the hint while held → Test `test_drained_claim_keeps_held_hint`
- **Dimension 3.6** — DONE — A fleet nobody holds is claimed, released and unhinted exactly as before → Test `test_unheld_fleet_claims_as_before`

## Interfaces

```
afd_wire::report::ReportRequest      + held_until_ms: Option<i64>    absent = nothing held; clamped to now + SANDBOX_HOLD_IDLE_MS
afd_wire::runner::HeldFleets         fleet ids, at most one per worker (garde-bounded; published max = enforced max)
afd_wire::runner::HeartbeatRequest   + holds: HeldFleets              every fleet the runner holds now
afd_wire::runner::HeartbeatResponse  + release_holds: Vec<fleet id>   listed fleets not active-and-held-by-this-runner
afd_wire::lease::LeaseRequest        + holds: HeldFleets              the poll body; the daemon offers these first
afd_wire::lease::LeasePayload        + resume_hold: bool              true only for the slot's latest hold on this runner, never on a reclaim; absent = false
fleet.runner_affinity                + held_until BIGINT NULL         milliseconds since the epoch; NULL = not held
afr_sandbox::Sandbox                 + freeze(), thaw()               cgroup.freeze, settled on cgroup.events
afr_supervisor holds                 take(HoldKey) · park(HoldKey, sandbox) · release(fleet, Release)
HoldKey                              fleet · workspace · Limits · policy (network policy + repository binding)
Release                              Expired | Saturated | Capped | Mismatch | Inactive | Shutdown | ThawFailed | Superseded
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Holder dies | Crash or host loss mid-hold | Other runners claim after `RUNNER_OFFLINE_AFTER_MS`; the boot sweep removes the sandbox (`bubblewrap_engine/sweep.rs:25`); the user waits at most that long |
| Holder saturated | Its last free worker took a lease | Holds released, heartbeat sent at once; the fleet is claimable everywhere |
| Key mismatch | Limits or policy changed between leases | Hold destroyed, fresh sandbox, `Mismatch` logged; the reply carries no stale state |
| Thaw fails | cgroup error, or the executor died while frozen | Hold destroyed, fresh sandbox; `sandbox_thaw_failed` logs `ThawFailed` with the kernel's cause, and no reuse is counted (`test_thaw_failure_falls_back_fresh`, `test_a_failed_thaw_counts_no_reuse`) |
| Superseded report | The lease was reclaimed and the fencing token bumped | Report refused as today; the runner destroys what it parked (`Superseded`) |
| Fleet halted or deleted | Owner action during the hold | Named in the next heartbeat answer; the runner destroys the hold and its tenant data |
| Too many fleets | More holds than workers | The oldest hold is released (`Capped`) |
| Stale hold | The event is a reclaim, the hold lapsed, or another runner ran the fleet since | The lease carries `resume_hold` false; the runner builds fresh and ends the hold as `Superseded` (`test_a_reclaimed_event_builds_fresh_on_a_live_hold`, `test_a_claim_after_another_runner_ran_the_fleet_builds_fresh`, `test_a_lease_not_resuming_builds_fresh_and_ends_the_hold`) |
| Holder cannot lease | It is degraded, drained, revoked or cordoned while its beat is still recent | It counts as no holder; other runners claim its held fleets (`test_a_degraded_holders_fleet_is_leased_by_another_runner`, `test_a_drained_holders_fleet_is_leased_by_another_runner`) |
| Beat list predates a report | The runner lists its holds, then a report commits a hold before the beat lands | A hold reported within the last beat interval survives that beat; the next beat clears it if still unlisted (`test_a_hold_reported_inside_the_beat_interval_survives_that_beat`). A `closing` beat's list is final, so it clears such a hold at once (`test_a_closing_beat_clears_a_hold_reported_inside_the_interval`) |
| Bad holds list | A beat whose list is unreadable, out of bounds or empty | Read as holding nothing: the runner's recorded holds are cleared, on the beat and the poll alike (`test_an_out_of_bounds_holds_list_holds_nothing`) |
| Leasing stops | The runner stops leasing for any cause, or shuts down | Every hold ends as `Shutdown` and a later park is destroyed; a beat with an empty list goes out at once, and once more as serving ends, each marked `closing`, so the daemon frees those fleets (`test_leasing_stopped_ends_every_hold_and_beats_at_once`, `test_a_shutdown_ends_every_hold_and_says_so_in_a_last_beat`). A stop abandons a beat in flight and gives up on a last beat unanswered after 5 s (`test_a_stop_abandons_a_beat_in_flight_for_the_last_one`, `test_an_unanswered_last_beat_is_given_up_after_its_bound`) |
| Report rejected or lost | The daemon rejects the report for good, or it was never spooled | Its hold ends as `Superseded`, as a superseded report's does (`test_a_rejected_report_destroys_what_it_parked`, `test_an_unspooled_report_lost_destroys_what_it_parked`, `test_a_drained_rejected_report_releases_what_its_lease_parked`) |
| Sandbox gone at park | The sandbox is no longer running when its lease ends | Destroyed, not held (`test_a_sandbox_no_longer_running_is_destroyed_not_held`) |
| Background process | The model backgrounded a server with `&`, or left it in an exec session | The executor kills the shell's process group as the command ends, and the run's end closes every session; the next lease finds the files, not the process |
| Two events at once | Back-to-back messages | The affinity slot admits one holder, unchanged; the second waits for the first's report (`test_unheld_fleet_claims_as_before`) |
| Host disk filling | Held workspace disks keep their bytes | Nothing reserves disk for a hold (M211_003 deferred its reserve); the cap bounds holds to `worker_count` (`test_holds_capped_oldest_first`), and a full host disk ends a writer in `ENOSPC` as it does today |

## Invariants

1. A held sandbox serves only its own fleet — take compares fleet and workspace ids; `test_mismatch_destroys_the_hold`.
2. No credential outlives its lease — no credential enters a sandbox at all (`afr_supervisor/src/lease_loop/checkout.rs:1-3`), so a held one carries none; the host never writes into a held clone; `test_reuse_leaves_the_clone_untouched`.
3. A held sandbox runs nothing — its cgroup reads `frozen 1` from park to take; `test_frozen_sandbox_resumes_where_it_stopped` (kernel).
4. A hold never extends a lease — the report goes out at lease end and no renewal is sent for a held fleet; `test_processed_lease_parks_its_sandbox_frozen`.
5. At most `worker_count` holds per runner — the registry releases the oldest first; `test_holds_capped_oldest_first`.
6. Only the holder claims a held fleet while it is live — the claim statement's own condition; `test_other_runner_skips_held_fleet`, and through another runner's poll naming the fleet, `test_naming_a_held_fleet_wins_another_runner_nothing`.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| `sandbox_held` (runner log, info) | ops | A processed lease parks its sandbox | lease id, fleet id, deadline | no paths, no file contents, no credentials | `test_a_parked_sandbox_is_taken_by_its_fleets_next_lease` |
| `sandbox_reused` (runner log, info) | ops | A lease takes a hold | lease id, fleet id, milliseconds held | as above | `test_a_reused_sandbox_is_marked_on_the_lease_span_and_logged` |
| `sandbox_hold_released` (runner log, info) | ops | A hold ends without reuse | fleet id, reason | as above | `test_expired_hold_is_destroyed` |
| `agentsfleet.sandbox.reused` on the `runner.lease` span (a new `afd_observability::semconv` constant) | ops | Every lease that used a sandbox | `true` when taken from a hold | ids only | `test_a_reused_sandbox_is_marked_on_the_lease_span_and_logged` |
| `agentsfleet_lease_held_claims_total` (daemon, `afd_observability` declared counter + producer) | ops | A claim on a fleet that was held | `outcome`: `holder` or `other_after_lapse` | no ids in labels | `test_the_holders_claim_counts_as_the_holders`, `test_the_holders_reclaims_on_a_live_hold_count_nothing`, `test_another_runners_claim_after_a_lapse_counts_as_other_after_lapse` |
| `agentsfleet_runner_sandbox_holds_total` (runner census, `afr_telemetry` producer) | ops | A hold is parked, reused or released | `outcome`: `parked`, `reused`, or one of the eight release reasons (ten labels) | closed label set | `test_every_runner_census_family_has_a_producer`, `test_a_park_and_its_reuse_are_counted` |

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit | `test_processed_lease_parks_its_sandbox_frozen` | lease ends processed → the sandbox is frozen and parked, not destroyed; no renew after the report |
| 1.2 | kernel | `test_frozen_sandbox_resumes_where_it_stopped` | lease writes `/workspace/note` and starts a detached ticker; freeze → `cgroup.events` reads `frozen 1` and the ticker makes no progress; thaw → `note` reads back, the ticker is alive and runs on |
| 1.2 | unit | `test_next_lease_reuses_the_held_sandbox` | a processed lease, then its fleet's lease with `resume_hold` → one sandbox, built once, thawed for the second lease and held again |
| 1.3 | unit | `test_mismatch_destroys_the_hold` | limits, then policy, changed → the hold destroyed, `Mismatch`, a fresh sandbox prepared; another fleet's lease → the hold untouched |
| 1.3 | unit | `test_a_lease_not_resuming_builds_fresh_and_ends_the_hold` | a hold, then its fleet's lease with `resume_hold` false → the hold ends `Superseded`, a fresh sandbox is built |
| 1.4 | unit | `test_expired_hold_is_destroyed` | fake clock past the deadline → destroyed, `Expired` |
| 1.5 | unit | `test_saturation_releases_holds` | two workers, two holds; both workers take leases → zero holds, an immediate heartbeat |
| 1.5 | unit | `test_leasing_stopped_ends_every_hold_and_beats_at_once` | a held fleet, then leasing stops → the hold ends `Shutdown`; the next beat lists none before the tick |
| 1.5 | unit | `test_a_shutdown_ends_every_hold_and_says_so_in_a_last_beat` | a held fleet, then shutdown → the hold ends, and one last beat lists none |
| 1.6 | unit | `test_failed_lease_never_parks` | endings failed and interrupted, and a report answered superseded → destroy called, no hold |
| 1.6 | unit | `test_a_rejected_report_destroys_what_it_parked` | a parked lease whose report the daemon rejects for good → the hold ends `Superseded` |
| 1.6 | unit | `test_a_sandbox_no_longer_running_is_destroyed_not_held` | a processed lease whose sandbox stopped running → destroyed, no hold |
| 1.7 | unit | `test_holds_capped_oldest_first` | `worker_count` 2, three parks → the first released as `Capped`, two remain |
| 1.8 | unit | `test_reuse_leaves_the_clone_untouched` | a reused sandbox → no checkout step runs, the tenant's `.git` is never read or written by the host |
| 1.9 | unit | `test_thaw_failure_falls_back_fresh` | the held sandbox refuses to thaw → released `ThawFailed`; `sandbox_thaw_failed` carries its code once and a reason holding the refusal's cause; a fresh sandbox; the lease ends `processed` |
| 2.1 | integration | `test_report_stores_hold_under_fencing` | current token → `held_until` stored; bumped token → row unchanged |
| 2.1 | unit | `test_a_held_deadline_is_clamped_to_the_window` | past → nothing stored; beyond the window → `now + SANDBOX_HOLD_IDLE_MS`; within → as asked |
| 2.2 | integration | `test_heartbeat_clears_dropped_holds` | holds `[f1, f2]`, then `[f1]` → `f2.held_until` NULL |
| 2.2 | integration | `test_an_out_of_bounds_holds_list_holds_nothing` | a holds list past its bounds, by count or by an entry's length, and an unreadable body → each clears its runner's hold |
| 2.2 | integration | `test_a_hold_reported_inside_the_beat_interval_survives_that_beat` | a report commits a hold, then a beat whose list omits it inside the interval → the hold stands; the next beat clears it |
| 2.3 | integration | `test_inactive_fleet_hold_destroyed` | `f1` halted → answer names `f1` |
| 2.3 | unit | `test_a_released_hold_is_destroyed_as_inactive` | the heartbeat answer names `f1` → the runner releases `f1` as `Inactive`; a saturation release beats at once |
| 2.4 | unit | `test_hold_fields_are_optional_on_the_wire` | a report and a heartbeat without the fields → decode with nothing held |
| 3.1 | integration | `test_holder_claims_its_held_fleet` | runner A holds `f1`; event on `f1` → A's poll leases it |
| 3.2 | integration | `test_other_runner_skips_held_fleet` | B polls first → no lease for `f1`; A then claims it |
| 3.3 | integration | `test_lapsed_hold_is_claimable` | A's heartbeat older than `RUNNER_OFFLINE_AFTER_MS`, or the hold past → B leases `f1` |
| 3.3 | integration | `test_a_degraded_holders_fleet_is_leased_by_another_runner` | holder A beats, then is marked degraded → B's poll leases A's held fleet (`test_a_drained_holders_fleet_is_leased_by_another_runner` for a drained A) |
| 3.3 | integration | `test_a_reclaimed_event_builds_fresh_on_a_live_hold` | the holder's resumed lease lapses unreported → the reclaim's lease has `resume_hold` false though the hold is live (`test_a_claim_after_another_runner_ran_the_fleet_builds_fresh` when B ran the fleet between) |
| 3.4 | integration | `test_holder_polls_its_holds_first` | `f1` in a partition A's cursor has not reached → A's next poll leases `f1` |
| 3.5 | integration | `test_drained_claim_keeps_held_hint` | A claims held `f1` with nothing ready → `last_runner_id` stays A |
| 3.6 | integration | `test_unheld_fleet_claims_as_before` | no hold anywhere → claim order, release and hint clearing unchanged |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | A fleet's next lease continues in its held sandbox, frozen and credential-free between (§1) | `make test-runner-kernel 2>&1 \| grep -c "test_frozen_sandbox_resumes_where_it_stopped ... ok"` | 1 | P0 | |
| R2 | Hold bookkeeping ends every hold it must (§1) | `cargo test --manifest-path rustd/Cargo.toml -p afr_supervisor holds` | exit 0 | P0 | |
| R3 | The daemon routes a held fleet to its holder and frees it on lapse (§2, §3) | `make test-integration-rustd 2>&1 \| grep -c "test_lapsed_hold_is_claimable ... ok"` | 1 | P0 | |
| R4 | Diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 paths missing from the Files Changed table | P0 | |
| S1 | Conform gates green | `make harness-verify` | exit 0 | P0 | |
| S2 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | |
| S3 | Lint green | `make lint-all` | exit 0 | P0 | |
| S4 | Integration green | `make test-integration-rustd` | exit 0 | P0 | |
| S5 | Version in sync | `make check-version` | exit 0 | P0 | |
| S6 | No secrets | `gitleaks detect` | exit 0 | P0 | |
| S7 | No oversize source file | `git diff --name-only origin/main...HEAD \| grep -v '\.md$' \| xargs wc -l 2>/dev/null \| awk '$1>350 && $2!="total"'` | no output | P0 | |

**Command source rule:** every declared `conform` and `verify.*` command from `.oracle/orly.json` appears above verbatim. **Grading protocol (VERIFY):** Graded = ✅/❌ + one decisive output line; repository rows point to the final `orly gate pr` results in Session Notes.

## Dead Code Sweep

N/A — no files deleted.

## Out of Scope

- Earlier turns in the prompt: the model sees only the instructions and the current message (`rustd/crates/afr_agent/src/prompt.rs:25-58`); M211_005 carries them, cached.
- A conversation identifier: the fleet is the conversation (`rustd/crates/afd_events/src/history/statement.rs:264-271`).
- Carrying a workspace between hosts in R2 (Cloudflare's object storage); M213_001 defers it.
- Snapshot and restore under the Firecracker engine.
- Any user-facing control over the hold, and holding the sandbox of a failed lease.

---

## Product Clarity (authoring record)

1. **Successful user moment** — The user asks for a script and a running server, then for a flag; the fleet edits the same file, restarts the same server and answers without a word about setting up again.
2. **Preserved user behaviour** — A message after the window, a fleet without sandbox tools, and every failed lease behave exactly as today; billing stays per lease.
3. **Optimal-way check** — The unconstrained shape is a per-conversation microVM snapshotted between messages; that waits for the Firecracker engine, and the hold is its bubblewrap-era approximation.
4. **Rebuild-vs-iterate** — Iterate: two hook points in `sandboxed()`, three wire fields and one column (Decomposition).
5. **What we build** — The hold registry with freeze, the report and heartbeat fields, holder-first claim, three log events, the two architecture edits.
6. **What we do NOT build** — Prompt history, conversation identifiers, cross-host workspaces, snapshots, user controls (Out of Scope).
7. **Fit with existing features** — Compounds with M211's sandbox tools and M211_002's sub-runs; must not destabilize the single live holder per fleet.
8. **Surface order** — No new surface; the change shows in chat and in the runner's log.
9. **Dashboard restraint** — No hold indicator in the user interface (UI) until reuse rates are measured.
10. **Confused-user next step** — The chat page of the published docs says how long a sandbox is kept, so "my files are gone" after the window has its answer.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** three Sections by authority — the runner owns the sandbox (§1), the report and heartbeat carry the state (§2), the daemon owns the claim (§3). §1 alone already reuses on a one-runner host; §2 and §3 make reuse hold with many runners.
- **Alternatives considered:** keeping the lease open between messages (rejected: every renewal bills a run fee, `afd_fleet/src/lease/sql/renew.rs:75`, and it needs mid-run message delivery); runner-only reuse with no routing (rejected: the hint only reorders a runner's own candidates, `sql/lease.rs:160`, so with many runners most follow-ups land elsewhere); restoring the workspace from R2 on every message (later and cross-host, but it loses running processes and pays a restore each time).
- **Patch-vs-refactor verdict:** this is a **patch** because the lease, fencing and settle paths stay as they are; the hold sits beside them.

## Discovery (consult log)

- **Consults** — Indy (in-session, Oct 04, 2026): "I want to provide a seamless faster approach on chat?"; chose "Spec chat keep-alive" for a pending spec. Source trace (Oct 05, 2026, at `b0138d7b3`), each claim read from source: the prompt holds the instructions and the current message only (`afr_agent/src/prompt.rs:25-58`); each lease prepares and destroys its sandbox before settle (`afr_supervisor/src/lease_loop/workspace.rs:26-56`); the scan only sorts the last runner first (`afd_fleet/src/lease/sql/lease.rs:145-161`) and an empty claim clears the hint (`:66-68`); readiness marks carry a generation, not a time (`afd_fleet/src/lease/mark.rs:10-12`). Architecture consult: `ARCH: grounded in runner_execution.md:15,158 | proposal: a processed lease's sandbox may be held, frozen, for its fleet's next lease | status: conflicts | landing: a` — Indy (in-session, Oct 05, 2026): "Fix all fixes in this PR, may be the 215_001 must be renamed to the next sequence in 211_00X and fix all that is needed", folding the hold into this Pull Request; the doc edit lands in the docs commit that lands this spec. Indy chose "Add M211_005, cached (recommended)" for earlier turns. The ten-minute window stays an implementation default Indy may change.
- **Built-against-spec amendments** (Oct 07, 2026, read from the branch's source) — no `/run/creds` step, because no credential enters a sandbox (`afr_supervisor/src/lease_loop/checkout.rs:1-3`); no toolbox digest in the key, because the toolbox is fixed per runner process (`afr_sandbox/src/bubblewrap_engine.rs:52-56,107-111`); `Release::Capped` added for the cap; only another runner's claim clears `held_until`, or Dimension 3.5 contradicts "any claim clears"; holder liveness is `fleet.runners.last_seen_at > now - RUNNER_OFFLINE_AFTER_MS`; the kernel's 1.1 and 1.2 run as one engine-level trial, 1.1 moves to the supervisor's unit tier; the daemon clamps `held_until_ms`.
- **Two calls made while Indy was away, then confirmed** — > Indy (2026-10-07 ~16:00, AskUserQuestion): chose "Leave it untouched (Recommended)" — context: a held sandbox's repository is never refreshed from the host, because a tenant-written `.git` under a privileged `copy_tree`/`create_dir_all` is a symlink escape; Dimension 1.8 becomes `test_reuse_leaves_the_clone_untouched`. > Indy (2026-10-07 ~16:00, AskUserQuestion): chose "Yes, carry holds (Recommended)" — context: `LeaseRequest { holds }` restored as the poll body, reversing the deletion documented in `afd_api_runner/src/handler/runner/lease.rs`; idle polls stay Dragonfly-only.
- **What a hold keeps** (Oct 07, 2026) — Indy asked "When does this happen?" of background processes. Source: the executor KILLs the shell's process group as a command ends (`afr_executor/src/server/process.rs`, after the drive loop), and the agent loop closes every exec session at run end (`afr_agent/src/loop/finish.rs:76`). So a hold keeps files, installed packages and caches; only a process that left the group with `setsid` before the kill survives. The agent's call, unanswered by Indy and confirmable in the Pull Request: promise files only, no executor change.
- **Kernel trial root cause** (Oct 07, 2026) — the trial's ticker died before the freeze: `/workspace/tick` was never created and pid 4 was absent 0.2 s after start. `setsid` was still in the shell's group when the shell ended and the executor's group KILL landed. The trial now waits for the first tick before its shell ends; 3 of 3 runs `ok` on afr-kernel.
- **Production run deferred to M213_001** — > Indy (2026-10-07 ~16:08, AskUserQuestion): "Move them the dimenstions as parked and the spec as DONE. Mention in a prompt to me so i can ask the other agent in M213 to deploy and test this." — context: `agentsfleet_runner/src/main.rs:184-185` refuses every lease with `NO_AGENT_ENGINE`, so §1 and the runner half of §2 run in tests only until M213_001 Dimension 1.4 lands; each such Dimension is marked PARKED.
- **Unit-test ledger at the boundary (Oct 07, 2026)** — the ledger over the branch's own diff found two places the code contradicted this spec, both fixed to it. A drained report answered `Superseded` released nothing (`afr_supervisor/src/drainer.rs`); it now ends the hold its lease parked, the call the live settle makes, and `Holds::supersede` takes the lease alone because a spooled report knows only its lease id (`test_a_drained_superseded_report_releases_what_its_lease_parked`). A failed thaw logged `[UZ-INTERNAL-003] an input/output call failed` without its cause; a silent or failing executor now logs its own failure with the cause (`test_an_executor_silent_after_thaw_falls_back_fresh`, `test_an_executor_failing_after_thaw_falls_back_fresh`). The heartbeat's two malformed shapes first differed: a holds list past its bounds reconciled nothing, while a body that would not parse cleared every hold. Review made them one rule (`6905beb74`): a beat whose holds list is unreadable, out of bounds or empty reads as holding nothing and clears the runner's recorded holds, on the beat and the poll alike (`test_an_out_of_bounds_holds_list_holds_nothing`).
- **Review fixes (Oct 08, 2026)** — A lease now says whether it resumes its runner's hold: `resume_hold` is true only when the slot's last lease ran on the claiming runner, its hold had not lapsed, and the event is not a reclaim (`6905beb74`; `test_the_holders_claim_on_a_live_hold_resumes_it`, `test_a_reclaimed_event_builds_fresh_on_a_live_hold`). The runner takes a hold only on `resume_hold` and otherwise ends it as superseded (`d0afd22a0`; `test_a_lease_resuming_takes_the_hold`, `test_a_lease_not_resuming_builds_fresh_and_ends_the_hold`). A degraded, drained, revoked or cordoned holder binds no fleet (`test_a_degraded_holders_fleet_is_leased_by_another_runner`). A beat spares a hold reported within its interval (`test_a_hold_reported_inside_the_beat_interval_survives_that_beat`), and a partial index keeps each beat's clear to the runner's held rows. Every hold ends when leasing stops or the runner shuts down, and a beat with an empty list frees those fleets (`test_leasing_stopped_ends_every_hold_and_beats_at_once`). A rejected or lost report ends its hold (`test_an_unspooled_report_lost_destroys_what_it_parked`). Reuse is counted once the thaw answers (`test_a_park_and_its_reuse_are_counted`, `test_a_failed_thaw_counts_no_reuse`), and a failed thaw logs the kernel's cause (`test_thaw_failure_falls_back_fresh`). Three findings are not fixed and are parked for Indy. A held sandbox keeps live `setsid` processes from any event type, webhook-driven ones included, into the next lease; security reviewers recommend killing the tenant leaf before freezing, or holding chat leases only. Held sandboxes' memory and disk are not counted against a host budget. The chat turn window also replays the fleet's non-chat events as turns.
- **Metrics review** — Three runner log events, exported with the rest by M214_001; no analytics or funnel playbook change, because no product event changes.
- **Skill-chain outcomes** — `/orly-write-unit-test` at the boundary (Oct 08, 2026): the review round's ledger over `657ae5807..df3b1f52d` (44 production files) found 8 gaps, closed in `190012f01`, with two won't-test rows (a refused `memory.high` write needs a cgroup file system; an unencodable report cannot occur); a coverage pass closed the remaining testable arms in `b9e282233`. `make test-coverage-rustd` at `b9e282233`: patch coverage 99.3078% (2726 of 2745 changed lines) against the 99 floor, line coverage 98.8582%; the 19 unhit lines are pre-exec hooks and the sandbox entry, which run in a forked child or inside bubblewrap where no profile is written, plus arms no input reaches. gstack `/review`: ten readers over `e2242b5b4..657ae5807`, fixes in `585f8c29f`, `6905beb74` and `d0afd22a0`; a second pass over the fixes found four more, fixed in `e96879f57` and `a414fbc13`; the findings left open are listed for Indy in the Pull Request. `make test-runner-kernel` on afr-kernel at `a414fbc13`: 41 passed, 0 failed. Mutation testing over the review round runs after the Pull Request opens and its result goes in the Pull Request's Session notes; `orly-babysit-prs` follows the push.
- **Deferrals** — the production run of the runner-side Dimensions, to M213_001 Dimension 1.4 (quote above).
