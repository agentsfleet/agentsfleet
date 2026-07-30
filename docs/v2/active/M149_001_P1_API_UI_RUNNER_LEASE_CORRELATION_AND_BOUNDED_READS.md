<!--
SPEC AUTHORING RULES (load-bearing — the one comment that survives):
- Body order = the executing agent's read order. Fill via the kishore-spec-new
  skill (authoring order lives there); after filling, DELETE every "tpl:"
  guidance comment — the SPEC TEMPLATE GATE blocks tpl residue, unfilled
  {slots}, and missing required sections (audits/spec-template.sh --staged).
- No time/effort/hour/day estimates anywhere. No effort columns, complexity
  ratings, percentage-complete, implementation dates, assigned owners.
- Priority (P0/P1/P2/P3) is the only sizing signal; Dependencies are the only
  sequencing signal. A section that contradicts these rules loses — delete it.
-->

# M149_001: Runner lease workspace correlation and bounded read paths

**Prototype:** v2.0.0
**Milestone:** M149
**Workstream:** 001
**Date:** Jul 30, 2026
**Status:** IN_PROGRESS
**Priority:** P1 — operator-facing correlation gap plus read paths that grow without bound
**Categories:** API, UI
**Batch:** B1 — single workstream, no parallel siblings
**Branch:** feat/m149-runner-lease-reads
**Test Baseline:** unit=3266 integration=501
**Depends on:** none (M148_001 pending touches runner policy, not these read paths)
**Provenance:** LLM-drafted (Claude Fable 5, Jul 30, 2026) — grounded in a live investigation of runner `ant` on dev; every claim below was verified against code or the live environment that day
**Canonical architecture:** `docs/architecture/runner_fleet.md` § runner lease surface

---

## Overview

**Goal (testable):** A platform admin filters a runner's lease list to one workspace and sees only that workspace's rows with a visible Workspace column; the runner detail, lease page, and activity page reads are index-served with no whole-history aggregation at 100k leases; deleting a Clerk user leaves zero live fleets, schedule rows, or Upstash QStash registrations.
**Problem:** The admin runner lease table shows every workspace's leases with no ownership column, so an operator cannot tell their own fleet's work from End-to-End (E2E) fixture noise. Lifetime counters re-aggregate the runner's entire lease history on every page load, the pager total is an unindexed count, and the activity feed post-filters its dominant event types. Nothing prunes `fleet.runner_leases`/`fleet.runner_events`. A deleted fixture user left a live fleet whose cron still fires — the account purge path missed it and its Upstash QStash schedule was never unregistered.
**Solution summary:** Surface `workspace_id` (already on the wire) as a column and filter; maintain runner lifetime counters incrementally at lease write time (mirroring `core.fleet_activity_counters`) so the detail read stops aggregating; index the activity feed's filtered reads; split the chat failure copy so a runner refusal stops reading as "needs instructions"; make the E2E suites tear their fleets down and let global teardown sweep leaks; add a retention sweeper for terminal lease/event rows; make account teardown unregister Upstash QStash schedules and emit an observable failure signal when purge work fails.

## PR Intent & comprehension handshake

- **PR title (eventual):** feat(api,app): correlate runner leases by workspace, bound the read paths
- **Intent (one sentence):** Platform admins can attribute any runner lease to its workspace at a glance, and every runner read stays fast no matter how long the runner has lived.
- **Handshake** — the implementing agent fills this at PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `src/agentsfleetd/http/handlers/fleet/runner_leases.zig` + `src/agentsfleetd/http/handlers/fleet/sql.zig` — the current lease read path this spec re-shapes; the SQL file's own comment names the windowed-counter follow-up this spec delivers.
2. `schema/030_fleet_activity_counters.sql` + `src/agentsfleetd/state/fleet_events_store.zig` — the in-repo prior art for write-time incremental counters; mirror its shape.
3. `schema/041_runner_leases_operator_read_indexes.sql` + `src/agentsfleetd/db/index_usage_integration_test.zig` — the migration + plan-proof pattern (`expectServesFilter`) every new index here must follow.
4. `src/agentsfleetd/fleet/liveness_sweeper.zig` + `src/agentsfleetd/cmd/serve_background.zig` — the background sweeper pattern and registration point for the retention sweeper.
5. `src/agentsfleetd/state/account_teardown.zig` + the cron sync used by `src/agentsfleetd/http/handlers/fleets/create.zig` — the purge path that today deletes schedule rows by cascade without unregistering Upstash QStash.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `src/agentsfleetd/http/handlers/fleet/runner_leases.zig` | EDIT | accept + validate the workspace filter; totals from bounded reads |
| `src/agentsfleetd/http/handlers/fleet/runner_get.zig` | EDIT | detail read serves lifetime stats from the counter row |
| `src/agentsfleetd/http/handlers/fleet/sql.zig` | EDIT | filtered page/count statements; counter-joined detail statement |
| `src/agentsfleetd/fleet/service_lease_row.zig` | EDIT | increment `acquired` in the same statement/transaction as lease insert |
| `src/agentsfleetd/fleet/service_report.zig` | EDIT | increment terminal counters with the terminal status write |
| `src/agentsfleetd/fleet/renewal_settle.zig` | EDIT | count expiry transitions where they are settled |
| `src/agentsfleetd/fleet/retention_sweeper.zig` | CREATE | terminal lease/event retention sweep (new sweeper) |
| `src/agentsfleetd/cmd/serve_background.zig` | EDIT | register the retention sweeper |
| `src/agentsfleetd/cmd/backfill.zig` | EDIT | idempotent counter backfill from existing lease history |
| `src/agentsfleetd/state/account_teardown.zig` | EDIT | unregister Upstash QStash schedules; emit failure signal |
| `src/agentsfleetd/observability/metrics_counters.zig` | EDIT | retention + teardown-failure operational counters |
| `schema/042_runner_lifetime_counters.sql` | CREATE | per-runner lifetime counter table |
| `schema/043_runner_events_type_index.sql` | CREATE | partial index for the lifecycle-tag activity reads |
| `schema/embed.zig` | EDIT | register both migrations |
| `src/agentsfleetd/db/index_usage_integration_test.zig` | EDIT | plan proofs for the new reads |
| `ui/packages/app/app/(dashboard)/admin/runners/[runnerId]/components/LeaseTable.tsx` | EDIT | Workspace column + filter control |
| `ui/packages/app/app/(dashboard)/admin/runners/[runnerId]/page.tsx` | EDIT | thread the workspace filter search param |
| `ui/packages/app/app/(dashboard)/admin/runners/[runnerId]/components/runner-copy.ts` | EDIT | column/filter labels |
| `ui/packages/app/lib/api/runners.ts` | EDIT | workspace filter param on the lease list client |
| `ui/packages/app/components/domain/fleetFailureCopy.tsx` | EDIT | split runner-refusal copy from needs-instructions copy |
| `ui/packages/app/components/domain/FleetMessageRow.test.tsx` | EDIT | copy-split coverage |
| `ui/packages/app/tests/e2e/acceptance/fleet-thread.spec.ts` | EDIT | add the missing `afterEach` fleet cleanup |
| `ui/packages/app/tests/e2e/acceptance/operator-journey.spec.ts` | EDIT | capture teardown state at creation, not at success |
| `ui/packages/app/tests/e2e/acceptance/global-teardown.ts` | EDIT | fleet-prefix + stale journey-workspace sweep |
| `ui/packages/app/tests/e2e/acceptance/fixtures/teardown.ts` | EDIT | shared sweep helpers the global teardown calls |
| `deploy/baremetal/agentsfleet-runner.service` | EDIT | comment-only: stale "allow_all is the current default" claim (Indy-approved deploy-config edit, Jul 30, 2026) |
| `docs/architecture/runner_fleet.md` | EDIT | counters, retention, and purge flow become the documented shape |

New unit/integration test files colocate with the files above per repo convention and are in scope.

## Applicable Rules

- **`~/Projects/dotfiles/docs/greptile-learnings/RULES.md`** — UFS (retention window, failure-detail literals, event names as named constants), NDC, NLR (retire the read-time aggregate and dead SQL constants in the touched files), ORP (sweep removed SQL constant symbols), NSQ (schema-qualified SQL, named constants), KYS (workspace filter composes with the existing composite keyset cursor), MIG (migration-array position assertions), STS (no static strings in the new schema files), TST-NAM, XCC, FLS (every `conn.query()` drained), OWN (one owner for counter writes), ECL (teardown distinguishes retryable Upstash QStash failures from fatal).
- `dispatch/write_zig.md` — pg-drain, tagged-union results, errdefer, file/function length, cross-compile both linux targets.
- `dispatch/write_sql.md` — Schema Table Removal Guard n/a (no drops); ≤100-line single-concern migrations; `schema/embed.zig` + migration array updated together.
- `dispatch/write_ts_adhere_bun.md` — LeaseTable/copy edits stay on design-system primitives and token utilities.
- `~/Projects/dotfiles/docs/REST_API_DESIGN_GUIDELINES.md` — the new query parameter's naming, validation, and error shape.
- `~/Projects/dotfiles/docs/LOGGING_STANDARD.md` — sweeper and teardown log lines carry registered error codes.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | yes — handlers, sweepers, teardown | cross-compile x86_64-linux + aarch64-linux; drain audit via `make check-pg-drain` |
| PUB / Struct-Shape | yes — new sweeper module | shape verdict recorded at PLAN for each new pub surface |
| File & Function Length (≤350/≤50/≤70) | yes — `sql.zig` is near budget | move new statements to a sibling split if the cap approaches; no gate-silencing |
| UFS (repeated/semantic literals) | yes | retention window, counter column names, detail literals, event types as named constants; UI matches runner detail strings via one shared constants module |
| UI Substitution / DESIGN TOKEN | yes — LeaseTable filter control | design-system select/badge primitives; token utilities only |
| LOGGING / LIFECYCLE / ERROR REGISTRY / SCHEMA | yes | new `UZ-` codes for filter validation + teardown unregister failure; SCHEMA GUARD satisfied by two single-concern migrations registered in `schema/embed.zig` |

## Prior-Art / Reference Implementations

- **Reference:** `core.fleet_activity_counters` (schema/030 + `state/fleet_events_store.zig`) — the incrementally-maintained counter shape this spec extends to runners; divergence: one row per runner, not per fleet-day.
- **Reference:** `schema/041` + `db/index_usage_integration_test.zig` — measured-index discipline: every new index lands with a plan proof.
- **Reference:** `lifecycle.spec.ts` `afterEach` + `fixtures/teardown.ts` — the teardown pattern the two non-compliant specs adopt.

## Sections (implementation slices)

### §1 — Workspace correlation on the runner lease surface

The lease list API accepts an optional workspace filter and the admin table renders the ownership it already receives. This kills the "whose fleet is this?" gap that made fixture noise indistinguishable from a customer's work.

- **Dimension 1.1** — `GET /v1/fleets/runners/{runner_id}/leases` accepts an optional `workspace_id` query parameter; the filtered page returns only matching leases and keyset pagination stays stable across pages → Test `test_runner_leases_workspace_filter_pages`
- **Dimension 1.2** — a malformed `workspace_id` returns 400 with a registered error code; an unknown one returns an empty page, not an error → Test `test_runner_leases_workspace_filter_rejects_malformed`
- **Dimension 1.3** — LeaseTable renders a Workspace column (link via `workspacePath`) fed from the existing `workspace_id` field; no additional fetch per row → Test `test_lease_table_workspace_column`
- **Dimension 1.4** — the filter is a URL search param: deep-linkable, survives reload, and composes with the existing cursor trail → Test `test_lease_table_workspace_filter_deep_link` (e2e)

### §2 — Activity feed reads become index-served

The seven lifecycle tags the user interface (UI) requests are rare; the two per-lease tags dominate the table. Filtered page and count reads must be served by an index that carries `event_type`. **Implementation default:** a partial index excluding the two high-volume per-lease tags, because the UI never requests them and the index stays small; the agent proves the plan with the existing `expectServesFilter` harness.

- **Dimension 2.1** — the lifecycle-filtered activity page read is index-served → Test `test_runner_events_type_filter_plan`
- **Dimension 2.2** — the lifecycle-filtered activity count read is index-served → Test `test_runner_events_type_count_plan`
- **Dimension 2.3** — the migration is registered in `schema/embed.zig` and the migration array with position assertions → Test `test_migration_array_positions`

### §3 — Runner lifetime counters maintained at write time

A per-runner counter row (acquired/succeeded/failed/expired) is incremented in the same transaction as the lease write that changes the tally; the detail read joins that row and never aggregates `fleet.runner_leases`. Live-now numbers (active leases, active fleets) stay computed from the existing `(runner_id, status)` index — they are point-in-time, not lifetime. The lease pager's exact total stays: §6's retention bounds the per-runner row count, and the existing slot-041 index serves it.

- **Dimension 3.1** — counters are exact under concurrent lease churn: after N parallel acquire/report cycles the counter row equals a recount → Test `test_runner_counters_match_recount_under_churn`
- **Dimension 3.2** — the detail read touches only `fleet.runners` + the counter row; the plan shows no scan of `fleet.runner_leases` → Test `test_runner_detail_counter_plan`
- **Dimension 3.3** — the backfill command populates counters from existing history and is idempotent on rerun → Test `test_counter_backfill_idempotent`
- **Dimension 3.4** — the lease list total is index-served post-retention → Test `test_runner_lease_total_plan`

### §4 — Startup-posture chat copy names the real refusal

`startup_posture` covers both "the fleet has no instructions" and "the runner refused the lease" (sandbox/egress/resource-domain). The chat surface distinguishes them by the failure detail, keyed on the runner's detail literals shared through one named-constants module. **Implementation default:** detail-literal matching, because a new wire failure class is a bigger change this spec rejects (Out of Scope).

- **Dimension 4.1** — an egress/sandbox/resource-domain detail renders runner-refusal copy, not "needs instructions" → Test `test_chat_copy_runner_refusal`
- **Dimension 4.2** — the no-instructions detail keeps today's copy verbatim (regression) → Test `test_chat_copy_needs_instructions_regression`

### §5 — E2E suites stop leaking fleets

Leaked fixture fleets carry live cron schedules that hammer runners around the clock — 16 `journey-fleet-*` and several shared-workspace leftovers were swept by hand on Jul 30, 2026. The suites become self-cleaning and the global teardown becomes the backstop for fleets, not just users.

- **Dimension 5.1** — `fleet-thread.spec.ts` gains the missing `afterEach` scoped to its `steer-probe-` prefix → Test: suite run leaves zero `steer-probe-` fleets
- **Dimension 5.2** — `operator-journey.spec.ts` records workspace/fleet identifiers at creation so `afterEach` cleans up even when the test fails before minting its API key → Test: forced mid-test failure still tears down
- **Dimension 5.3** — `global-teardown.ts` sweeps known leak prefixes across fixture workspaces and reaps stale `journey-*` workspaces past the existing staleness window → Test: a seeded leak disappears on a direct `bun global-teardown.ts` run

### §6 — Retention sweep for terminal runner rows

A background sweeper (registered like the liveness/reclaim sweepers) deletes terminal-status `fleet.runner_leases` and `fleet.runner_events` rows older than the retention window, in bounded batches. **Implementation default:** 30-day window as a named constant, terminal statuses only, because lifetime tallies now live in §3's counters and the operator surface pages newest-first.

- **Dimension 6.1** — rows older than the window with terminal status are deleted; each cycle emits an operational counter → Test `test_retention_sweeps_terminal_rows`
- **Dimension 6.2** — active/renewing leases and in-window rows are never touched (negative) → Test `test_retention_spares_live_and_recent`

### §7 — Account teardown purges schedules end-to-end and fails loud

Teardown today deletes schedule rows by cascade but never unregisters the Upstash QStash side, and a missed purge is invisible. Replaying `user.deleted` must leave zero fleets, zero schedule rows, zero Upstash QStash registrations — and a purge failure must be observable.

- **Dimension 7.1** — teardown unregisters every Upstash QStash schedule belonging to the tenant's fleets before deleting rows (client mocked at the system boundary) → Test `test_teardown_unregisters_qstash`
- **Dimension 7.2** — replaying the same `user.deleted` twice is a no-op the second time → Test `test_teardown_replay_idempotent`
- **Dimension 7.3** — an unregister failure logs a registered error code, increments a failure counter, and does not abort the row purge → Test `test_teardown_unregister_failure_observable`

## Interfaces

```
GET /v1/fleets/runners/{runner_id}/leases?workspace_id=<uuid>   (new optional filter;
    malformed value → 400 with a registered UZ- code; response shape unchanged)
fleet.runner_lifetime_counters — one row per runner; monotonic acquired/succeeded/
    failed/expired tallies; written only by the lease write paths and the backfill
No other wire shapes change: runner detail/lease/event response fields stay as-is.
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Malformed workspace filter | non-UUID query value | 400 + registered error code; page renders its error state |
| Counter write contention | concurrent terminal reports for one runner | per-row atomic increments inside the existing lease transaction; recount equality proven under churn |
| Retention races a live lease | sweep overlaps an acquire/renew | terminal-status + age predicate excludes it; negative test |
| Upstash QStash unregister fails | provider 5xx/timeout during teardown | log registered code + increment failure counter, continue purge; replay retries (idempotent) |
| Backfill rerun on populated counters | operator runs backfill twice | idempotent upsert from recount; second run changes nothing |
| Fixture cleanup hits state machine | fleet not yet killed at delete | existing kill-then-delete helper path; teardown tolerates per-fleet failure and reports count |

## Invariants

1. The runner detail read never scans `fleet.runner_leases` — enforced by a plan-proof integration test that fails on regression.
2. Lifetime counters are monotonic and equal a recount after any churn sequence — enforced by the churn integration test.
3. Retention deletes only terminal-status rows older than the named window — enforced by the SQL predicate plus the negative test.
4. Every row on a workspace-filtered page carries the filtered `workspace_id` — enforced by the filter integration test.
5. After teardown replay, the tenant has zero fleets, schedule rows, and Upstash QStash registrations — enforced by the extended teardown integration test.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| `runner_retention_swept_total` | ops | each retention sweep cycle | rows deleted per table, runner count | ids only, no tenant content | `test_retention_sweeps_terminal_rows` |
| `account_teardown_unregister_failures_total` | ops | Upstash QStash unregister fails during purge | tenant id, schedule count | no user email/token material | `test_teardown_unregister_failure_observable` |

No product analytics events are added, renamed, or removed; no analytics/funnel playbook update is required.

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | integration | `test_runner_leases_workspace_filter_pages` | two workspaces' leases seeded → filtered pages contain only the filtered workspace, cursor stable |
| 1.2 | integration | `test_runner_leases_workspace_filter_rejects_malformed` | `workspace_id=not-a-uuid` → 400 + code; unknown uuid → empty page |
| 1.3 | unit | `test_lease_table_workspace_column` | lease item with `workspace_id` → column renders link; no extra fetch issued |
| 1.4 | e2e | `test_lease_table_workspace_filter_deep_link` | operator loads filtered URL → only matching rows; reload preserves filter |
| 2.1 | integration | `test_runner_events_type_filter_plan` | lifecycle-tag page query plan uses the new partial index |
| 2.2 | integration | `test_runner_events_type_count_plan` | lifecycle-tag count query plan uses the new partial index |
| 2.3 | integration | `test_migration_array_positions` | both migrations present at asserted positions |
| 3.1 | integration | `test_runner_counters_match_recount_under_churn` | ≥100 parallel acquire/report cycles → counter row == recount |
| 3.2 | integration | `test_runner_detail_counter_plan` | detail query plan shows no `fleet.runner_leases` scan |
| 3.3 | integration | `test_counter_backfill_idempotent` | backfill twice over seeded history → identical counters |
| 3.4 | integration | `test_runner_lease_total_plan` | total query plan is index-served |
| 4.1 | unit | `test_chat_copy_runner_refusal` | egress/sandbox/resource detail → runner-refusal sentence + cause |
| 4.2 | unit | `test_chat_copy_needs_instructions_regression` | no-instructions detail → today's copy verbatim |
| 5.1–5.3 | e2e | suite + direct teardown runs | seeded leaks removed; forced mid-test failure still cleans up |
| 6.1 | integration | `test_retention_sweeps_terminal_rows` | aged terminal rows deleted; counter emitted with row counts |
| 6.2 | integration | `test_retention_spares_live_and_recent` | active + in-window rows survive a sweep cycle |
| 7.1 | integration | `test_teardown_unregisters_qstash` | tenant with scheduled fleets → unregister called per schedule before row purge |
| 7.2 | integration | `test_teardown_replay_idempotent` | second `user.deleted` replay → zero additional deletes, no error |
| 7.3 | integration | `test_teardown_unregister_failure_observable` | injected unregister failure → code logged, counter incremented, rows still purged |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Workspace filter returns only matching leases (§1) | `make test-integration` | exit 0; filter tests listed as passed | P0 | |
| R2 | Runner detail plan has no lease-table scan (§3) | `make test-integration` | `test_runner_detail_counter_plan` passed | P0 | |
| R3 | Activity filtered reads index-served (§2) | `make test-integration` | both plan tests passed | P0 | |
| R4 | Chat copy split (§4) | `make test-unit-app` | both copy tests passed | P1 | |
| R5 | Teardown purges Upstash QStash end-to-end (§7) | `make test-integration` | all three §7 tests passed | P0 | |
| R6 | Diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 paths missing from the Files Changed table | P0 | |
| S1 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | |
| S2 | Lint clean | `make lint-all` | exit 0 | P0 | |
| S3 | Integration passes | `make test-integration` | exit 0 | P0 | |
| S4 | E2E walks the operator path | `make acceptance-e2e` | exit 0 | P0 | |
| S5 | No leaks | `make memleak` | exit 0 | P0 | |
| S6 | Cross-compile | `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` | exit 0 | P0 | |
| S7 | No secrets | `gitleaks detect` | exit 0 | P0 | |

**Grading protocol (VERIFY):** run the Verify command verbatim; grade ONLY from its output. Graded = ✅/❌ + the one decisive output line (`342 passed`); long evidence goes to PR Session Notes with a pointer here. **Ship gate:** every row graded, every P0 ✅ → eligible for CHORE(close); any ❌ or empty cell → return to EXECUTE; a P1 ❌ ships only with an Indy-acked deferral quote in Discovery.

## Dead Code Sweep

**1. Orphaned files — deleted from disk and git.**

N/A — no files deleted.

**2. Orphaned references — zero remaining imports/uses.**

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| the read-time detail aggregate statement (name per `sql.zig`) | `grep -rn "SELECT_RUNNER_DETAIL" src/ \| head` | only the counter-joined replacement remains |
| the standalone lease total statement if retired | `grep -rn "SELECT_RUNNER_LEASE_TOTAL" src/ \| head` | 0 stale matches |

## Out of Scope

- A tenant-facing "my runner activity" surface — M148_001 (pending) owns runner policy/visibility evolution.
- A new wire-level failure class for runner refusals — detail-literal split ships first; class change is a follow-up decision.
- A platform-admin tenant-purge API endpoint — reconciliation of the one known dev orphan is operational work, tracked outside this PR.
- Automated provisioning of runner host environment files (the `RUNNER_NETWORK_POLICY` incident's ops fix is manual and already in flight).
- Any change to lease acquire/report wire shapes or the runner token trust plane.

---

## Product Clarity (authoring record)

1. **Successful user moment** — Indy opens runner `ant`'s Leases tab, picks `gentle-mesa-130` in the workspace filter, and every visible row is his; the failed chat row reads "the runner refused this run — strict egress policy is not implemented on this runner" instead of blaming missing instructions.
2. **Preserved user behaviour** — existing columns, newest-first ordering, cursor pagination, the row drawer with its fleet link, all current response fields, and the genuine needs-instructions copy stay unchanged.
3. **Optimal-way check** — the correlation fix is optimal (the field is already on the wire). Counters-at-write is the standard bounded-read shape with in-repo prior art. The unconstrained optimum for attribution is a tenant-scoped runner view; deliberately deferred to M148's policy work.
4. **Rebuild-vs-iterate** — iterate. The read path is sound (keyset + measured indexes); it needs a counter row, one index, and a rendered column — not a redesign. Determinism is untouched.
5. **What we build** — workspace filter param + column, one partial index, one counter table with write-path increments + backfill, copy split, three test-hygiene edits, one retention sweeper, teardown unregister + failure signal.
6. **What we do NOT build** — admin purge API (security surface, no urgent need); wire failure-class change (bigger blast radius than the copy bug); Upstash QStash provider migration (works; the bug was ours).
7. **Fit with existing features** — compounds M146's lease-first surface and the slot-041 index work; must not destabilize the lease acquire/report hot path — §3 shares its transaction and proves exactness under churn.
8. **Surface order** — UI + API together in one PR; the CLI is untouched because the admin runner surface is dashboard-native (M146 decision).
9. **Dashboard restraint** — no new charts or controls beyond the workspace filter; counters replace numbers the strip already shows; the filter renders only when the runner has leases.
10. **Confused-user next step** — the failure copy itself names the runner posture cause; the runner docs page gains the workspace-filter mention at CHORE(close) via the docs repo flow.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** seven sections in one workstream because they share one surface (the runner lease read/write path) and one PR budget; splitting UI from API would put two PRs on the same tables in the same week.
- **Alternatives considered:** (a) materialized view for lifetime stats — rejected: write-path counters are simpler, transactional, and have in-repo prior art; (b) dropping the pager total — rejected: retention bounds the count and the index serves it; (c) full index (not partial) on `runner_events` including the high-volume tags — rejected: doubles index write cost for reads the UI never issues.
- **Patch-vs-refactor verdict:** this is a **patch** — solution-size matches problem-size; the only structural piece (counters) is the follow-up M146 explicitly named, landing in its intended shape.

## Discovery (consult log)

- **Consults** — Architecture / Legacy-Design / gate-flag triage: the question asked + Indy's decision.
- **Metrics review** — events added, extra events found during `/review`, analytics/funnel playbook update or the explicit no-change reason.
- **Skill-chain outcomes** — `/write-unit-test`, `/review`, `kishore-babysit-prs` results (order per `AGENTS.md` CHORE(close); iteration counts, findings dispositioned).
- **Deferrals** — every "deferred to follow-up" needs an **Indy-acked verbatim quote** here, format `> Indy (YYYY-MM-DD HH:MM): "<quote>" — context: <which item, why>`. An agent-unilateral deferral is **incomplete scope, not deferral**, and blocks CHORE(close) until the item lands or the quote is captured.
