<!--
SPEC AUTHORING RULES (load-bearing — the one comment that survives):
- Body order = the executing agent's read order. Fill via the orly-spec-new
  skill (authoring order lives there); after filling, DELETE every "tpl:"
  guidance comment — the SPEC TEMPLATE GATE blocks tpl residue, unfilled
  {slots}, and missing required sections (.orly/audits/spec-template.sh --staged).
- No time/effort/hour/day estimates anywhere. No effort columns, complexity
  ratings, percentage-complete, implementation dates, assigned owners.
- Priority (P0/P1/P2/P3) is the only sizing signal; Dependencies are the only
  sequencing signal. A section that contradicts these rules loses — delete it.
-->

# M225_001: A steer is leased within 250 ms at p50, and every settled lease logs where its time went

**Prototype:** v2.0.0
**Milestone:** M225
**Workstream:** 001
**Date:** Oct 12, 2026
**Status:** PENDING
**Priority:** P1 — every steer waits about 2 s, up to about 4 s, before any runner looks at it
**Categories:** API, OBS
**Batch:** B1 — one Pull Request; the wake hint and the phase timings ship together so the second measures the first
**Branch:** pending — set at CHORE(open)
**Baseline revision:** pending — record the full comparison commit at CHORE(open)
**Test Baseline:** pending — measure declared unit and integration lanes before the Pull Request
**Baseline evidence:** pending — report path or run URL with revision, commands, passed/failed/skipped counts, and environment
**Depends on:** none
**Provenance:** LLM-drafted (Claude Opus 5.5, Oct 12, 2026) from dev measurements during M213_001's close-out (runner journal on `zombie-dev-worker-ant`, `flyctl logs` on the dev API) and `rustd/crates/afd_fleet/src/lease/assign.rs`
**Canonical architecture:** `docs/architecture/runner_fleet.md` §Datastore topology (the readiness index); `docs/architecture/lease_flow.md`; `docs/architecture/scaling.md` (idle poll cost)

---

## Overview

**Goal (testable):** `test_marked_fleet_is_leased_on_the_next_poll` — a fleet marked ready is offered on the very next lease poll whichever partition the cursor names, and an idle poll costs two Dragonfly commands (the dirty read and one partition peek) and no Postgres statement.
**Problem:** A fleet owner sends a steer and nothing happens for 2–4 s before the model even starts. Measured on dev, Oct 12, 2026: pickup took 1.9 s, 2.2 s, 2.95 s and 3.8 s across four steers; in the 3.8 s case the steer was admitted at 03:34:23.232 and leased at 03:34:26.178, after 11 polls answered `no leasable work`. The reply itself took `wall_ms` 1,285 for a one-line chat, and nothing says how much of that was sandbox, model or report.
**Solution summary:** Each poll reads one of `READY_PARTITIONS` (16) readiness hashes in rotation (`afd_fleet/src/lease/assign.rs`), and four workers on the 1 s `NO_WORK_RETRY_AFTER_MS` turn the rotation once in about 4 s. Admission now also marks the fleet's partition dirty in one small key; a poll reads that key first and peeks a dirty partition before falling back to the rotation. The runner's `lease_completed` line gains per-phase durations, so the time a reply takes is readable without a collector.

## PR Intent & comprehension handshake

- **PR title (eventual):** perf(lease): wake the poll on fresh work and log each lease's phases
- **Intent (one sentence):** A steer starts running within a quarter second, and an operator can read where the rest of the reply time went.
- **Handshake** — pending until the implementing agent performs PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `rustd/crates/afd_dragonfly/src/ready.rs` — the readiness index: the hint-never-record rule, minted tokens and compare-and-clear; the dirty key follows the same rules.
2. `rustd/crates/afd_fleet/src/lease/assign.rs` — the ready-first poll and its cost tally (`PollCost`), which the new read must keep honest.
3. `docs/architecture/runner_fleet.md` §Datastore topology — the sixteen hashes, why they are partitioned on a cluster, and the reclaim sweeper that repairs a lost mark.
4. `rustd/crates/afr_supervisor/src/lease_loop.rs` — where `lease_completed` is logged.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `rustd/crates/afd_dragonfly/src/ready.rs` | EDIT | `mark` also flags the partition dirty; a read and a compare-and-clear for the dirty set |
| `rustd/crates/afd_dragonfly/src/ready/partition.rs` | EDIT | the dirty key's name beside `READY_INDEX_KEY` |
| `rustd/crates/afd_dragonfly/src/ready/tests.rs` | EDIT | dirty-set unit tests |
| `rustd/crates/afd_fleet/src/lease/assign.rs` | EDIT | read dirty partitions before the rotation; tally the read in `PollCost` |
| `rustd/crates/afd_fleet/src/lease/affinity_tests.rs` | EDIT | the next-poll and idle-cost proofs |
| `rustd/crates/afr_supervisor/src/lease_loop.rs` | EDIT | phase durations on `lease_completed` |
| `rustd/crates/afr_supervisor/src/lease_telemetry_tests.rs` | EDIT | the phase-field proof |
| `docs/architecture/runner_fleet.md` | EDIT | the dirty key in the datastore table |
| `docs/architecture/lease_flow.md` | EDIT | the poll step reads the dirty key first |
| `docs/architecture/scaling.md` | EDIT | idle poll cost restated: two Dragonfly reads, zero Postgres |

## Applicable Rules

- **`.orly/docs/greptile-learnings/RULES.md`** — UFS (the dirty key and every duration field are named constants), NDC, NLR, FLL.
- `.orly/dispatch/write_rust.md` — preserved error variants, deterministic concurrency tests for the mark-versus-clear race.
- `docs/RUST_ERROR_STANDARD.md` — any new fallible signature in `afd_dragonfly`.
- `.orly/dispatch/name_architecture.md` — a new Dragonfly key is named against `docs/architecture/runner_fleet.md` before it is written.
- `.orly/docs/LOGGING_STANDARD.md` — the new `lease_completed` fields.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| LOGGING | yes — new fields on an existing event | field names from the logging standard; no lease content, only durations |
| UFS | yes — new key and field names | named constants beside the existing ones |
| Architecture consult | yes — new Dragonfly key | the doc edit lands in the same commit as the key |
| File & Function Length (≤350/≤50/≤70) | yes — `assign.rs`, `ready.rs` | split the dirty-set helpers into `ready/dirty.rs` if `ready.rs` nears the cap |

## Prior-Art / Reference Implementations

- **Reference:** `rustd/crates/afd_dragonfly/src/ready.rs` `clear_if_unchanged` — the dirty set clears the same way, so a mark landing between a poll's read and its clear survives.

## Sections (implementation slices)

### §1 — The poll finds fresh work on its next read

**Implementation default:** one dirty key per deployment holding the set of partition numbers that gained a mark, read with one command before the rotation peek; a poll that finds its dirty partition empty clears that bit only if no newer mark set it. Rejected: a server-held long poll, because it needs cross-machine wake-ups through Dragonfly and holds a request per worker (Indy, Oct 12, 2026: "New spec, wake hint (Recommended)").

- **Dimension 1.1** — A fleet marked in any partition is offered on the next poll, whichever partition the cursor names → Test `test_marked_fleet_is_leased_on_the_next_poll`
- **Dimension 1.2** — An idle poll with an empty dirty set costs exactly two Dragonfly commands, the dirty read and one rotation peek, and zero Postgres statements; the peek stays because it is Dimension 1.4's fallback for a lost dirty bit → Test `test_idle_poll_cost_is_bounded`
- **Dimension 1.3** — A mark written between a poll's dirty read and its clear survives the clear → Test `test_mark_racing_a_clear_survives`
- **Dimension 1.4** — A lost dirty bit costs only latency: the rotation still finds the fleet within one full turn → Test `test_lost_dirty_bit_falls_back_to_rotation`

### §2 — Every settled lease logs its phases

- **Dimension 2.1** — `lease_completed` carries `queue_wait_ms` (event creation to lease), `bundle_ms`, `sandbox_ms`, `first_token_ms`, `model_ms`, `tools_ms` and `report_ms`, and `bundle_ms + sandbox_ms + model_ms + tools_ms + report_ms` is within 5 ms of the lease's own wall time (`queue_wait_ms` precedes the lease; `first_token_ms` is part of `model_ms`) → Test `test_lease_completed_logs_its_phases`
- **Dimension 2.2** — A lease that fails before a phase starts logs that phase as absent, not zero → Test `test_unstarted_phase_is_absent`

### §3 — Measured on dev

- **Dimension 3.1** — Ten steers on dev after deploy: pickup p50 ≤250 ms and max ≤1,000 ms, read from `queue_wait_ms` → Test `test_dev_pickup_p50_under_250ms`

## Interfaces

```
lease_completed  lease_id=… queue_wait_ms=… bundle_ms=… sandbox_ms=… first_token_ms=… model_ms=… tools_ms=… report_ms=…
Dragonfly: fleet:ready:{p} (unchanged) + one dirty key named in runner_fleet.md
Wire: LeaseResponse unchanged; retry_after_ms unchanged
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Dirty write fails | Dragonfly error on the admission path | logged and ignored; the admission succeeds; the rotation finds the fleet within one turn |
| Dirty read fails | Dragonfly error on the poll | the poll falls back to the rotation peek and counts the failure |
| Mark races a clear | admission marks while a poll clears | compare-and-clear leaves the newer bit set |
| Stale dirty bit | the fleet was drained by another runner | the peek finds nothing and clears the bit with its token |

## Invariants

1. The readiness index and the dirty key are hints; the streams are the record — enforced by the reclaim sweeper re-deriving readiness, already proven, and by Dimension 1.4.
2. An idle poll touches no Postgres — enforced by the `PollCost` tally asserted in Dimension 1.2.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| `lease_completed` (new fields) | ops | a runner settles a lease | the seven phase durations, lease identifier | durations only — no prompt, reply or tool content | `test_lease_completed_logs_its_phases` |

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | integration | `test_marked_fleet_is_leased_on_the_next_poll` | mark a fleet in partition 9, cursor at 0 → the next poll leases it |
| 1.2 | integration | `test_idle_poll_cost_is_bounded` | empty index and empty dirty set → `PollCost` reads 2 Dragonfly commands, 0 Postgres round trips |
| 1.3 | integration | `test_mark_racing_a_clear_survives` | clear with the stale token after a new mark → the bit remains |
| 1.4 | integration | `test_lost_dirty_bit_falls_back_to_rotation` | mark with the dirty write failed → leased within 16 polls |
| 2.1 | unit | `test_lease_completed_logs_its_phases` | a fake daemon lease → seven fields present; the five in-lease phases sum within 5 ms of wall time |
| 2.2 | unit | `test_unstarted_phase_is_absent` | a refusal before sandbox start → `sandbox_ms` absent |
| 3.1 | manual | `test_dev_pickup_p50_under_250ms` | ten steers → p50 ≤250 ms, max ≤1,000 ms; journal lines pasted |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Fresh work is leased on the next poll (§1) | `make test-integration-rustd` | exit 0, `test_marked_fleet_is_leased_on_the_next_poll` passed | P0 | |
| R2 | Idle poll cost bounded at two Dragonfly reads and no Postgres (§1) | `make test-integration-rustd` | `test_idle_poll_cost_is_bounded` passed | P0 | |
| R3 | Each lease logs its phases (§2) | `make test-unit-all` | `test_lease_completed_logs_its_phases` passed | P0 | |
| R4 | Dev pickup p50 ≤250 ms (§3) | manual — ten `queue_wait_ms` values in Session Notes | p50 ≤250, max ≤1,000 | P0 | |
| R5 | Diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 paths missing from the Files Changed table | P0 | |
| S1 | Conform gates green | `make harness-verify` | exit 0 | P0 | |
| S2 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | |
| S3 | Lint green | `make lint-all` | exit 0 | P0 | |
| S4 | Integration green | `make test-integration-rustd` | exit 0 | P0 | |
| S5 | Version in sync | `make check-version` | exit 0 | P0 | |
| S6 | No secrets | `gitleaks detect` | exit 0 | P0 | |

**Command source rule:** copy every declared `conform` and `verify.*` invocation from `.orly/orly.json` into a Verify cell, verbatim, with an Expected value. Include conditional suites; the final gate decides applicability from the actual branch diff. Additional spec-specific commands, secret scans, and named manual checks are allowed. Missing configuration must be completed before authoring. See `.orly/dispatch/lifecycle.md` for command timing; baseline metadata is pending at opening and measured before the Pull Request.

**Grading protocol (VERIFY):** run each spec-specific Verify command verbatim; Graded = ✅/❌ + one decisive output line. Repository-command rows point to the final `orly gate pr` results in Pull Request Session Notes. **Ship gate:** every required check must pass before the Pull Request is ready; missing evidence or any ❌ returns to EXECUTE. A P1 ❌ requires an Indy-acked deferral quote in Discovery. A P0 may be MOVED only under the transfer rule in `.orly/docs/TEMPLATE.md`.

## Dead Code Sweep

N/A — no files deleted.

## Out of Scope

- The model's own time: a one-line chat spent `wall_ms` 1,285 on dev; §2 measures how much is time to first token, and a faster model or provider is a catalogue decision, not this spec.
- Runner OTLP export on dev (`telemetry_export_disabled`): the runner's host collector is "built later" per `docs/architecture/observability.md`.
- A refused egress write costing 167,429 tokens and 166 s as the model retried variants (dev event `1791777420543-1356`); recorded for Indy to place.

## Product Clarity (authoring record)

1. **Successful user moment** — A fleet owner sends "how are you" and the reply starts in about the time the model takes, with no dead pause first.
2. **Preserved user behaviour** — Every steer, webhook and schedule still delivers exactly once; the wire does not change.
3. **Optimal-way check** — A long poll would cut pickup to one round trip (about 60 ms); the wake hint gets to about 250 ms with one extra read and no held requests, which Indy chose.
4. **Rebuild-vs-iterate** — Iterate: the readiness index stays; one key joins it.
5. **What we build** — A dirty-partition key, a poll that reads it first, seven phase fields.
6. **What we do NOT build** — A long poll; runner OTLP on dev; model or provider changes.
7. **Fit with existing features** — Idle poll cost rises from one Dragonfly read to two and stays at zero Postgres; `docs/architecture/scaling.md` restates the budget in the same Pull Request.
8. **Surface order** — N/A — no user surface; operators read the journal.
9. **Dashboard restraint** — N/A — no dashboard change.
10. **Confused-user next step** — An operator greps the lease's `lease_completed` line and reads which phase was slow.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** the wake hint and the phase timings ship together, because §3's proof reads its numbers from §2's fields.
- **Alternatives considered:** a server-held long poll (rejected by Indy for its moving parts); fewer partitions (rejected: it shortens the rotation but keeps the wait proportional to it); a shorter idle back-off (rejected: it multiplies idle poll cost across every runner).
- **Patch-vs-refactor verdict:** this is a **patch** because the readiness index and the poll keep their shape and gain one read.

## Discovery (consult log)

- **Consults** — Measured on dev, Oct 12, 2026, during M213_001's close-out: four steers picked up in 1.9 s, 2.2 s, 2.95 s and 3.8 s; four workers poll about once a second each (`runner_lease_no_work` bursts in `flyctl logs`); the dev host reaches the API in about 60 ms, so the network is not the wait. `READY_PARTITIONS` is 16 (`afd_dragonfly/src/ready/partition.rs:31`) and `NO_WORK_RETRY_AFTER_MS` is 1,000 (`afd_core/src/timing.rs:75`).
  > Indy (2026-10-12): "How do we make this performant? and reduce 1.2s to may be 500ms? What is causign the 1.2s, can we add logging tracing to the code to see what happens?" — context: the request this spec answers.
  > Indy (2026-10-12): "New spec, wake hint (Recommended)" — context: the mechanism, over a long poll.
- **Metrics review** — `lease_completed` gains seven duration fields; no analytics or funnel playbook update required: operator log only.
- **Skill-chain outcomes** — pending.
- **Deferrals** — none.
