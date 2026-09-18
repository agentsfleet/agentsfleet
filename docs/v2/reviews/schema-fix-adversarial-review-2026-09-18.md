# Adversarial review of schema remedies

Review date: Sep 18, 2026. Inspected commit: `333c0060259f248e5ac2a311e0fb179132c2f5e5`, plus the untracked original audit.

This review challenges the remedies in [the original audit](schema-usage-audit-2026-09-18.md). Its dispositions supersede that report's suggested implementation scope.
Approval here means technical approval for the bounded work described below. It does not authorize deployment, production database writes, or destructive removal.

The intervening commit changes five Rust files concerning wire validation and runner lease handling. `git diff c6b54a1a5365b28d3d35ebc3d0384b6920b3f469..HEAD --stat` showed no schema changes.

## Final disposition

| Original item | Decision | Approved work and reason |
|---|---|---|
| F01: unused channel and repair-run tables | HOLD feature implementation and removal | No runtime consumer exists, but these tables describe intended features. Resolve product scope before restoring or deleting them. Approve documenting the actual gap under D07. |
| F02: missing repair producers | HOLD feature implementation and removal | Adding inserts alone would bypass evidence validation and correlation rules. Restore the complete feature only after agreeing its behavior and security boundary. Approve documenting the actual gap under D07. |
| F03: unused connector query | APPROVE | Remove only `afd_connector::sql::SELECT_INSTALL_WORKSPACE` and its obsolete references. Preserve the live ingress query and installation behavior. |
| F04: delivery index mismatch | APPROVE with plan evidence | Preserve delivery semantics and supply a usable index through a new migration. Choose its definition after comparing the exact update and recovery reads. |
| F05: recovery starvation | APPROVE, highest priority | Repair fleet rotation and progress within a fleet. A cursor across fleets alone leaves a second reproducible failure. |
| F06: unused session execution fields | APPROVE documentation; HOLD column removal | Current lease code explicitly replaces the execution handle with fenced leases. Removing stored columns still needs deployed-consumer and data checks. |
| F07: write-only bundle pointer | APPROVE stopping the write; HOLD column removal | The stored path also disagrees with the live bundle layout. Remove the redundant insert argument and private helper after checking callers. Keep the nullable column through the compatibility window. |
| F08: misleading outbound attempt count | APPROVE bounded metric repair | Count delivery cycles consistently, including failure, and emit the count through existing telemetry. Preserve successful-delivery and acknowledgement semantics. |
| D01: child foreign-key indexes | INVESTIGATE; no blanket additions | An absent general index can matter for parent deletion, but row count, cascades and existing prefixes determine whether another index pays. |
| D02: copied scope consistency | INVESTIGATE; no blanket constraints | Separate foreign keys allow mismatched scopes. Enumerate intended equalities, existing violations and deletion semantics before choosing constraints. |
| D03: ordered-reader indexes | INVESTIGATE; no blanket additions | Missing ordering suffixes do not establish harmful query plans, especially for bounded collections. |
| D04: generic plans and partial indexes | INVESTIGATE | Compare actual prepared custom and generic plans. Do not globally change plan caching or inline application vocabulary to force one plan. |
| D05: retention and growing recovery scans | INVESTIGATE; no deletion | Terminal admissions still carry producer deduplication identity. Define the retry horizon before retention, partitioning or cleanup. |
| D06: overlapping indexes | INVESTIGATE; no removal | Constraint enforcement and foreign-key actions count as consumers. Source search cannot establish physical redundancy or operational cost. |
| D07: inaccurate documentation | APPROVE | Correct current architecture and generated API claims to match the daemon. Preserve historical records and identify missing features explicitly. |
| D08: continuation index's absent reader | INVESTIGATE; no removal | The column is live in event writes and projections. The index's declared lookup is absent; that alone does not justify deleting the column or index. |

Primary key (PK), foreign key (FK), and pull request (PR) abbreviations are used below.

## Adversarial checks completed

### F03: distinguish a dead definition from a live table

Searched the symbol across Rust, scripts, command-line code and the application. Checked its definition, unit assertions and documentation reference.
The connector definition is at `rustd/crates/afd_connector/src/sql.rs:69`. The live ingress definition is at `rustd/crates/afd_ingress/src/sql.rs:49`.
Its production caller is `rustd/crates/afd_ingress/src/app.rs:101`.

Rejected remedy: deleting `connector_installs`, consolidating unrelated crate ownership, or preserving dead SQL through a new artificial caller.
Before removal, Claude must recheck exports and whether this public Rust symbol has consumers outside this workspace.

### F04: challenge the tempting predicate fix

Compared `MARK_DELIVERED` at `rustd/crates/afd_admission/src/sql.rs:155` with `schema/910_fleet_admissions.sql:114`.
The update permits a NULL receipt; the partial index excludes it. Adding `receipt IS NOT NULL` changes which deliveries get stamped.
The update addresses logical event identity, which remains stable when replay changes the physical stream receipt.

Candidate remedy: an index on `(fleet_id, created_at, seq)` with predicate `delivered_at IS NULL`.
This is a candidate, not a selected plan. Compare it with a separate lookup index and include the unreceipted backlog in the workload.
Do not widen the reconciliation query's predicate simply because an index becomes broader.

Checked migration execution: `rustd/crates/afd_db/src/migrate.rs:243` starts a transaction for each slot; line 248 runs the slot inside it.
`CREATE INDEX CONCURRENTLY` cannot run inside that transaction. A regular index build can block writers.
Claude must choose a supported migration and document its lock duration and rollout implications.
PostgreSQL sources: [partial-index predicates](https://www.postgresql.org/docs/current/indexes-partial.html) and [index creation](https://www.postgresql.org/docs/current/sql-createindex.html).

Rejected remedies: predicate tightening, receipt-keyed delivery stamping, an unregistered manual index, or a concurrent build inside the existing transactional runner.

### F05: attack both batch boundaries

Traced `SELECT_UNDELIVERED_FLEETS`, `SELECT_UNDELIVERED_ON_FLEET`, `reconcile`, `void_lost_on`, and replay receipt recording.
Sources: `rustd/crates/afd_admission/src/sql.rs:182`, `:209`, `:94`; `rustd/crates/afd_admission/src/reconcile.rs:119`.
The driver uses 128 fleets and 32 rows at `rustd/crates/afd_runner/src/sweep/reconcile.rs:46` and `:52`.

Two counterexamples survive source review:

1. Keep 128 lower-sorting fleets unfinished. Put lost work in fleet 129. Every pass selects the same first 128.
2. Lose 33 admissions on one fleet. Reconcile voids the first 32 receipts; replay restores those rows. The oldest receipt now exists. The shortcut skips the fleet, leaving row 33 missing until another event changes the head.

I ran an abstract Python state model of these branches. Output:

```text
Fleet model: 10 passes; 128/129 visited; lost fleet 129 never visited.
Row model: 33 lost; first pass voids 32; replay restores first 32; next 10 passes skip final lost row.
```

This model establishes the branch counterexample; it is not a repository test or a live datastore reproduction.
The existing real recovery test seeds three admissions and settles one before loss, so it never crosses the 32-row boundary.
See `rustd/crates/afd_fleet/tests/integration_admission_recovery.rs:278` and its reconcile/replay calls at `:338`.

Checked protections the fix must preserve:

- A failed datastore probe retains the receipt: `reconcile.rs:174` and `tests/integration_recovery_outage.rs:147`.
- Voiding compares the probed receipt and requires no delivery stamp: `afd_admission/src/sql.rs`, `VOID_LOST_RECEIPT`.
- Replay owns append and receipt recording; reconciliation only voids receipts.
- The lease path deduplicates logical event identity; recovery must not charge or settle twice.
- Existing reclaim cursor behavior is prior art, not a drop-in implementation: `afd_runner/src/sweep/reclaim.rs:21`.

Approved outcome: bounded, resumable coverage of every eligible fleet and row, even while earlier rows remain queued.
Choose a bounded cursor design that survives replay changing receipts. Define wrap, reset, error and restart behavior.
Do not retain unbounded per-fleet cursor state or hold a synchronous mutex across an await.
An in-memory cursor can be sufficient for steady-state progress, but repeated restarts reset it; document that limit or implement a justified durable alternative.

Rejected remedies: increasing limits, deleting limits, random ordering without a progress guarantee, or adding only a fleet cursor.

### F06 and F07: challenge destructive cleanup and rollout

The session read explicitly says execution handles are replaced by fenced leases: `rustd/crates/afd_fleet/src/lease/sql/fleet.rs:21`.
Current SQL omits both execution columns. This supports removing the stale documentation; it does not establish that deployed older binaries omit them.

The bundle pointer has stronger evidence than the first audit recorded:

| Path | Source |
|---|---|
| Stored unused pointer: `fleet-bundles/{hash}.tar.zst` | `rustd/crates/afd_fleet_lifecycle/src/install/row.rs:217` |
| Prepared bundle key: `fleet-bundles/sha256/{hash}.tar` | `rustd/crates/afd_library/src/prepare.rs:9` and `:27` |
| Live retrieval layout: the same `sha256/` prefix and `.tar` suffix | `rustd/crates/afd_fleet/src/bundle/mod.rs:55` and `:58` |

`bundle_snapshot_key` is nullable in `schema/500_fleets.sql:54`. Its sole current SQL writer is `afd_fleet_lifecycle/src/sql.rs:126`.
The caller binds the derived pointer at `install/row.rs:173`.

Approve removing that insert column, argument, helper and helper-only assertions. Keep `bundle_content_hash` and the live bundle layout.
Check timestamp parameter numbering after removing the bind. Prove a newly installed bundle remains retrievable through the public path.
Do not rewrite stored pointers, migrate objects, or introduce a new reader to justify the column.

A column-drop migration applied before older installers retire would break their inserts even if those installers never read the column.
Before any later removal, inspect deployed binaries, rollback targets, external queries, stored values, views and trigger dependencies.
The present prompt excludes column drops. The table cleanup findings remain open until separately resolved.

### F08: challenge counter meaning, retries and telemetry

Read the counter update, its caller, the retry loop and the recovery age filter:

- `rustd/crates/afd_outbound/src/obligation/sql.rs:22`: counter increments only with the first successful stamp.
- `rustd/crates/afd_outbound/src/lanes.rs:209`: only `Verdict::Delivered` enters that branch.
- `rustd/crates/afd_outbound/src/poster.rs:110`: one worker delivery cycle can contain several poster attempts.
- `rustd/crates/afd_outbound/src/obligation/sql.rs:69`: recovery eligibility depends on `updated_at`.
- `rustd/crates/afd_outbound/tests/integration_obligations.rs:363`: the current test asserts one acceptance is one attempt, reproducing the ambiguity.

Approve defining `attempt_count` as recorded worker delivery-cycle starts for an undelivered obligation.
A cycle starts when the worker accepts the job for `deliver_with_retry`; internal vendor retries do not each increment it.
This bounded definition exposes repeated exhausted cycles without adding a database write for every vendor retry.
It is not an exact vendor request counter or proof of delivery.

Record starts independently of the success stamp. A later retry cycle increments again while the obligation remains undelivered.
Preserve `delivered_at` as the first successful acceptance timestamp. Repeated stamping alone must not increase the counter.
Expose the resulting count in existing structured telemetry, including failure outcomes. Do not add a public endpoint solely for this field.

Before editing, write a small state table covering start, retries, exhaustion, permanent refusal, cancellation, database failure and successful stamping.
Choose how count updates interact with `updated_at` and recovery pacing; preserving or changing that behavior must be deliberate and tested.
If telemetry persistence fails, preserve delivery and acknowledgement semantics and log the failure; disclose that the count can under-report starts.
Already-delivered rows should not be mutated merely to count duplicate queue handling. Do not quietly introduce a new deduplication protocol here.
Existing counts have the old meaning: do not fabricate historical failure counts or reset them. Document the rollout boundary and interpretation.

Rejected remedies: incrementing at acknowledgement, counting success as an attempt, claiming exact vendor counts, or letting bookkeeping failure discard an owed answer.

### F01/F02 and D07: distinguish missing features from removable objects

`afd_api_ingress/src/handler/events.rs:88` names the missing Slack producer.
`afd_api_ingress/src/handler/webhook/app_route.rs:24` explicitly says repair writers are unported.
The same route's generated API description at `:110` claims those writers exist. That contradiction should be fixed now.

Repair evidence is not an arbitrary insert surface. `schema/831_repair_run_results.sql` requires event-bound immutable evidence.
The dispatcher correlates links, events, results and merge identity: `afd_runner/src/sql/sweep.rs:274`.
Before a future restoration, require signed payload validation, workspace ownership, approved repair-branch provenance, exact deployed commit matching,
out-of-order arrival handling, duplicate delivery handling, ambiguity refusal and dispatcher claim fencing.
Slack restoration additionally needs safe concurrent first-mention creation, installation movement/disconnect behavior and outbound destination scoping.

D07 also applies to the session-handle claims at `docs/architecture/data_flow.md:316` and database-policy claims at `:1079`.
The current workspace query explicitly says no database policy exists: `rustd/crates/afd_tenant/src/sql/workspace.rs:20`.
Correct those descriptions without declaring the application's authorization broken or introducing database policies as a documentation fix.

### D01-D06 and D08: resist speculative schema changes

Rechecked FK prefixes, composite scope constraints, active reader predicates and index declarations against the original inventory.
Physical index use, production row distributions and deployed drift remain unmeasured.
The old conventions page also says FKs reference only PKs, while current scope constraints reference composite UNIQUE keys.
See `docs/SCHEMA_CONVENTIONS.md` and `schema/500_fleets.sql:62`, `schema/610_runner_leases.sql:77`.
Do not remove those scope constraints or redesign identities to make that stale blanket statement true.

Required follow-up evidence: representative plans, parent deletion tests, index dependencies, retention/retry requirements and stored-scope mismatch queries.
For D08 specifically, preserve `resumes_event_id`; event writes and history projections consume the column even without the declared index lookup.

## Checks not performed

No production access, database queries, query-plan measurements, deployed-binary inventory or repository test gates ran.
No Claude process was invoked. This file provides the requested handoff prompt rather than claiming an independent Claude review.
Only review documents were written.

Rust review guidance read and applied: `M-TAUTOLOGICAL-TESTS`, `M-HOTPATH`, `M-YIELD-POINTS`, `M-INTEGRATION-TESTS`, `M-LOG-NOT-PRINT`, `M-TEST-UTIL`.
Source: `/Users/kishore/Projects/oss/rust-guidelines/all.txt`, the local reference required by `dispatch/write_rust.md`.
Repository rules require real Postgres and Dragonfly integration tests for datastore behavior, and generated Rust error shells for fallible interfaces.

## Prompt for Claude

```text
Work in /Users/kishore/Projects/agentsfleet. Implement the technically approved
schema-audit remedies below, with regression evidence. Do not treat the original
audit's suggestions as permission for every possible schema change.

Read:
- AGENTS.md and AGENTS.orly.md.
- docs/v2/reviews/schema-usage-audit-2026-09-18.md.
- docs/v2/reviews/schema-fix-adversarial-review-2026-09-18.md (this decision wins).
- Applicable dispatch rules, docs/SCHEMA_CONVENTIONS.md,
  docs/RUST_ERROR_STANDARD.md and the relevant architecture pages.

Preflight before edits:
1. Record HEAD, branch, VERSION and working-tree changes. Preserve others' work.
   The adversarial review inspected 333c0060259f248e5ac2a311e0fb179132c2f5e5.
   Recheck the cited predicates and callers against your current checkout.
2. Identify current migration slots and the runner's transaction behavior.
   Shipped SQL files stay frozen; schema changes use new registered slots.
3. Read existing recovery/outbound fixtures and test-isolation rules. Trace
   production callers, not only test helpers or public SQL constants.
4. Record a short implementation plan with the evidence and invariants below.
   Resolve routine implementation choices yourself. Continue independent work
   if a deployment fact is unavailable; do not assume missing facts are true.

Approved work, in priority order:

A. F05: fix admission recovery progress across fleets AND within a fleet.
   Prove the current failures before the fix:
   - 129 unfinished fleets, with lost work beyond the first 128.
   - 33 lost admissions on one fleet; reconcile 32, replay them, then ensure
     subsequent passes still recover the final missing admission.
   Design bounded resumable iteration, explicit wrap/error/restart behavior,
   and bounded memory. A fleet cursor alone is insufficient. The oldest-entry
   shortcut must not conceal missing later receipts after partial recovery.
   Preserve receipt compare-and-set, delivered guards, replay ownership of
   appends, logical event identity and exactly one settlement/debit.
   Failed probes retain receipts. Avoid holding database locks/connections
   across reconciliation probes or synchronous mutexes across awaits.
   Do not solve this by increasing limits or scanning everything each pass.

B. F04: repair the admission delivery lookup through a measured index change.
   Preserve MARK_DELIVERED's ability to stamp before receipt recording and
   after replay changes the physical receipt. Evaluate the candidate index
   (fleet_id, created_at, seq) WHERE delivered_at IS NULL against alternatives.
   Compare the exact prepared UPDATE and recovery reads on representative
   healthy, historical and unreceipted-backlog datasets. Record plans/buffers
   and write cost; do not force a tiny-table test to choose an index.
   Keep reconciliation's receipt predicate intact. Use a new migration;
   CREATE INDEX CONCURRENTLY does not fit the current transactional runner.
   Document lock/rollout implications. Do not redesign migration execution
   just to avoid presenting an unresolved deployment constraint.

C. F03: remove only the unused afd_connector SELECT_INSTALL_WORKSPACE SQL
   definition and its obsolete assertions/comments. Preserve the live
   afd_ingress definition and its callers. Recheck public/exported consumers.

D. F07: stop writing the unused bundle_snapshot_key during fleet creation.
   Remove the insert column/bind and private snapshot_key helper if its only
   purpose disappears. Check parameter renumbering and all callers.
   The dead helper stores fleet-bundles/{hash}.tar.zst, whereas live preparation
   and retrieval use fleet-bundles/sha256/{hash}.tar. Keep the live hash/layout.
   Prove installation followed by retrieval still returns the correct bundle.
   Leave the nullable database column and historical values in place. No DROP,
   object rename, data rewrite, or artificial new reader belongs in this patch.

E. F08: repair attempt_count using the review's explicit meaning: recorded
   worker delivery-cycle starts while an obligation remains undelivered.
   One deliver_with_retry cycle counts once, including eventual failure;
   internal vendor retries are not separate cycles. Record starts independently
   of STAMP_DELIVERED; keep the first successful delivered_at immutable.
   Read the resulting count into existing structured telemetry, including
   failure outcomes. No new public endpoint is required.
   Before coding, write the state table for start, retry, exhaustion, permanent
   refusal, shutdown, database failure and successful stamping. Decide and test
   how updated_at changes affect the recovery age filter. Bookkeeping failure
   must not discard an owed answer or silently change acknowledgement rules.
   Document best-effort counting, crash windows and old rows' different meaning.
   Do not backfill invented attempt counts or reset historical data.

F. D07/F06: correct current documentation and generated API prose concerning
   missing Slack/repair producers, session execution handles and absent database
   row policies. Preserve historical specs and frozen migration files.
   Public behavior documentation requires the matching ~/Projects/docs branch;
   follow its instructions in that repository, never through this worktree.
   Do not implement a new feature or authorization mechanism to match stale prose.

Excluded from implementation:
- F01/F02 feature restoration or table deletion. Report missing writers as open.
- F06/F07 column drops. Require deployed/rollback consumer inventory, stored-data
  and dependency checks, then an explicit removal decision before a later migration.
- D01-D06/D08 blanket index additions/removals, scope constraints, retention,
  partitioning or row-policy changes. Investigate with evidence, report decisions;
  do not claim these investigations are fixed by the approved patch.
- No production writes, deployment, merge or force-push is authorized by this prompt.

Required adversarial regressions:
- F05's two batch-boundary failures, healthy-head/missing-tail mixtures, ties in
  created_at, cursor wrap, cursor-row deletion, concurrent insertion, process
  restart, concurrent reconcilers, probe outage, receipt replacement and delivery
  racing void. Demonstrate eventual progress under a stated stable-workload bound.
- F04: delivery before receipt write, replayed physical receipt, duplicate delivery,
  long-lived fleet history, and new migration on both empty and existing schemas.
- F07: installation and bundle retrieval through real production paths after bind
  removal; old schema compatibility. No object-key change is expected.
- F08: failures increase cycles; internal retries obey the chosen count; duplicate
  stamping preserves delivered_at; cancellation/acknowledgement semantics survive;
  database bookkeeping failure does not lose an answer; recovery pacing remains
  correct; telemetry actually reads the recorded count.

Use existing real-schema fixtures and isolated test-owned stream keys. Never flush
the shared cluster or replace production tables with temporary lookalikes. Preserve
the recovery suite's existing shared lane coordination. Datastore tests remain
ignored in the unit lane and run through make test-integration-rustd.

Use the repository's Make targets and orly gate workflow. Package-only runs are
inner-loop evidence. Required repository claims use make harness-verify,
make lint-all, make test-unit-all, make test-integration-rustd and make check-version.
For a fresh worktree, install root, CLI and each UI package dependencies as AGENTS.md
requires. Read the Rust error standard before changing fallible interfaces.

Deliver the changes, the regression results, before/after plan evidence, migration
rollout notes, and a disposition for every F01-F08/D01-D08 item. Separate fixed,
partially addressed, evidence-dependent and excluded items. Name checks you could
not run. Do not mark all schema findings PASS merely because the approved subset
passes. Prepare the result for review; do not deploy or merge it.
```
