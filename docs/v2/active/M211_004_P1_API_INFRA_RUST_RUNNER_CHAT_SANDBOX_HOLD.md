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
**Status:** IN_PROGRESS
**Priority:** P1 — a follow-up chat message to a code-running fleet starts from an empty workspace in a new sandbox, so the files, processes and caches its last message made are gone
**Categories:** API, INFRA
**Batch:** B1 — folded into M211_001 and shipped in its Pull Request; its Sections run after M211_003's
**Branch:** `feat/m211-sandbox-tools-and-nested-loops`
**Folded-into:** `M211_001`
**Baseline revision:** `b0138d7b3124b871668f07e2361dba923bc774d2`
**Test Baseline:** pending — measured before the Pull Request; shared with M211_001
**Baseline evidence:** pending — report path or run URL with revision, commands, passed/failed/skipped counts, and environment
**Depends on:** M211_001 (the sandbox-side tools; a lease with only supervisor tools builds no sandbox, `rustd/crates/afr_agent/src/engine.rs:139-141`) · M211_003 (its reserve counts every held sandbox at its full disk limit, and its two cgroup leaves freeze together) · M213_001 (the Rust runner takes leases; `rustd/crates/agentsfleet_runner/src/main.rs:114-137` refuses them until then)
**Provenance:** LLM-drafted (Claude Opus 5.5, Oct 05, 2026) from a source trace of the chat path at `b0138d7b3`, recorded in Discovery
**Canonical architecture:** `docs/architecture/runner_execution.md` §"Workspace between leases", §Toolbox; `docs/architecture/runner_fleet.md` §"Per-lease renewal — how a long fleet keeps its lease", §"Memory continuity — durable fleet memory rides the trusted plane"

---

## Overview

**Goal (testable):** `test_next_lease_reuses_the_held_sandbox` — a fleet's second lease, within the idle window, reads the file its first lease wrote and finds the process it started still running.
**Problem:** A user asks a fleet in chat to write a script and start a server, then asks for a change. The second message lands in a new sandbox with an empty `/workspace`, on whichever runner claims it first, so the fleet rebuilds everything before it can answer, or answers about files that are gone.
**Solution summary:** When a lease ends processed and used a sandbox, the runner freezes that sandbox and holds it for its fleet for an idle window instead of destroying it. The report and every heartbeat tell the daemon what the runner holds, and the daemon lets that runner claim the fleet's next event first while it is alive. The next lease thaws the sandbox and continues where the last one stopped. Any mismatch, lapse or load falls back to today's fresh sandbox.

## PR Intent & comprehension handshake

- **PR title (eventual):** folded into M211_001's Pull Request
- **Intent (one sentence):** A fleet's follow-up message continues in the sandbox its last message left, files and processes intact, instead of starting over.
- **Handshake** — pending until the implementing agent performs PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `rustd/crates/afr_supervisor/src/lease_loop/workspace.rs` — where a lease's sandbox is prepared and destroyed (`sandboxed`); the hold's take and park go here.
2. `rustd/crates/afr_sandbox/src/warm_slots.rs` — the one existing owner of ready sandboxes: a single task behind a channel, no lock; the hold registry mirrors it.
3. `rustd/crates/afd_fleet/src/lease/sql/lease.rs` — the affinity claim, both releases and the candidate scan the hold changes.
4. `docs/architecture/runner_fleet.md` — line 507's single-live-holder invariant, which a hold must keep.
5. https://docs.kernel.org/admin-guide/cgroup-v2.html — `cgroup.freeze` and the `frozen` key of `cgroup.events`.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `rustd/crates/afr_supervisor/src/lease_loop/workspace.rs` | EDIT | Take the fleet's hold before preparing; park instead of destroying on a processed ending |
| `rustd/crates/afr_supervisor/src/holds.rs` | CREATE | The hold registry: one owning task, keys, deadlines, the cap, release reasons |
| `rustd/crates/afr_supervisor/src/worker_pool.rs` | EDIT | One registry per runner; the last free worker's lease releases the rest; shutdown releases all |
| `rustd/crates/afr_supervisor/src/heartbeat.rs`, `rustd/crates/afr_supervisor/src/report.rs` | EDIT | Send the hold list and `held_until_ms`; destroy what the answer releases |
| `rustd/crates/afr_sandbox/src/engine.rs`, `rustd/crates/afr_sandbox/src/cgroup.rs`, `rustd/crates/afr_sandbox/src/bubblewrap_engine/` | EDIT | `freeze` and `thaw` on the `Sandbox` trait, over `cgroup.freeze` |
| `rustd/crates/afd_core/src/timing.rs` | EDIT | `SANDBOX_HOLD_IDLE_MS` |
| `rustd/crates/afd_wire/src/report.rs`, `rustd/crates/afd_wire/src/runner.rs` | EDIT | The three wire fields in Interfaces |
| `rustd/crates/afd_fleet/src/lease/sql/lease.rs`, `rustd/crates/afd_fleet/src/lease/assign.rs`, `rustd/crates/afd_fleet/src/lease/commit.rs` | EDIT | Scan, claim and unleased release honour a hold; the holder looks at its holds first; the report's release records one |
| `rustd/crates/afd_runner/src/heartbeat.rs`, `rustd/crates/afd_api_runner/src/handler/runner/heartbeat.rs` | EDIT | Reconcile a runner's holds and answer the inactive ones |
| `schema/630_runner_affinity.sql`, or the next numbered migration | EDIT / CREATE | `held_until`, per `docs/SCHEMA_CONVENTIONS.md` at CHORE(open) |
| `rustd/crates/afr_sandbox/examples/kernel_lane/trials.rs`, `rustd/crates/afd_fleet/tests/integration_held_sandbox.rs` | EDIT / CREATE | The kernel and integration proofs |
| `docs/architecture/runner_execution.md`, `docs/architecture/runner_fleet.md` | EDIT | A sandbox may outlive its lease; holder-first claim; the stale `uq_runner_affinity_fleet_id` at lines 29 and 507 |
| `rustd/crates/afd_observability/src/semconv.rs`, `rustd/crates/afd_observability/src/metrics/declared/fleet.rs`, `rustd/crates/afd_observability/src/producers/fleet.rs` | EDIT | The reuse span attribute and the held-claim counter with its producer |
| `docs/v2/pending/M214_001_P1_DOCS_OBS_RUST_RUNNER_EXPORTS_ITS_TELEMETRY.md` | EDIT | The runner census gains the holds family |

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — NDC, UFS (the idle window and the cap are named constants; timing lives in `afd_core::timing`), TGU (the release reason is one enum), LOG, ERR-RS, MSID, ARCH, FLL, TST-NAM.
- `dispatch/write_rust.md` + `docs/RUST_ERROR_STANDARD.md` — every new fallible path through `afd_core::error_shell!`; a thaw failure keeps its cause.
- `docs/LOGGING_STANDARD.md` — the three hold events carry ids and a reason, never paths or contents.
- `dispatch/write_sql.md` + `docs/SCHEMA_CONVENTIONS.md` — the `held_until` column follows the rebuild posture or the additive rule, whichever holds at CHORE(open).
- `dispatch/name_architecture.md` — the hold reverses `runner_execution.md` lines 15 and 158 ("one lease, then destroyed"); the doc edit lands in the docs commit with this spec.
- Indy's Rust bar (Oct 04 and Oct 05, 2026): the registry is a struct that owns its sandboxes behind one task and a `tokio::sync::mpsc` channel, the shape `WarmSlots` uses, so no `Mutex`; `take` and `park` move `Box<dyn Sandbox>` by value, never clone it; freeze and thaw are `Sandbox` trait methods every engine implements; the deadline comes from an injected `Fn() -> Instant`, so tests drive time without sleeping; the release reason is one enum; `afd_core::timing` holds the window; `afd_observability` carries the span attribute and both metric families; no hand-rolled time or channel code.

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

### §1 — Hold the sandbox a processed lease leaves

`sandboxed()` takes the fleet's held sandbox before it prepares one, and parks the sandbox instead of destroying it when the lease ends processed. Park empties `/run/creds`, freezes the cgroup and waits for `frozen 1`, then records the key: fleet, workspace, limits, toolbox digest and network policy. Take requires the whole key to match, thaws, and checks that the executor answers; anything else destroys the hold and prepares fresh. Holds end at their deadline, when the runner's last free worker takes a lease (every hold but the one reused, because a busy runner cannot serve them), at shutdown, and when the daemon names the fleet inactive (§2). At most `worker_count` holds; the oldest goes first. On reuse the supervisor fetches into the existing clone and never touches the working tree, because the fleet's uncommitted edits are what the hold keeps. **Implementation default:** a ten-minute idle window because a chat follow-up usually lands within minutes, and the cap and the freeze bound what a hold costs; Indy may change it.

- **Dimension 1.1** — A processed lease parks its sandbox frozen, with `/run/creds` empty and no renewal sent → Test `test_processed_lease_parks_its_sandbox_frozen`
- **Dimension 1.2** — The fleet's next lease takes the hold: the file reads back and the background process still runs → Test `test_next_lease_reuses_the_held_sandbox`
- **Dimension 1.3** — Another fleet, other limits, another toolbox digest or another network policy destroys the hold and prepares fresh → Test `test_mismatch_destroys_the_hold`
- **Dimension 1.4** — A hold past its deadline is destroyed → Test `test_expired_hold_is_destroyed`
- **Dimension 1.5** — The last free worker's lease releases every other hold → Test `test_saturation_releases_holds`
- **Dimension 1.6** — A failed, interrupted or superseded lease destroys its sandbox, as today → Test `test_failed_lease_never_parks`
- **Dimension 1.7** — Holds never exceed `worker_count`; the oldest is released first → Test `test_holds_capped_oldest_first`
- **Dimension 1.8** — Reuse fetches into the existing clone and leaves the working tree and local branches alone → Test `test_reuse_fetches_without_touching_the_worktree`
- **Dimension 1.9** — A thaw that fails, or an executor silent after thaw, falls back to a fresh sandbox → Test `test_thaw_failure_falls_back_fresh`

### §2 — Tell the daemon what the runner holds

The report carries `held_until_ms` when its lease parked. The report's fencing-guarded release writes it to `fleet.runner_affinity.held_until` in the report's own transaction, so a superseded holder records nothing and destroys what it parked. Every heartbeat carries the fleets the runner holds; the daemon clears `held_until` on that runner's rows the list omits, and answers with the listed fleets that are halted or deleted, which the runner destroys. A runner that releases holds for saturation heartbeats at once rather than on its tick.

- **Dimension 2.1** — A report with `held_until_ms` stores it under the fencing guard; a stale token stores nothing → Test `test_report_stores_hold_under_fencing`
- **Dimension 2.2** — A heartbeat that omits a held fleet clears its hold → Test `test_heartbeat_clears_dropped_holds`
- **Dimension 2.3** — The answer names a held fleet that was halted or deleted, and the runner destroys that hold → Test `test_inactive_fleet_hold_destroyed`
- **Dimension 2.4** — A report or heartbeat without the new fields decodes as today → Test `test_hold_fields_are_optional_on_the_wire`

### §3 — The holder claims first

`SELECT_READY_CANDIDATES` and `CLAIM_AFFINITY_SLOT` skip a fleet another runner holds while `held_until` is in the future and that runner heartbeat within `RUNNER_OFFLINE_AFTER_MS` (`afd_core/src/timing.rs:55`). The holder looks at the fleets it holds before its partition scan, so the next message does not wait for the round-robin (`afd_fleet/src/lease/assign.rs:113`). `RELEASE_UNLEASED_SLOT` keeps the hint while the fleet is held; its starvation guard still covers every fleet not held. Any claim clears `held_until`. A hold is a head start at the slot, never the slot: the holder claims through the same fencing-guarded statement, so the single live holder stays one.

- **Dimension 3.1** — The holder claims its held fleet's next event → Test `test_holder_claims_its_held_fleet`
- **Dimension 3.2** — Another runner skips a held fleet while the holder is live → Test `test_other_runner_skips_held_fleet`
- **Dimension 3.3** — Another runner claims it once the holder misses `RUNNER_OFFLINE_AFTER_MS` or the hold lapses → Test `test_lapsed_hold_is_claimable`
- **Dimension 3.4** — The holder finds its held fleet's event on its next poll, whatever the partition → Test `test_holder_polls_its_holds_first`
- **Dimension 3.5** — An empty claim by the holder keeps the hint while held → Test `test_drained_claim_keeps_held_hint`
- **Dimension 3.6** — A fleet nobody holds is claimed, released and unhinted exactly as before → Test `test_unheld_fleet_claims_as_before`

## Interfaces

```
afd_wire::report::ExecutionResult    + held_until_ms: Option<i64>    absent = nothing held; decoded leniently
afd_wire::runner::HeartbeatRequest   + holds: Vec<fleet id>           every fleet the runner holds now
afd_wire::runner::HeartbeatResponse  + release_holds: Vec<fleet id>   held fleets that are halted or deleted
fleet.runner_affinity                + held_until BIGINT NULL         milliseconds since the epoch; NULL = not held
afr_sandbox::Sandbox                 + freeze(), thaw()               cgroup.freeze, settled on cgroup.events
afr_supervisor holds                 take(key) · park(key, sandbox) · release(fleet, reason)
reason                               Expired | Saturated | Mismatch | Inactive | Shutdown | ThawFailed | Superseded
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Holder dies | Crash or host loss mid-hold | Other runners claim after `RUNNER_OFFLINE_AFTER_MS`; the boot sweep removes the sandbox (`bubblewrap_engine/sweep.rs:25`); the user waits at most that long |
| Holder saturated | Its last free worker took a lease | Holds released, heartbeat sent at once; the fleet is claimable everywhere |
| Key mismatch | Limits, policy or toolbox changed between leases | Hold destroyed, fresh sandbox, `Mismatch` logged; the reply carries no stale state |
| Thaw fails | cgroup error, or the executor died while frozen | Hold destroyed, fresh sandbox, `ThawFailed` logged with its cause |
| Superseded report | The lease was reclaimed and the fencing token bumped | Report refused as today; the runner destroys what it parked (`Superseded`) |
| Fleet halted or deleted | Owner action during the hold | Named in the next heartbeat answer; the runner destroys the hold and its tenant data |
| Too many fleets | More holds than workers | The oldest hold is released |
| Two events at once | Back-to-back messages | The affinity slot admits one holder, unchanged; the second waits for the first's report (`test_unheld_fleet_claims_as_before`) |
| Host disk filling | Held workspace disks keep their bytes | M211_003 §4 counts a held sandbox as live, so the host keeps room for it at its full disk limit plus the reserve (`test_capacity_rule_counts_remaining_limits`); the cap bounds holds to `worker_count` (`test_holds_capped_oldest_first`) |

## Invariants

1. A held sandbox serves only its own fleet — take compares fleet and workspace ids; `test_mismatch_destroys_the_hold`.
2. No credential outlives its lease — park empties `/run/creds` before freezing, and take refuses a non-empty one; `test_processed_lease_parks_its_sandbox_frozen`.
3. A held sandbox runs nothing — its cgroup reads `frozen 1` from park to take; same test.
4. A hold never extends a lease — the report goes out at lease end and no renewal is sent for a held fleet; same test.
5. At most `worker_count` holds per runner — the registry releases the oldest first; `test_holds_capped_oldest_first`.
6. Only the holder claims a held fleet while it is live — the claim statement's own condition; `test_other_runner_skips_held_fleet`.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| `sandbox_held` (runner log, info) | ops | A processed lease parks its sandbox | lease id, fleet id, deadline | no paths, no file contents, no credentials | `test_processed_lease_parks_its_sandbox_frozen` |
| `sandbox_reused` (runner log, info) | ops | A lease takes a hold | lease id, fleet id, milliseconds held | as above | `test_next_lease_reuses_the_held_sandbox` |
| `sandbox_hold_released` (runner log, info) | ops | A hold ends without reuse | fleet id, reason | as above | `test_expired_hold_is_destroyed` |
| `agentsfleet.sandbox.reused` on the `runner.lease` span (a new `afd_observability::semconv` constant) | ops | Every lease that used a sandbox | `true` when taken from a hold | ids only | `test_next_lease_reuses_the_held_sandbox` |
| `agentsfleet_lease_held_claims_total` (daemon, `afd_observability` declared counter + producer) | ops | A claim on a fleet that was held | `outcome`: `holder` or `other_after_lapse` | no ids in labels | `test_holder_claims_its_held_fleet`, `test_lapsed_hold_is_claimable` |
| `agentsfleet_runner_sandbox_holds_total` (runner census, M214_001) | ops | A hold is parked, reused or released | `outcome`: `parked`, `reused`, or the release reason | closed label set | M214_001's producer-coverage test |

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | kernel | `test_processed_lease_parks_its_sandbox_frozen` | lease ends processed → `cgroup.events` reads `frozen 1`, `/run/creds` empty, no renew after the report |
| 1.2 | kernel | `test_next_lease_reuses_the_held_sandbox` | lease A writes `/workspace/a` and starts `sleep 600 &` → lease B reads `a`, finds the sleeper alive |
| 1.3 | unit | `test_mismatch_destroys_the_hold` | each of five key fields changed in turn → the hold destroyed, `Mismatch`, a fresh sandbox prepared |
| 1.4 | unit | `test_expired_hold_is_destroyed` | fake clock past the deadline → destroyed, `Expired` |
| 1.5 | unit | `test_saturation_releases_holds` | two workers, two holds; both workers take leases → zero holds, an immediate heartbeat |
| 1.6 | unit | `test_failed_lease_never_parks` | endings failed and interrupted, and a report answered superseded → destroy called, no hold |
| 1.7 | unit | `test_holds_capped_oldest_first` | `worker_count` 2, three parks → the first released, two remain |
| 1.8 | kernel | `test_reuse_fetches_without_touching_the_worktree` | an uncommitted edit and a local commit survive the fetch; the remote ref moves |
| 1.9 | unit | `test_thaw_failure_falls_back_fresh` | thaw errors → `ThawFailed` with its cause, a fresh sandbox, the lease runs |
| 2.1 | integration | `test_report_stores_hold_under_fencing` | current token → `held_until` stored; bumped token → row unchanged |
| 2.2 | integration | `test_heartbeat_clears_dropped_holds` | holds `[f1, f2]`, then `[f1]` → `f2.held_until` NULL |
| 2.3 | integration | `test_inactive_fleet_hold_destroyed` | `f1` halted → answer names `f1`; the fake runner destroys it |
| 2.4 | unit | `test_hold_fields_are_optional_on_the_wire` | a report and a heartbeat without the fields → decode with nothing held |
| 3.1 | integration | `test_holder_claims_its_held_fleet` | runner A holds `f1`; event on `f1` → A's poll leases it |
| 3.2 | integration | `test_other_runner_skips_held_fleet` | B polls first → no lease for `f1`; A then claims it |
| 3.3 | integration | `test_lapsed_hold_is_claimable` | A's heartbeat older than `RUNNER_OFFLINE_AFTER_MS`, or the hold past → B leases `f1` |
| 3.4 | integration | `test_holder_polls_its_holds_first` | `f1` in a partition A's cursor has not reached → A's next poll leases `f1` |
| 3.5 | integration | `test_drained_claim_keeps_held_hint` | A claims held `f1` with nothing ready → `last_runner_id` stays A |
| 3.6 | integration | `test_unheld_fleet_claims_as_before` | no hold anywhere → claim order, release and hint clearing unchanged |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | A fleet's next lease continues in its held sandbox, frozen and credential-free between (§1) | `make test-runner-kernel 2>&1 \| grep -c "test_next_lease_reuses_the_held_sandbox ... ok"` | 1 | P0 | |
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
- **Metrics review** — Three runner log events, exported with the rest by M214_001; no analytics or funnel playbook change, because no product event changes.
- **Skill-chain outcomes** — pending.
- **Deferrals** — none.
