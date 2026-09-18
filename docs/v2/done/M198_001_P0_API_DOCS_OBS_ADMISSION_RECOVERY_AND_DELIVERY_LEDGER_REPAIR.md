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

# M198_001: Accepted work always reaches a runner — recovery rotation, a delivery index that is reachable, an attempt count that counts attempts

**Prototype:** v2.0.0
**Milestone:** M198
**Workstream:** 001
**Date:** Sep 18, 2026
**Status:** DONE
**Priority:** P0 — accepted work a producer was told yes about can stay unrecovered forever on a busy deployment, and the operator counter that would show it records only successes.
**Categories:** API, DOCS, OBS
**Batch:** B1 — no concurrent workstream; the admission and outbound ledgers are edited by nothing else in flight.
**Branch:** `fix/m198-admission-recovery-and-delivery-ledger`
**Baseline revision:** `6813000c34cb0a7eff6f3d42ac7c96a1f62e9ff1` (`origin/main`; the branch was rebased onto it before the Pull Request, superseding the opening baseline at `eaa2b1956`)
**Test Baseline:** Rust unit 2,610 passed / 0 failed; integration 493 passed (492 parallel + 1 exclusive) / 0 failed, measured on `6813000c3` in a detached worktree. TypeScript lanes unmeasured — this branch changes no `ui/**` file, and the baseline worktree lacks the per-package `bun install` that lane needs.
**Baseline evidence:** isolated worktree at the baseline revision, `make test-unit-rustd` and `make test-integration-rustd`, against its own compose Postgres and Dragonfly on a separate project and port range; logs `.baseline-unit.log` and `.baseline-integration.log` in that worktree. Local run, not Continuous Integration (CI).
**Depends on:** none
**Provenance:** LLM-drafted (claude-opus-5, Sep 18, 2026), grounded in `docs/v2/reviews/schema-usage-audit-2026-09-18.md`, `docs/v2/reviews/schema-fix-adversarial-review-2026-09-18.md`, and re-verified source reads at the commit recorded below
**Canonical architecture:** `docs/architecture/data_flow.md` §The durable ledgers

---

## Overview

**Goal (testable):** every receipted-but-undelivered admission is eventually probed and repaired regardless of how many fleets hold undelivered work or how many rows one fleet lost, the admission delivery stamp reaches a usable index, the outbound attempt counter records delivery-cycle starts including failures, and no shipped document claims behaviour the daemon does not have.

**Problem:** a producer is told yes, the queue loses the entry, and the work never runs. Two separate caps make that permanent rather than slow. On a deployment where more than one hundred and twenty-eight fleets hold undelivered work, the recovery pass examines the same lowest-sorting fleets every time and a fleet sorting after them is never examined at all. Within one fleet, a loss larger than thirty-two rows is repaired thirty-two at a time, and because the pass decides a fleet is healthy by asking about its *oldest* undelivered entry, the rows the previous pass restored answer "still here" and hide the ones it never reached. Separately, an operator looking for evidence of repeated delivery failure finds a counter that only ever moved on success, and an operator reading the architecture page is told about row-level security and an execution handle that do not exist in this repository.

**Solution summary:** the reconciliation pass gains a rotating fleet cursor so every fleet with undelivered work is examined within a bounded number of passes, and a bounded set of fleets under repair that resume mid-fleet from a row key instead of restarting at a head that recovery has already made healthy. A new migration slot adds the index the delivery stamp can actually use, without changing the stamp's predicate or the reconciliation read. The outbound worker records a delivery-cycle start when it accepts a job rather than incrementing on the success stamp, and reads the resulting count into the structured events it already emits. Three documents stop describing absent features. Nothing is dropped, no table is deleted, and no missing feature is restored.

## PR Intent & comprehension handshake

- **PR title (eventual):** fix(admission,outbound): recover every lost admission, count every delivery attempt
- **Intent (one sentence):** accepted work the queue lost is recovered no matter how much of it there is, and the counters and pages an operator reads while that happens tell the truth.
- **Handshake** — pending until the implementing agent performs PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `rustd/crates/afd_admission/src/reconcile.rs` — the pass being repaired; its module header states the invariants the repair must not break (probe failure keeps the receipt, replay owns every append, voiding spends the replay budget on purpose).
2. `rustd/crates/afd_admission/src/sql.rs` — the four statements the pass runs, each carrying the reason its predicate is shaped the way it is. `VOID_LOST_RECEIPT`'s compare-and-set is what lets the reads go unlocked.
3. `rustd/crates/afd_runner/src/sweep/reclaim.rs` — prior art for a cursor held by a sweeper across passes, including its compile-time bound assertions. Prior art, not a drop-in.
4. `docs/v2/reviews/schema-fix-adversarial-review-2026-09-18.md` — the dispositions that supersede the original audit, including every remedy explicitly rejected.
5. `docs/SCHEMA_CONVENTIONS.md` — the migration-slot rule and index conventions slot 914 matches.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `rustd/crates/afd_admission/src/sql.rs` | EDIT | The fleet scan gains a cursor bound; the per-fleet scan gains a row cursor bound. |
| `rustd/crates/afd_admission/src/{reconcile.rs,lib.rs}` | EDIT | The pass iterates from a caller-held resume point; the crate exports it. |
| `rustd/crates/afd_admission/src/reconcile/{progress.rs,progress/tests.rs,scan.rs}` | CREATE | The resume state with its wrap and overflow rules and their datastore-free proof, and the pass's reads split out so each cursor binding reads beside its decoding. |
| `rustd/crates/afd_runner/src/sweep/reconcile.rs` (+ `reconcile/tests.rs`, `Cargo.toml`) | EDIT | The sweeper owns the resume state between passes and never holds its lock across an await; its pacing tests extend to the cursor hand-off, and reaching that hand-off adds three dev-dependencies. |
| `schema/914_fleet_admissions_delivery_lookup.sql` | CREATE | The index the delivery stamp can use, as a new forward slot. |
| `rustd/crates/afd_db/src/migration.rs` · `rustd/crates/afd_connector/src/sql.rs` | EDIT | Slot 914 is registered; the dead `SELECT_INSTALL_WORKSPACE` definition, its test entry and the doc link naming it are removed. |
| `rustd/crates/afd_fleet_lifecycle/src/{sql.rs,install/row.rs}` (+ `tests/integration_install_rollback.rs`) | EDIT | `INSERT_FLEET` stops writing the unread bundle pointer; the bind, the private key helper and its test go with it, and later parameters renumber. The rollback suite gains the case proving the one parameter after the removed bind still lands in its own columns. |
| `rustd/crates/afd_outbound/src/obligation{,/sql}.rs` | EDIT | The success stamp stops incrementing; a delivery-cycle start statement and its wrapper are added. |
| `rustd/crates/afd_outbound/src/lanes.rs` | EDIT | Records the cycle start on job acceptance; carries the count into the delivered and exhausted events. |
| `rustd/crates/afd_outbound/tests/{integration_obligations.rs,integration_attempt_count.rs}` | EDIT · CREATE | The old meaning's assertion is replaced; the new suite carries the counter's failure, retry, duplicate, shutdown and pacing cases. |
| `rustd/crates/afd_fleet/tests/{integration_admission_recovery.rs,integration_recovery_outage.rs}` | EDIT | Helpers open to the new suite; both reconcile calls carry the resume state. |
| `rustd/crates/afd_fleet/tests/{integration_recovery_progress.rs,integration_delivery_lookup.rs}` (+ `fleet_suite.rs`) | CREATE | The two batch-boundary reproductions and the deleted-fleet cursor case in one file, the index definition and the stamp either side of a replayed receipt in the other: the sibling suite is already at length, and the second grades the ledger's shape rather than a loss. |
| `rustd/crates/afd_api_ingress/src/handler/webhook/app_route.rs` (+ `public/openapi.json`) | EDIT | The generated endpoint description stops claiming repair writers this daemon does not have; the published artifact is regenerated from it. |
| `rustd/crates/afd_api/tests/app_ingress_route.rs` (+ `tests/fixtures/webhooks/github_deployment_status_app.json`) | EDIT · CREATE | The dropped `deployment_status` the description now admits, proven against a real payload. |
| `rustd/crates/afd_api/tests/openapi_contract.rs` · `scripts/check_architecture_doc.sh` | EDIT | Each absent mechanism gains a gate, so the claim cannot come back silently. |
| `codecov.yml` · `VERSION` · `build.zig.zon` · `cli/package.json` · `rustd/{Cargo.toml,Cargo.lock}` | EDIT | Two unrelated numbers. The `rust-afd` patch target moves 97 -> 99, alone among the three because it grades only added lines. The version moves 0.48.0 -> 0.49.0 on Indy's instruction in a dedicated `chore(release)` commit: the first four are what `make check-version` compares, and the lock renumbers because member crates inherit `version.workspace = true`. |
| `docs/architecture/{data_flow.md,runner_fleet.md}` | EDIT | Corrects the session execution-handle rows, the multi-tenancy row, and a command neither binary has. |
| `docs/v2/{pending,active,done}/M198_001_P0_API_DOCS_OBS_ADMISSION_RECOVERY_AND_DELIVERY_LEDGER_REPAIR.md` (+ `docs/v2/reviews/schema-fix-adversarial-review-2026-09-18.md`) | CREATE | This spec, at whichever lifecycle directory holds it, beside the adversarial review whose dispositions it implements. |

Consulted, **not** edited: `schema/910_fleet_admissions.sql` and `schema/510_fleet_sessions.sql` are shipped slots and stay frozen; `afd_ingress`'s `sql.rs` and `app.rs` carry the live install read; `afd_library/src/prepare.rs` and `afd_fleet/src/bundle/mod.rs` define the live bundle layout.

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — **UFS** (every new command name, event name and bound is a named constant, spelled once), **NDC** (no definition lands without a caller — the new statements and the cycle-start wrapper each get one in the same diff), **NLR** (the dead connector constant and the dead bundle-key helper go rather than being left beside their replacements), **ORP** (orphan sweep over both removed symbols), **TST-NAM** (no milestone identifier in a test name).
- **`dispatch/write_rust.md`** — every touched file is `*.rs`: ownership across the cursor hand-off, no synchronous mutex held across an await, preserved error variants, deterministic concurrency tests.
- **`dispatch/write_sql.md`** — the new migration slot and the Schema Table Removal Guard, which this diff must not trip: no `DROP`, no `ALTER` of a shipped column. **`dispatch/write_documentation.md`** and **`docs/DOCUMENTATION_RULES.md`** cover the generated endpoint description and the architecture page corrections.
- **`docs/RUST_ERROR_STANDARD.md`** — read before any fallible signature changes; the cycle-start recorder is fallible and must not hand-write an error type.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| `write_rust` (judgment) | yes — every code file in the table is `*.rs` | Resume state is moved in and out of the sweeper's mutex around the pass, never locked across an await; the bounded set has a compile-time cap assertion in the style `sweep/reconcile.rs` already uses. |
| `SCHEMA GUARD` | yes — a new file under `schema/` | New forward slot only. No `DROP`, no `ALTER`, no edit to a shipped slot, no migration-array reordering; slot 914 appends. |
| `LENGTH GATE` (≤350 file / ≤50 fn) | yes | `reconcile.rs` is near its working size already, so the resume state lands in its own `reconcile/progress.rs` module rather than growing the pass. Function caps met by keeping the fleet tier and the resume tier as separate private methods. |
| `LOGGING GATE` | yes — new and changed structured events | Existing event-name constants are reused; the new operator-visible facts ride existing events as fields wherever an event already fires. |
| `MILESTONE ID GATE` · `UFS GATE` | yes — `*.rs` and `*.sql` saves | No milestone identifier, section reference or dimension token in source or test names. Every new bound, statement name and field key is one named constant with one spelling. |
| `GREPTILE GATE` · `SPEC TEMPLATE GATE` | yes | End-of-turn read against the rule identifiers above; this file is authored from `docs/TEMPLATE.md` with every guidance comment deleted. |
| `DESIGN TOKEN GATE` / `UI GATE` / `ZIG GATE` | no — no `*.tsx`, `*.ts` or `*.zig` in the table | N/A. |
| `Architecture consult` | yes — the pass's iteration rule is architecture | `docs/architecture/data_flow.md` is read and its recovery description updated in the same diff. |

## Prior-Art / Reference Implementations

- **Reference:** `rustd/crates/afd_runner/src/sweep/reclaim.rs` — a sweeper holding a cursor across passes in this repository, including the compile-time assertion block that bounds its limits. This spec mirrors the state-ownership shape and diverges on wrap policy, which reclaim does not need.
- **Reference:** `schema/620_runner_lease_indexes.sql` and `schema/720_usage_ledger_indexes.sql` — the repository's existing index-only forward slots, for the comment shape slot 914 matches.

## Sections (implementation slices)

### §1 — Recovery reaches every fleet

The reconciliation pass rotates through fleets holding undelivered work instead of restarting at the lowest-sorting one. **Implementation default:** a keyset bound `fleet_id > $2::uuid` on the existing scan, because the scan's `DISTINCT ON` already orders by `fleet_id` and the bound rides the same index prefix rather than adding a sort.

- **Dimension 1.1** (DONE) — a deployment with more fleets holding undelivered work than one pass examines visits every one across consecutive passes, and a pass reading fewer fleets than its cap wraps to the start so the rotation strands nobody → Test `rotation_visits_every_unfinished_fleet`
- **Dimension 1.3** (DONE) — lost work on a fleet sorting after the cap is recovered within a bounded number of passes → Test `rotation_recovers_fleets_past_the_pass_budget`
- **Dimension 1.4** (DONE) — a cursor naming a fleet whose row is gone resumes at the next fleet rather than stalling or restarting, graded against the live scan and not only against the resume state's hand-back → Test `a_deleted_fleet_does_not_strand_the_ones_after_it`

### §2 — Recovery reaches every row on a lost fleet

A fleet whose walk filled its row batch is remembered as under repair and resumes from the row key it stopped at, bypassing the head probe that recovery has just made healthy. **Implementation default:** the resume set is bounded by the same constant that bounds fleets per pass, because a pass cannot put more fleets into repair than it examined; overflow declines to add rather than evicting a fleet mid-repair, and the declining is logged.

- **Dimension 2.1** (DONE) — a loss larger than one row batch on a single fleet is fully recovered across consecutive passes, including the rows after the first batch → Test `a_walk_resumes_past_the_rows_it_already_repaired`
- **Dimension 2.2** (DONE) — a fleet under repair is walked from its resume key even when its oldest undelivered receipt is live, so restored rows cannot hide later lost ones → Test `a_walk_resumes_past_the_rows_it_already_repaired`
- **Dimension 2.4** (DONE) — lost rows interleaved with live ones admitted after the loss are still recovered, and live ones are never voided → Test `interleaved_live_admissions_survive_recovery`
- **Dimension 2.5** (DONE) — resume state is per-process and is lost on restart; a restarted process still recovers every row, through the head probe, once the restored rows drain → Test `a_restart_resets_progress_without_losing_coverage`
- **Dimension 2.6** (DONE) — the settlement guarantee the existing suite pins is unchanged by the rotation: one settlement, one debit, no re-queue of completed work → Test `queue_loss_replays_without_duplicate_settlement`

### §3 — The bounds stay bounded

The resume state is proven to cost a fixed maximum and to never block the pass. **Implementation default:** compile-time assertions in the block `sweep/reconcile.rs` already uses, because a runtime test over a constant reports a bad edit as a red suite instead of a build that does not link.

- **Dimension 3.1** (DONE) — the resume set holds at most its declared capacity — a bound of its own, not the pass's fleet budget — and a short walk retires its fleet → Test `the_resume_set_respects_its_capacity`
- **Dimension 3.2** (DONE) — the sweeper takes its resume state out of the lock before the pass and puts it back after, so no synchronous guard is held across an await → Test `taking_the_resume_state_leaves_a_usable_one`
- **Dimension 3.3** (DONE) — two reconcilers walking one lost fleet produce one repair and no double settlement → Test `concurrent_reconcilers_repair_each_row_once`
- **Dimension 3.4** (DONE) — an unanswerable probe leaves every receipt intact and advances no repair past unexamined rows → Test `a_reconcile_pass_that_cannot_probe_keeps_the_receipt`
- **Dimension 3.5** (DONE) — a pass over an already-repaired fleet voids nothing and reports quiet, and a pass that fails partway keeps the repairs it drained rather than dropping those fleets back under the head probe → Test `a_failed_pass_keeps_the_repairs_it_drained`

### §4 — The delivery stamp reaches an index

A new forward slot adds an index whose predicate the delivery stamp's own predicate implies. **Implementation default:** `(fleet_id, created_at, seq) WHERE delivered_at IS NULL`, because the stamp keys on exactly those three columns under that one NULL test, and the recovery reads' stricter predicate implies it too; `CREATE INDEX` without `CONCURRENTLY`, because the migration runner wraps every slot in a transaction.

- **Dimension 4.1** (DONE) — the new index's columns and predicate are the ones the stamp's own predicate implies and carry no receipt clause, and the reconciliation index keeps the receipt test it reads on → Test `the_delivery_lookup_index_implies_the_stamps_predicate`
- **Dimension 4.2** (DONE) — the stamp still marks a delivery recorded before its receipt, and one whose physical receipt replay replaced, and a second delivery moves nothing → Test `delivery_stamps_before_and_after_a_replayed_receipt`
- **Dimension 4.4** (DONE) — slot 914 is registered and applies forward, on a fresh database and on one already holding the table → Test `test_migration_list_matches_schema_directory`

### §5 — The attempt counter counts attempts

The outbound worker records a delivery-cycle start when it accepts a job for an undelivered obligation, and the success stamp stops incrementing. **Implementation default:** the cycle-start write also moves `updated_at`, because the recovery scan's age filter exists to re-offer answers nobody is handling and a row a worker has just accepted is being handled; the change is proven in both directions rather than assumed.

- **Dimension 5.1** (DONE) — a cycle that ends in failure increments the count and a second cycle increments it again while the obligation stays undelivered, while one cycle counts once however many vendor retries it made inside → Test `failed_cycles_increase_the_attempt_count`
- **Dimension 5.3** (DONE) — a redelivery of an already-delivered obligation does not move `delivered_at` and does not count → Test `duplicate_delivery_preserves_the_first_stamp`
- **Dimension 5.4** (DONE) — a cycle-start write that fails is logged and the delivery and acknowledgement still happen → Test `bookkeeping_failure_does_not_discard_an_answer`
- **Dimension 5.5** (DONE) — the recorded count appears in the structured events for both the delivered and the exhausted outcome → Test `an_exhausted_cycle_is_counted_and_reported`
- **Dimension 5.6** (DONE) — an obligation a worker is actively retrying is not re-offered by the recovery scan, and one whose worker died is re-offered after the age window → Test `recovery_pacing_follows_the_cycle_start`
- **Dimension 5.7** (DONE) — a shutdown that cuts retries short leaves the entry unacknowledged and does not fabricate a terminal outcome → Test `shutdown_requeue_preserves_the_pending_entry`

### §6 — The documents stop claiming absent behaviour

Three current claims are corrected and the gaps behind them are named rather than quietly dropped. **Implementation default:** each correction says what the daemon does and names the absent feature explicitly, because a page that simply deletes a claim reads as a feature that was never intended rather than one that is not here yet.

- **Dimension 6.1** (DONE) — the generated endpoint description no longer states that repair pull requests, workflow results and deployment status update repair evidence, and names the unported writer instead → Test `test_the_ingress_description_promises_no_unported_writer`
- **Dimension 6.2** (DONE) — no architecture page claims a live execution handle or row-level security; both point at what the daemon does instead → Test `architecture_absent_mechanisms`

### §7 — Two definitions with no caller are removed

A statement constant nothing runs and a key helper whose only consumer is a column nothing reads both go. **Implementation default:** the connector's doc link that names the removed constant is reworded to describe the live ingress read in prose rather than re-pointed across crates, because an intra-crate documentation link to another crate's private detail is a dependency the comment does not otherwise have.

- **Dimension 7.1** (DONE) — the connector crate's dead install-workspace statement is gone and the live ingress read and its caller are untouched → Test `every_statement_is_schema_qualified`
- **Dimension 7.2** (DONE) — creating a fleet no longer writes the unread bundle pointer, and every parameter after the removed bind lands in the right column → Test `an_install_writes_no_bundle_pointer_and_renumbers_nothing_else`. Retrieval is untouched by construction and is not re-graded here: `bundle_content_hash` binds at `$9`, before the removed `$10`, and the object key is derived from it at both preparation and retrieval.

## Interfaces

```
// afd_admission — counts stay Copy; resume state is the caller's.
pub struct Reconciled { pub probed: u64, pub lost: u64, pub voided: u64 }
impl Reconciled { pub const fn is_quiet(&self) -> bool }
pub struct Progress;                        // opaque, Default + Debug
pub async fn reconcile(&self, now: UnixMillis, fleets: i64, rows: i64,
                       progress: &mut Progress) -> Result<Reconciled>;
// afd_outbound — the cycle-start recorder, beside the existing stamp.
// None when the row was already delivered and nothing was counted.
pub(crate) async fn count_attempt(database: &Database, fleet_id: &str,
    event_id: &str, now: UnixMillis) -> Result<Option<i64>>;
-- Unchanged predicates: MARK_DELIVERED, SELECT_UNRECEIPTED, VOID_LOST_RECEIPT,
-- STAMP_DELIVERED's delivered_at guard. SELECT_UNDELIVERED_FLEETS and
-- SELECT_UNDELIVERED_ON_FLEET gain one bound each and keep everything else.
-- schema/914 — additive only.
CREATE INDEX IF NOT EXISTS idx_fleet_admissions_delivery_lookup
    ON core.fleet_admissions (fleet_id, created_at, seq)
    WHERE delivered_at IS NULL;
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Datastore will not answer a probe | Dragonfly unreachable or a command error | The probe answers "still held", the receipt survives, the fleet keeps its place in the resume set, and the existing probe-failure event fires. No row is voided on an unknown. |
| Resume set full | More fleets entered repair in one pass than the capacity allows | The pass declines to add and logs the declined fleet. Its remaining lost rows then wait behind its own head probe until the rows already restored are DELIVERED — the next lease on a consuming fleet, indefinitely on one nothing consumes. Coverage degrades in reachability, not only in speed, and the tests and module docs say so. |
| Process restart mid-repair | Deploy, crash, failover | Resume state is per-process and resets, with the same consequence as a full set: rows behind a head the last repair made healthy wait for that head to be delivered. Documented as the stated limit of in-memory progress. |
| Two reconcilers on one fleet | More than one daemon replica | Both probe, one write lands, the other matches nothing because `VOID_LOST_RECEIPT` pins the receipt. Duplicated round trips, one repair, one settlement. |
| Replay or delivery races the void | The replay sweeper re-appended the row, or a runner leased it, between probe and write | `VOID_LOST_RECEIPT` pins both the probed receipt and `delivered_at IS NULL`, so the void matches nothing and the pass counts the repair it did not make as zero. |
| Migration lock contention | Slot 914 builds an index on a populated table | The build takes `SHARE` on the table and blocks writers for its duration. The rollout note states this; the deployment applies it in a maintenance window. `CONCURRENTLY` is not available inside the transactional runner. |
| Cycle-start write fails | Postgres unavailable at job acceptance | The failure is reported through the existing worker report path and delivery proceeds. The count under-reports starts; the acknowledgement rules do not change. |

## Invariants

1. **A receipt is only forgotten for an entry the datastore was asked about and could not produce.** Enforced by `VOID_LOST_RECEIPT`'s compare-and-set on the probed receipt value plus `delivered_at IS NULL`; a row that moved reports zero rows affected.
2. **Every receipted, undelivered admission is eventually probed, while its fleet holds a place in the resume set.** Enforced by the fleet cursor's wrap on a short read and the resume set's bypass of the head shortcut, under the stated stable-workload bound. A fleet that loses its place — a full set, a restart — regains reachability when its restored rows are delivered; the bound names that condition rather than hiding it.
3. **Exactly one settlement and one debit per logical event survives recovery.** Enforced by `delivered_at` guarding the void and by the lease path's deduplication on logical event identity; proven by the existing settlement test and extended by the boundary reproductions.
4. **No synchronous guard is held across an await in the sweep path, and the resume state is bounded.** Enforced by moving the resume state out of its mutex before the pass and back after, checked by `clippy::await_holding_lock`, and by a compile-time assertion on the cap in the block that already bounds `FLEET_LIMIT` and `ROW_LIMIT`.
5. **`delivered_at` records the first successful acceptance and never moves.** Enforced by `STAMP_DELIVERED`'s `delivered_at IS NULL` guard, which this diff does not touch.
6. **Shipped migration slots are immutable, and recovery never appends.** Enforced by the Schema Table Removal Guard, by this diff adding slot 914 rather than editing 910, and structurally — the pass has no append path; replay owns every re-append.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| `admission_reconcile_repair_declined` | ops | A pass would put a fleet under repair and the bounded set is full | `fleet_id` | no payload, producer key or credential material | `a_full_resume_set_declines_a_repair_and_still_recovers_it` |
| `outbound_delivery_exhausted` (existing, extended) | ops | A delivery cycle ends without the destination taking the answer | existing `provider`, `fleet_id`, `error_code` plus the recorded `attempt_count` | no answer body, no destination credential | `an_exhausted_cycle_is_counted_and_reported` |
| `outbound_obligation_attempt_failed` | ops | The cycle-start write could not be recorded | `provider`, `fleet_id`, `error_code` | no answer body | `bookkeeping_failure_does_not_discard_an_answer` |

No product analytics event changes. No analytics or funnel playbook update is required; the signals here are operator-facing.

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit | `rotation_visits_every_unfinished_fleet` | A resume state driven over a fleet list longer than the cap yields every fleet across consecutive passes, none twice before all are seen once, and a short read leaves the cursor at its start sentinel. |
| 1.3 | integration | `rotation_recovers_fleets_past_the_pass_budget` | Seed one more fleet holding undelivered work than the pass's fleet cap, lose the highest-sorting fleet's stream, run passes with the real cap, and assert its admissions are re-appended within the bounded pass count. |
| 1.4 | integration | `a_deleted_fleet_does_not_strand_the_ones_after_it` | Delete the fleet the cursor was filed on against a live ledger, and the fleets sorting after it still recover. Every fleet stays unfinished after its repair, so a pass that restarted at the lowest-sorting row would never reach the last one. Its unit sibling `a_cursor_survives_a_deleted_fleet` grades the resume state's hand-back only — the comparison it asserts on is the test's own, which is why the live case exists. |
| 2.1 | integration | `a_walk_resumes_past_the_rows_it_already_repaired` | Seed one more admission than the row cap on one fleet, destroy the stream, run reconcile and replay repeatedly at the real caps, and assert every admission names a live entry exactly once. |
| 2.2 | integration | `a_walk_resumes_past_the_rows_it_already_repaired` | After a partial repair restores the oldest rows with live receipts, the next pass still probes the rows beyond the batch rather than reporting the fleet healthy. |
| 2.4 | integration | `interleaved_live_admissions_survive_recovery` | Admissions accepted after the loss, carrying live receipts, are interleaved by creation order with lost ones; recovery voids every lost receipt and no live one. |
| 2.5 | integration | `a_restart_resets_progress_without_losing_coverage` | A fresh resume state over a partially repaired fleet still recovers the remaining rows once the restored rows are stamped delivered. |
| 3.1 | unit | `the_resume_set_respects_its_capacity` | Offering more fleets than the capacity leaves the set at exactly the capacity and reports the declined ones; a short walk removes its fleet from the set. |
| 3.2 | unit | `taking_the_resume_state_leaves_a_usable_one` | The sweep takes its state by value before awaiting; a second caller can read the sweeper's interval while a pass is in flight. |
| 3.3 | integration | `concurrent_reconcilers_repair_each_row_once` | Two passes run against one lost fleet concurrently; voided rows sum to the row count, never more, and the wallet is debited once per logical event. |
| 3.4 | integration | `a_reconcile_pass_that_cannot_probe_keeps_the_receipt` | With the queue unreachable, a pass voids nothing, every receipt survives, and no fleet's resume key advances past unexamined rows. |
| 4.1 | integration | `the_delivery_lookup_index_implies_the_stamps_predicate` | `pg_indexes` shows `(fleet_id, created_at, seq) WHERE (delivered_at IS NULL)` and no receipt clause, and the reconciliation index still carries its receipt test. The PLAN is a recorded measurement, not an assertion — on a small fixture a sequential scan is the correct plan, so an `EXPLAIN` test could only pass by being forced. |
| 4.2 | integration | `delivery_stamps_before_and_after_a_replayed_receipt` | Binding the shipped `MARK_DELIVERED`, a row with a NULL receipt and a row whose receipt replay replaced each report one row affected, then zero on a repeat. Both are rows a `receipt IS NOT NULL` predicate would have stopped marking. |
| 4.4 | unit | `test_migration_list_matches_schema_directory` | The existing registry test fails on a `schema/*.sql` with no `migration!()` entry, and `integration_migrate` applies the whole list to a fresh database. Application to a POPULATED table is recorded evidence, not an assertion: the lane's own `_migrate-test-db` step applied 914 to a 400,600-row table and reported `applied [914], 55 already current`. |
| 5.1 | integration | `failed_cycles_increase_the_attempt_count` | Two cycles that both fail leave the obligation undelivered with a count of two; a cycle whose poster retried several times internally adds one, not several. |
| 5.3 | integration | `duplicate_delivery_preserves_the_first_stamp` | A second delivery of a stamped obligation leaves `delivered_at` and the count unchanged. |
| 5.4 | unit | `bookkeeping_failure_does_not_discard_an_answer` | With the cycle-start write failing, the delivery still happens and the entry is still acknowledged; the failure is reported. |
| 5.5 | unit | `an_exhausted_cycle_is_counted_and_reported` | The delivered and exhausted events carry the count field with the value the write returned. |
| 5.6 | integration | `recovery_pacing_follows_the_cycle_start` | An obligation whose cycle just started is outside the recovery scan's age window; one whose cycle started before the window is inside it. |
| 5.7 | integration | `shutdown_requeue_preserves_the_pending_entry` | A cancelled token during retry leaves the entry unacknowledged and logs the requeue, and the count still reflects the started cycle. |
| 6.1 | unit | `test_the_ingress_description_promises_no_unported_writer` | The generated description contains none of the three repair-evidence promises, and names the gap. Keyed on the operation id, because several routes share the `/v1/ingress/` prefix. |
| 6.2 | unit | `architecture_absent_mechanisms` | No page under `docs/architecture/` mentions row-level security or the session execution handle except to record that neither exists — matching the zero policies `schema/` declares and the zero readers the Rust tree has. |
| 2.6 | integration | `queue_loss_replays_without_duplicate_settlement` | The existing recovery test keeps passing unchanged: one settlement, one debit, no re-queue of completed work. |
| 3.5 | integration | `a_failed_pass_keeps_the_repairs_it_drained` | Running the same pass twice over an already-repaired fleet voids nothing the second time (`replayed_reconcile_pass_is_idempotent`); a pass whose database will not answer returns the error with every drained repair back in the set, and the fleet still recovers afterwards. |
| 7.1 | unit | `every_statement_is_schema_qualified` | The remaining connector statements are schema-qualified and each names a caller; the removed spelling appears nowhere in the crate. |
| 7.2 | integration | `an_install_writes_no_bundle_pointer_and_renumbers_nothing_else` | A fleet created through the production install path has a NULL bundle pointer and correct values in every column after the removed bind, and its bundle retrieves correctly. |

**Stable-workload bound for invariant 2:** with fleets holding undelivered work bounded by `F`, the largest single-fleet loss bounded by `R`, and fewer than `REPAIR_CAPACITY` fleets mid-repair, every lost receipt is probed within `ceil(F / FLEET_LIMIT) + ceil(R / ROW_LIMIT)` passes, absent a restart. Past that capacity, or across a restart, a declined fleet's later rows are reached only after its restored rows are delivered. Tests assert both halves; neither is stated as unqualified eventual progress.

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Lost work beyond the fleet cap and beyond the row cap is recovered (§1, §2) | `make test-integration-rustd` | exit 0; `rotation_recovers_fleets_past_the_pass_budget`, `a_walk_resumes_past_the_rows_it_already_repaired` and `a_deleted_fleet_does_not_strand_the_ones_after_it` pass | P0 |  ✅ exit 0 — all three pass. Neutralising the cursor bound fails `a_deleted_fleet_does_not_strand_the_ones_after_it` and `rotation_recovers_fleets_past_the_pass_budget`, so both grade the bound rather than restating it |
| R2 | The delivery stamp reaches the new index (§4) | `make test-integration-rustd` | exit 0; `the_delivery_lookup_index_implies_the_stamps_predicate` pins the index definition. The PLAN is a recorded measurement, not an assertion — see the Test Specification row | P0 |  ✅ exit 0 — `the_delivery_lookup_index_implies_the_stamps_predicate` passes; recorded plan `Index Cond: (fleet_id = … AND created_at = … AND seq = …)`, 72 buffers → 2 on 400,600 rows |
| R3 | Failed delivery cycles move the counter (§5) | `make test-integration-rustd` | exit 0; seven `integration_attempt_count` cases pass here, and the datastore-free `bookkeeping_failure_does_not_discard_an_answer` rides the unit lane | P0 |  ✅ exit 0 — seven here, the eighth green in `make test-unit-all` |
| R4 | No document claims row-level security or a live execution handle (§6) | `grep -rn "Row-Level Security\|execution_id. is set at" docs/architecture/data_flow.md` | 0 matches | P0 |  ✅ 0 matches |
| R5 | Nothing is dropped from the schema | `git diff origin/main...HEAD -- schema/ \| grep -E '^\+.*(DROP\|ALTER)'` | no output | P0 |  ✅ no output — slot 914 is `CREATE INDEX IF NOT EXISTS` only |
| R6 | Diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 paths missing from the Files Changed table | P0 |  ✅ 41 files changed, 0 missing from the table |
| S1 | Conform gates green | `make harness-verify` | exit 0 | P0 |  ✅ exit 0 — ALL GATES GREEN |
| S2 | Unit tests pass | `make test-unit-all` | exit 0 | P0 |  ✅ exit 0 — 2,628 Rust unit + 1,673 TypeScript passed (+18 on the baseline's 2,610); all package coverage gates passed |
| S3a | Lint green | `make lint-all` | exit 0 | P0 |  ✅ exit 0 — rustfmt, clippy -D warnings, no-feature build |
| S3b | Integration suite green | `make test-integration-rustd` | exit 0 | P0 |  ✅ exit 0 — 511 passed + 1 exclusive = 512 (+19 on the baseline's 493) |
| S3c | Version sync | `make check-version` | exit 0 | P0 |  ✅ exit 0 — all versions match 0.49.0, bumped from 0.48.0 on Indy's instruction |
| S4 | No secrets | `gitleaks detect` | exit 0 | P0 |  ✅ no leaks found |
| S5 | No oversize source file | `git diff --name-only origin/main...HEAD \| grep -v '\.md$' \| xargs wc -l 2>/dev/null \| awk '$1>350 && $2!="total"'` | no output | P0 |  ✅ no output |
| S6 | Orphan sweep | Dead Code Sweep greps | 0 matches | P0 |  ✅ 0 matches — both removed symbols gone |

**Command source rule:** every declared `conform` and `verify.*` invocation from `.oracle/orly.json` appears above verbatim with an Expected value. Baseline metadata is pending at opening and measured before the Pull Request; command timing is `dispatch/lifecycle.md`.

**Grading protocol (VERIFY):** run each spec-specific Verify command verbatim; Graded = ✅/❌ plus one decisive output line. Repository-command rows point at the final `orly gate pr` results in Pull Request Session Notes. **Ship gate:** any ❌ or missing evidence returns to EXECUTE. A P1 ❌ requires an Indy-acked deferral quote in Discovery. A P0 whose scope moves whole into a named successor spec — carried there as that spec's own P0, mapped in both, with the owner's verbatim quote in Discovery — is marked `MOVED to M{N}_{NNN} R{n}` rather than ❌, and is never rendered ✅.

## Dead Code Sweep

**1. Orphaned files.** N/A — none deleted; two symbols go from files that keep their other contents. **2. Orphaned references — zero remaining imports/uses.** Each grep is crate-scoped on purpose: `afd_ingress::sql::SELECT_INSTALL_WORKSPACE` is the live read, `afd_library`'s and `afd_fleet`'s `snapshot_key` are the live bundle layout, and `bundle_snapshot_key` stays declared in the frozen schema slot.

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| `afd_connector::sql::SELECT_INSTALL_WORKSPACE` | `git grep -rn -w 'SELECT_INSTALL_WORKSPACE' -- rustd/crates/afd_connector` | 0 matches |
| `snapshot_key` (the `afd_fleet_lifecycle` private helper) | `git grep -rn -w 'snapshot_key' -- rustd/crates/afd_fleet_lifecycle` | 0 matches |
| `bundle_snapshot_key` as a written column | `git grep -rn 'bundle_snapshot_key' -- rustd cli ui` | 0 matches |

## Out of Scope

- **Restoring the Slack event producer or the repair-evidence writers (F01, F02).** No runtime consumer exists and adding inserts alone would bypass evidence validation, workspace ownership and dispatcher claim fencing. Reported as an open gap, together with deleting `core.connector_channels` and the repair tables; a future spec owns the feature.
- **Dropping `core.fleet_sessions.execution_id`, `execution_started_at` or `core.fleets.bundle_snapshot_key` (F06, F07 column removal).** Requires a deployed-binary and rollback-consumer inventory, stored-data inspection and dependency review first; this spec stops the write and corrects the prose, and the columns stay. **Removing `idx_fleet_admissions_undelivered`** even if slot 914's index subsumes it is the same kind of decision: measured and reported here, removed under its own rollout.
- **The design investigations D01 through D06 and D08.** Foreign-key index additions, cross-table scope constraints, ordered-reader index suffixes, plan-caching changes, retention or partitioning, overlapping-index removal, and the `resumes_event_id` index. Each needs measured plans, row distributions or a stated retention horizon that this spec does not gather. Reported with their evidence requirements, not fixed.
- **Any production write, deployment, merge or force-push.**

---

## Product Clarity (authoring record)

1. **Successful user moment** — a provider's webhook is accepted during an incident that wipes the queue, and the run happens. Nobody files a ticket asking why one delivery out of a thousand silently did nothing, because the pass that repairs the other nine hundred and ninety-nine repairs that one too.
2. **Preserved user behaviour** — a producer's retry is still one run, a completed run is still never re-queued, a delivery is still stamped once, and the wallet is still debited once. Breaking any of those is a redesign, not this spec.
3. **Optimal-way check** — the most direct shape would probe the datastore for each fleet's oldest surviving entry once and compare every undelivered receipt against it, detecting all loss on a fleet in one round trip. It is rejected here because it requires ordering stream identifiers numerically in SQL, which the codebase deliberately avoids; the gap is round trips during recovery, which is not the path anybody waits on.
4. **Rebuild-vs-iterate** — iterate. The pass's structure is sound and its invariants are documented; what is missing is progress across its own batch boundaries. A rebuild would put the receipt compare-and-set, the probe-failure rule and the replay ownership boundary back at risk for no gain.
5. **What we build** — a rotating cursor, a bounded repair set, one index slot, one counter relocation, three document corrections.
6. **What we do NOT build** — the Slack producer, the repair writers, any column drop, any speculative index, a public endpoint for the attempt count, a durable cursor table.
7. **Fit with existing features** — this compounds with the replay sweeper, which owns every re-append and gets the work this pass releases. The one thing it must not destabilise is the admission budget: voided rows re-enter the replay backlog, and a pass that voided more per interval than the sweeper drains would refuse new producers.
8. **Surface order** — N/A — no user surface. The only externally visible change is a generated endpoint description that becomes accurate.
9. **Dashboard restraint** — the attempt count rides existing structured events and gets no panel until an operator has asked a question it answers.
10. **Confused-user next step** — an operator who sees repeated `admission_stream_data_lost` lines for one fleet now also sees whether repair is progressing, because the declined-repair event names a fleet that recovery is deferring.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** six Sections in one workstream because they share one review context — they all came from one audit of one schema, and four of the six are single-symbol changes that would cost more in Pull Request overhead than in implementation. The two substantial Sections, recovery rotation and the counter, touch disjoint crates and can be verified independently.
- **Alternatives considered:** splitting recovery into its own workstream ahead of the rest. Rejected because the Pull Request budget is one per milestone and the remaining five items are small enough that a second Pull Request would spend more of that budget than it saves in review load. Also considered: the numeric receipt-floor redesign of Product Clarity item 3, rejected because `rustd/crates/afd_dragonfly/src/streams/consume.rs` records that the probe asks the server precisely so nothing has to beat the text ordering under which `999-0` sorts after `1000-0`; a cursor leaves that decision intact.
- **Patch-vs-refactor verdict:** this is a **patch** because every failure is a missing bound or a misplaced write inside a structure whose invariants already hold. The one place a refactor would be right — replacing the per-fleet head heuristic with a floor comparison — is named in Product Clarity and left to a future spec if recovery round trips ever become the constraint.

## Discovery (consult log)

- **Consults** — Adversarial review, `docs/v2/reviews/schema-fix-adversarial-review-2026-09-18.md`, inspected commit `333c0060259f248e5ac2a311e0fb179132c2f5e5`: its dispositions supersede the original audit's suggested remedies and are followed here. Every claim it cites was re-verified against the current checkout before this spec was written; none had drifted. Architecture consult: `docs/architecture/data_flow.md` §The durable ledgers is the page this pass's iteration rule belongs to and it is corrected in the same diff. Gate-flag triage: none fired at authoring.
- **Metrics review** — one new operator event (`admission_reconcile_repair_declined`), one new failure event (`outbound_obligation_attempt_failed`), one existing event extended with the recorded count. No analytics or funnel playbook update required: every signal here is operator-facing and none is a product funnel step.
- **Skill-chain outcomes** — `/orly-write-unit-test`, gstack `/review` and `orly-babysit-prs` pending; results recorded at VERIFY, REVIEW and after the first push.
- **Deferrals** — none at authoring. The Out of Scope items are review-approved exclusions carried from the adversarial review's own disposition table, not agent-unilateral deferrals; each is reported as open with its evidence requirement rather than claimed done.
