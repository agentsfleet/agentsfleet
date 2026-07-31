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
**Depends on:** none for correctness, but M148_001 (in progress on `feat/m148-assigned-runner-policy`) overlaps at merge: it edits `schema/embed.zig`, `public/openapi{.json,/paths/fleet.yaml}`, `src/agentsfleetd/http/handlers/fleet/sql.zig`, and `ui/packages/app/lib/api/runners.ts` too, and it claimed schema slot 042 first. This workstream's three migrations were renumbered to 043–045 to clear it (Indy's call, Jul 30, 2026), leaving slot 042 vacant for M148 whichever order the two land.
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
| `src/agentsfleetd/fleet/sql_lease_row.zig` | CREATE | lease-insert statement + acquired tally arm (length-split from `sql.zig`, re-exported) |
| `src/agentsfleetd/fleet/sql.zig` | EDIT | lease-row re-export after the length split |
| `src/agentsfleetd/fleet/service_report.zig` | EDIT | pass the report outcome into the claim so the tally rides it |
| `src/agentsfleetd/fleet/service_report_outbound.zig` | CREATE | connector-outbound answer delivery (length-split from `service_report.zig` once the outcome threading pushed it past budget) |
| `src/agentsfleetd/fleet/renewal_settle.zig` | EDIT | succeeded/failed tally arm inside the claim statement |
| `src/agentsfleetd/fleet/reclaim.zig` | EDIT | expired tally arm with the active→expired flip |
| `src/agentsfleetd/fleet/retention_sweeper.zig` | CREATE | terminal lease/event retention sweep (new sweeper) |
| `src/agentsfleetd/cmd/serve_background.zig` | EDIT | register the retention sweeper |
| `src/agentsfleetd/state/account_teardown.zig` | EDIT | resolve the tenant's fleet ids pre-purge for schedule unregistering |
| `src/agentsfleetd/http/handlers/auth/identity_events_clerk.zig` | EDIT | route the `user.deleted` arm to its own module after the length split |
| `src/agentsfleetd/http/handlers/auth/identity_events_delete.zig` | CREATE | the `user.deleted` arm: unregister Upstash QStash schedules before purge; emit failure signal |
| `src/agentsfleetd/observability/metrics_counters.zig` | EDIT | retention + teardown-failure operational counters |
| `src/agentsfleetd/observability/metrics_render.zig` | EDIT | render the two new families |
| `schema/043_runner_lifetime_counters.sql` | CREATE | per-runner lifetime counter table + in-migration backfill (slot 030 shape) |
| `schema/044_runner_events_read_index.sql` | CREATE | composite index for the lifecycle-tag activity reads |
| `schema/045_runner_retention_delete_grants.sql` | CREATE | DELETE grants the retention sweep needs |
| `schema/embed.zig` | EDIT | register the three migrations |
| `public/openapi/paths/fleet.yaml` | EDIT | document the lease-list workspace filter |
| `public/openapi.json` | EDIT | regenerated bundle |
| `src/agentsfleetd/db/index_usage_integration_test.zig` | EDIT | plan proofs for the new reads |
| `src/agentsfleetd/fleet/runner_counters_integration_test.zig` | CREATE | counter exactness under churn/retry |
| `src/agentsfleetd/fleet/retention_sweeper_integration_test.zig` | CREATE | retention deletes aged terminal rows only |
| `src/agentsfleetd/integration_tests.zig` | EDIT | register the two new integration test modules |
| `src/agentsfleetd/http/runner_read_integration_test.zig` | EDIT | workspace-filter and counter-backed detail coverage |
| `src/agentsfleetd/http/handlers/auth/identity_events_clerk_integration_test.zig` | EDIT | §7's three proofs, in the file that already owns the signed-webhook harness |
| `src/agentsfleetd/fleet/renewal_metering_test.zig` | EDIT | claim signature carries the outcome |
| `src/agentsfleetd/fleet/concurrency_renew_test.zig` | EDIT | claim signature carries the outcome |
| `ui/packages/app/app/(dashboard)/admin/runners/[runnerId]/components/LeaseTable.tsx` | EDIT | Workspace column + filter control |
| `ui/packages/app/app/(dashboard)/admin/runners/[runnerId]/components/LeaseWorkspaceFilter.tsx` | CREATE | the filter's client half: the search-param hook plus the active-filter chip |
| `ui/packages/app/app/(dashboard)/admin/runners/[runnerId]/page.tsx` | EDIT | thread the workspace filter search param |
| `ui/packages/app/app/(dashboard)/admin/runners/[runnerId]/components/runner-copy.ts` | EDIT | column/filter labels |
| `ui/packages/app/lib/api/runners.ts` | EDIT | workspace filter param on the lease list client |
| `ui/packages/app/lib/api/runners.test.ts` | EDIT | the filter rides the query string; an empty id sends no filter at all |
| `ui/packages/app/app/(dashboard)/admin/runners/[runnerId]/components/LeaseTable.test.tsx` | EDIT | Workspace column, active-filter chip, and link-vs-row click separation |
| `ui/packages/app/tests/runner-detail-page.test.ts` | EDIT | the page threads the workspace search param and fails closed on a malformed one |
| `ui/packages/app/components/domain/fleetFailureCopy.tsx` | EDIT | split runner-refusal copy from needs-instructions copy |
| `ui/packages/app/components/domain/fleetFailureCopy.test.ts` | CREATE | copy-split coverage, colocated with the module under test rather than with `FleetMessageRow` |
| `ui/packages/app/tests/e2e/acceptance/runner-detail.spec.ts` | EDIT | Dimension 1.4's deep-link walk; its arrangement is shared with the existing triage walk |
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

- **Dimension 1.1** — `GET /v1/fleets/runners/{runner_id}/leases` accepts an optional `workspace_id` query parameter; the filtered page returns only matching leases and keyset pagination stays stable across pages → Test `test_runner_leases_workspace_filter_scopes_rows_and_total` — **DONE**
- **Dimension 1.2** — a malformed `workspace_id` returns 400 with a registered error code; an unknown one returns an empty page, not an error → Test `test_runner_leases_rejects_malformed_workspace_filter` — **DONE**
- **Dimension 1.3** — LeaseTable renders a Workspace column (link via `workspacePath`) fed from the existing `workspace_id` field; no additional fetch per row → Test `renders the workspace cell as a shortened link carrying the full id` — **DONE**
- **Dimension 1.4** — the filter is a URL search param: deep-linkable, survives reload, and composes with the existing cursor trail → Test `test_lease_table_workspace_filter_deep_link` (e2e) — **DONE**

### §2 — Activity feed reads become index-served

The seven lifecycle tags the user interface (UI) requests are rare; the two per-lease tags dominate the table. Filtered page and count reads must be served by an index that carries `event_type`. **Implementation default (superseded — see Discovery C2):** a partial index excluding the two high-volume per-lease tags was the stated default, but the reads bind their tag list as a parameter array (`event_type = ANY($n)`) and the planner cannot prove a bound parameter satisfies a partial-index predicate, so generic plans fall off it. **Shipped shape:** the full composite `(runner_id, event_type, occurred_at DESC, id DESC)`, proven with the existing `expectServesFilter` harness.

- **Dimension 2.1** — the lifecycle-filtered activity page read is index-served → Test `events composite has the right shape and serves the filtered feed` — **DONE**
- **Dimension 2.2** — the lifecycle-filtered activity count read is index-served → Test `events composite has the right shape and serves the filtered feed` (its count probe) — **DONE**
- **Dimension 2.3** — the migration is registered in `schema/embed.zig` and the migration array with position assertions → Test `counter and retention slots are registered in the migration array` — **DONE**

### §3 — Runner lifetime counters maintained at write time

A per-runner counter row (acquired/succeeded/failed/expired) is incremented in the same transaction as the lease write that changes the tally; the detail read joins that row and never aggregates `fleet.runner_leases`. Live-now numbers (active leases, active fleets) stay computed from the existing `(runner_id, status)` index — they are point-in-time, not lifetime. The lease pager's exact total stays: §6's retention bounds the per-runner row count, and the existing slot-041 index serves it.

- **Dimension 3.1** — counters are exact under concurrent lease churn: after N parallel acquire/report cycles the counter row equals a recount → Test `counter row equals a recount after concurrent acquire and settle cycles` — **DONE**
- **Dimension 3.2** — the detail read touches only `fleet.runners` + the counter row; the plan shows no scan of `fleet.runner_leases` → Test `runner detail read never forces a full lease-history scan` — **DONE**
- **Dimension 3.3** — the migration backfills counters from existing history (slot 030's in-migration shape — no separate command) and reapplying the upsert is idempotent → Test `the migration backfill reconstructs the tallies and is idempotent on reapply` — **DONE**
- **Dimension 3.4** — the lease list total is index-served post-retention → Test `the lease pager.s exact total never walks the runner.s whole history` — **DONE**

### §4 — Startup-posture chat copy names the real refusal

`startup_posture` covers both "the fleet has no instructions" and "the runner refused the lease" (sandbox/egress/resource-domain). The chat surface distinguishes them by the failure detail, keyed on the runner's detail literals shared through one named-constants module. **Implementation default:** detail-literal matching, because a new wire failure class is a bigger change this spec rejects (Out of Scope).

- **Dimension 4.1** — an egress/sandbox/resource-domain detail renders runner-refusal copy, not "needs instructions" → Test `reads a runner refusal from the detail` — **DONE**
- **Dimension 4.2** — the no-instructions detail keeps today's copy verbatim (regression) → Test `keeps the needs-instructions sentence verbatim for a non-refusal detail` — **DONE**

### §5 — E2E suites stop leaking fleets

Leaked fixture fleets carry live cron schedules that hammer runners around the clock — 16 `journey-fleet-*` and several shared-workspace leftovers were swept by hand on Jul 30, 2026. The suites become self-cleaning and the global teardown becomes the backstop for fleets, not just users.

- **Dimension 5.1** — `fleet-thread.spec.ts` gains the missing `afterEach` scoped to its `steer-probe-` prefix → Test: suite run leaves zero `steer-probe-` fleets — **DONE**
- **Dimension 5.2** — `operator-journey.spec.ts` records workspace/fleet identifiers at creation so `afterEach` cleans up even when the test fails before minting its API key → Test: forced mid-test failure still tears down — **DONE**
- **Dimension 5.3** — `global-teardown.ts` sweeps known leak prefixes across fixture workspaces and reaps stale `journey-*` workspaces past the existing staleness window → Test: a seeded leak disappears on a direct `bun global-teardown.ts` run — **DONE**

### §6 — Retention sweep for terminal runner rows

A background sweeper (registered like the liveness/reclaim sweepers) deletes terminal-status `fleet.runner_leases` and `fleet.runner_events` rows older than the retention window, in bounded batches. **Implementation default:** 30-day window as a named constant, terminal statuses only, because lifetime tallies now live in §3's counters and the operator surface pages newest-first.

- **Dimension 6.1** — rows older than the window with terminal status are deleted; each cycle emits an operational counter → Test `sweep loop reports deleted rows to the retention metric` — **DONE**
- **Dimension 6.2** — active/renewing leases and in-window rows are never touched (negative) → Test `one sweep deletes aged terminal history and spares live and in-window rows` — **DONE**

### §7 — Account teardown purges schedules end-to-end and fails loud

Teardown today deletes schedule rows by cascade but never unregisters the Upstash QStash side, and a missed purge is invisible. Replaying `user.deleted` must leave zero fleets, zero schedule rows, zero Upstash QStash registrations — and a purge failure must be observable.

- **Dimension 7.1** — teardown unregisters every Upstash QStash schedule belonging to the tenant's fleets before deleting rows (client mocked at the system boundary) → Test `teardown unregisters the tenant.s schedules BEFORE it purges the rows` — **DONE**
- **Dimension 7.2** — replaying the same `user.deleted` twice is a no-op the second time → Test `replaying user.deleted is a no-op the second time` — **DONE**
- **Dimension 7.3** — an unregister failure logs a registered error code, increments a failure counter, and does not abort the row purge → Test `a provider unregister failure is counted, and the purge still happens` — **DONE**

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
| Upstash QStash unregister fails | provider 5xx/timeout during teardown | log registered code + increment failure counter; purge proceeds (erasure wins). A later replay cannot retry it — the rows are gone — so the counter is the reconciliation signal |
| Backfill rerun on populated counters | migration reapplied | idempotent upsert from recount; second run changes nothing |
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
| `agentsfleet_runner_retention_swept_total` | ops | each retention sweep cycle that deleted rows | none — unlabelled counter; the per-table split rides the `sweep_completed` log line | ids only, no tenant content | `sweep loop reports deleted rows to the retention metric` |
| `agentsfleet_account_teardown_unregister_failures_total` | ops | Upstash QStash unregister fails during purge | none — unlabelled counter; the tenant and fleet ids ride the log line | no user email/token material | `a provider unregister failure is counted, and the purge still happens` |

Both families are deliberately unlabelled, matching the existing lease-poll counters: they describe control-plane housekeeping rather than any one tenant's work, so a per-tenant label would add cardinality without adding an operator answer. The reconciliation detail an operator needs after a non-zero reading lives in the correlated log line.

No product analytics events are added, renamed, or removed; no analytics/funnel playbook update is required.

## Test Specification (tiered)

Test names below are the names on disk. Where a Dimension is covered by a
behaviour-named test rather than the `test_*` identifier this spec drafted, the
real name wins — a spec that names a test nobody wrote proves nothing.

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | integration | `test_runner_leases_workspace_filter_scopes_rows_and_total` | two workspaces' leases seeded → filtered pages contain only the filtered workspace, and the total narrows with them |
| 1.2 | integration | `test_runner_leases_rejects_malformed_workspace_filter` + `test_runner_leases_unknown_workspace_filter_returns_empty_page` | `workspace_id=not-a-uuid` → 400 + code; well-formed unknown uuid → empty page, not an error |
| 1.3 | unit | `renders the workspace cell as a shortened link carrying the full id` (`LeaseTable.test.tsx`) | lease item with `workspace_id` → column renders link with the full id in `title`; no extra fetch |
| 1.4 | e2e | `test_lease_table_workspace_filter_deep_link` (`runner-detail.spec.ts`) | operator loads the filtered URL → every row's workspace matches; reload keeps it; Back returns to it; an unowned id renders the empty state |
| 2.1 | integration | `events composite has the right shape and serves the filtered feed` | lifecycle-tag page read (`SELECT_RUNNER_EVENT_KEYSET_FIRST`) plans onto the new composite |
| 2.2 | integration | same test, second probe | lifecycle-tag count read (`SELECT_RUNNER_EVENT_COUNT`) plans onto the same composite |
| 2.3 | integration | `counter and retention slots are registered in the migration array` | slots 43/44/45 each resolve through `schema.migrations` — an unregistered slot never runs |
| 3.1 | integration | `counter row equals a recount … after mixed transitions` + `… after concurrent acquire and settle cycles` + `retried settle increments the tally exactly once` | mixed and 8-way-concurrent acquire/settle/expire cycles → counter row == recount; a retried settle counts once |
| 3.2 | integration | `runner detail read never forces a full lease-history scan` + `test_runner_get_lifetime_counters_from_durable_state` | detail plan carries no `Seq Scan on runner_leases` under forced index scans; the read serves tallies from the counter row |
| 3.3 | integration | `the migration backfill reconstructs the tallies and is idempotent on reapply` | real slot-43 backfill text over seeded history → equals the write-time arms; reapplied → unchanged |
| 3.4 | integration | `the lease pager's exact total never walks the runner's whole history` | both binds (NULL and a workspace id) plan without a full lease scan |
| 4.1 | unit | `reads a runner refusal from the detail` (each refusal literal) | egress/sandbox/resource detail → runner-refusal sentence + cause |
| 4.2 | unit | `keeps the needs-instructions sentence verbatim for a non-refusal detail` | no-instructions detail → today's copy verbatim |
| 5.1–5.3 | e2e | suite + direct teardown runs | seeded leaks removed; forced mid-test failure still cleans up |
| 6.1 | integration | `sweep loop reports deleted rows to the retention metric` | aged terminal rows deleted; counter emitted with row counts |
| 6.2 | integration | `one sweep deletes aged terminal history and spares live and in-window rows` | active + in-window rows survive the same sweep cycle that deletes the aged ones |
| 7.1 | integration | `teardown unregisters the tenant's schedules BEFORE it purges the rows` | tenant with a synced schedule → the faked provider is called, and the row it names is still on disk at that moment |
| 7.2 | integration | `replaying user.deleted is a no-op the second time` | second `user.deleted` → 200, zero additional provider calls, account still gone |
| 7.3 | integration | `a provider unregister failure is counted, and the purge still happens` | forced provider 500 → failure counter incremented, response still 200, schedule and account rows gone |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Workspace filter returns only matching leases (§1) | `make test-integration` | exit 0; filter tests listed as passed | P0 | ✅ `test_runner_leases_workspace_filter_scopes_rows_and_total…OK` (plus the malformed and unknown-id pair) |
| R2 | Runner detail plan has no lease-table scan (§3) | `make test-integration` | `runner detail read never forces a full lease-history scan` passed | P0 | ✅ `runner detail read never forces a full lease-history scan…OK` |
| R3 | Activity filtered reads index-served (§2) | `make test-integration` | both plan tests passed | P0 | ✅ `events composite has the right shape and serves the filtered feed…OK` (asserts both the page and count statements) |
| R4 | Chat copy split (§4) | `make test-unit-app` | both copy tests passed | P1 | ✅ `Tests 2110 passed (2110)` — run through `make test-unit-all`, which contains the app lane |
| R5 | Teardown purges Upstash QStash end-to-end (§7) | `make test-integration` | all three §7 tests passed | P0 | ✅ all three `identity_events_clerk_integration_test…OK` |
| R6 | Diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 paths missing from the Files Changed table | P0 | |
| S1 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | ✅ `UNIT_ALL_EXIT=0` · `✓ All package coverage gates passed` |
| S2 | Lint clean | `make lint-all` | exit 0 | P0 | ✅ `LINT_EXIT=0` · `✓ All lint checks passed` |
| S3 | Integration passes | `make test-integration` | exit 0 | P0 | ✅ `730 passed; 8 skipped; 0 failed.` — see the grading note below on the command actually run |
| S4 | E2E walks the operator path | `make acceptance-e2e` | exit 0 | P0 | |
| S5 | No leaks | `make memleak` | exit 0 | P0 | ✅ `MEMLEAK_EXIT=0` · all four lanes ✓ · `378 passed; 7 skipped; 0 failed` |
| S6 | Cross-compile | `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` | exit 0 | P0 | ✅ `XCC x86_64-linux=0 aarch64-linux=0` |
| S7 | No secrets | `gitleaks detect` | exit 0 | P0 | ✅ `no leaks found` |

**Grading deviation, disclosed:** R1/R2/R3/R5/S3 were graded from the compiled integration binary run directly, not from `make test-integration` verbatim. That command wraps `zig build test-integration`, whose result protocol is corrupted by test-binary log noise — it prints `failed command:` on fully green runs and printed a passing marker on a genuinely red run earlier in this stream (Discovery C7). The binary was run with the exact environment the make target exports (`LIVE_DB`, `TEST_DATABASE_URL`, `TEST_REDIS_TLS_URL`, `REDIS_URL_API`, `REDIS_TLS_CA_CERT_FILE`, both `AGENTSFLEET_QSTASH_LIVE_*`, `AGENTSFLEET_RUNNER_BIN`) against a database reset with `make down` first (C10). Grading from the wrapper would have been grading from a known-unreliable reporter.

**Grading protocol (VERIFY):** run the Verify command verbatim; grade ONLY from its output. Graded = ✅/❌ + the one decisive output line (`342 passed`); long evidence goes to PR Session Notes with a pointer here. **Ship gate:** every row graded, every P0 ✅ → eligible for CHORE(close); any ❌ or empty cell → return to EXECUTE; a P1 ❌ ships only with an Indy-acked deferral quote in Discovery.

## Dead Code Sweep

**1. Orphaned files — deleted from disk and git.**

N/A — no files deleted.

**2. Orphaned references — zero remaining imports/uses.**

| Symbol | Grep | Expected | Result |
|--------|------|----------|--------|
| the read-time detail aggregate statement | `grep -rn "SELECT_RUNNER_DETAIL" src/ \| head` | only the counter-joined replacement remains | ✅ one definition (`handlers/fleet/sql.zig`), one production caller (`runner_get.zig`), one plan proof. The aggregate was rewritten in place rather than renamed, so there is no second statement to strand. |
| the standalone lease total statement | `grep -rn "SELECT_RUNNER_LEASE_TOTAL" src/ \| head` | 0 stale matches | ✅ **not retired, deliberately** — §3 keeps the pager's exact total, because §6's retention bounds the row count and the slot-041 index serves it. One definition, one production caller (`runner_leases.zig`), two plan proofs. The "if retired" branch of this row did not fire. |

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

### Consults

**C1 — Counter maintenance: statement arms, not triggers.** Slot 030's prior art uses database triggers. Rejected here: classifying a terminal lease transition needs the lease/event status vocabulary, and RULE STS keeps value vocabularies out of schema objects so they cannot drift from the application constants that write them. Each transition's owning statement instead carries a counter arm conditioned on the guarded write actually affecting rows — single owner per transition, transactional by construction, retry-safe. Recorded in `schema/043_runner_lifetime_counters.sql`.

**C2 — Partial index rejected; §2's stated implementation default loses to a measured plan.** §2 named a partial index excluding the two high-volume per-lease tags as the default. It does not work: the activity reads bind the tag list as a parameter array (`event_type = ANY($n)`), and the planner cannot prove a bound parameter satisfies a partial-index predicate, so generic plans fall off the index entirely. Shipped shape is the full composite `(runner_id, event_type, occurred_at DESC, id DESC)`, which stays usable for any parameterized tag set. Cost accepted: one extra index entry per event insert, bounded by §6's retention sweep. §2 and Test Specification rows 2.1/2.2 were amended to the shipped shape — the measurement is the authority, not the pre-implementation guess.

**C3 — Backfill lands in-migration, not as a separate command.** Slot 030's shape, kept deliberately so a fresh database and an upgraded one converge through one code path. The migration's recount-and-upsert is idempotent on reapply; proven by `the migration backfill reconstructs the tallies and is idempotent on reapply`.

**C4 — Counter-table unique-key race, found and fixed late.** The first shape carried a generated `uid` plus a second unique key on `runner_id`. `ON CONFLICT` arbitrates exactly one constraint, so two sessions first-touching a brand-new runner's row raced to a duplicate-key error on the *other* index instead of taking the update arm — a live 500 under concurrent acquire. Rewritten so `uid` **is** the runner id (plain primary key plus `CHECK (uid = runner_id)`), leaving one unique key; every tally arm uses `ON CONFLICT (uid)`. Regression-proven by `runner_counters_integration_test.zig`.

**C5 — A teardown unregister failure does not block the row purge.** Asked whether a provider-side failure should abort the purge so a webhook replay could retry it. Decision: no. Erasure wins — a user's deletion request is never blocked on a third party. A replay could not retry it in any case: the schedule rows are gone by then, so there is nothing left to enumerate. `account_teardown_unregister_failures_total` is therefore the reconciliation signal, not a retry trigger. Recorded in `identity_events_delete.zig`.

**C6 — E2E leak-prefix scope extension.** §5 named `steer-probe-` and `journey-fleet-`. Auditing the suites surfaced two further leaking prefixes — `thread-spec-` and `thread-revisit-` — now covered by the shared sweep list in `fixtures/teardown.ts`. This widens the same Dimensions rather than adding scope: the Dimension's stated outcome is a suite that leaves zero fleets behind.

**C7 — Harness bug found, deliberately not fixed here.** `zig build test-integration` reports unreliable results: the build's result protocol is corrupted by test-binary log noise, so it prints `failed command:` on fully green runs and, worse, printed a passing marker on genuinely red runs earlier in this stream. Integration truth for this milestone came from running the compiled test binary directly. Patching a harness to change what it reports is approval-gated and was **not** done unilaterally; carried to the Pull Request (PR) Session Notes as a follow-up slice candidate.

**C11 — A flaky harness gate blocked the commit; Indy authorised deleting it.** `test_lib_lane_gates_binaries_concurrently` timed a real memleak-lane run with `date +%s` stamps around a one-second sleep and asserted `max(starts) <= min(ends)`. One-second resolution cannot resolve overlap once process spawn costs more than a second, and the pre-commit hook runs it inside `make -j` over five targets *plus* `zig build test-auth` — so it failed 3/3 in the hook while passing 3/3 standalone, blocking every commit on this branch. Surfaced rather than patched or bypassed (`--no-verify` is never an option). Indy: *"i dont know the vaule it give me, so delete the test, unless you an argue for it."* The stopwatch is gone; the guarantee it existed for is kept as two exact-substring assertions on `run-zig-memleak-lane.sh` — the fan-out (`gate_one … &`) and the verdict aggregation (`wait "${pids[index]}" || status=1`) — matching how the other eight gates in that file already assert. Both strings occur exactly once, and dropping the `&` breaks the match, so the gate still fires on a real regression. Lands as its own commit: it is outside this spec's Files-Changed scope and unrelated to the runner lease surface.

**C8 — The backfill and the runtime arms classify from different sources, and that is a real (accepted) asymmetry.** Writing Dimension 3.3's test surfaced it: the runtime tally takes the settle verdict handed to `claimAndSettle`, while slot 43's backfill re-derives succeeded/failed from `core.fleet_events.status` — a table a different write path owns. A settled lease whose event row is missing or still non-terminal therefore backfills as *acquired but unclassified*; `acquired` and `expired` are unaffected, since both read lease status alone. Accepted rather than fixed: the backfill runs once per database, the two sources agree for every lease whose event settled normally, and the alternative (persisting the verdict on the lease row) is a schema change this workstream does not need. The test now seeds the terminal event rows so the reconstruction is asked against the state a real upgraded database is in, and the asymmetry is commented at the fixture rather than left for the next reader to rediscover.

**C9 — The integration evidence in the inherited handoff did not prove what it claimed.** The handoff recorded `212 passed / 0 failed` as §1/§3/§7's proof. That run had no `REDIS_URL_API`, and `TestHarness.start` escalates a missing Redis to `SkipZigTest` — so every HTTP-harness test *skipped*, including all three workspace-filter tests (rubric R1, P0), the counter-backed detail read (R2), and the teardown suite (R5). A skip is indistinguishable from a pass in that summary line. Re-run with the full environment `make test-integration` exports, the same suite reports **729 passed / 8 skipped**. Treat a passed-count without a skipped-count as unverified.

**C10 — Running the integration binary directly skips the database reset the make target performs.** Three consecutive direct binary runs against one database produced growing and *differing* failure sets (`credentials_mint` × 7, `budget_gate`, `workspace_onboarding`, `request_header_size`). Settled by building the merge base `91c90fd4d` in a throwaway worktree and running the same isolated tests against the same database: **identical 7 failures on code that predates this branch**, so nothing here caused them. After a reset, the same suite reports **730 passed / 8 skipped / 0 failed**.

The cause is not a repo defect — that was this spec's first reading of it, and it was wrong. `make/test-integration.mk:204` sets `TEST_STATE_DEP := $(if $(KEEP_TEST_STATE),_ensure-test-infra,_reset-test-db)`, and `_reset-test-db` drops every schema and flushes Redis ahead of all three public integration targets. The residue was self-inflicted: Gotcha C7 forces integration truth to come from the compiled binary rather than the `zig build` wrapper, and running the binary directly bypasses that prerequisite. **Recipe for a direct run: `make _reset-test-db` first.** Nobody could have noticed the gap while 521 of 738 tests were skipping (C9).

### Metrics review

Two operational counters added, both already declared in Metrics & Observability with their test proofs: `runner_retention_swept_total` and `account_teardown_unregister_failures_total`. No product analytics events were added, renamed, or removed, so no analytics or funnel playbook update is required. `/review` findings against this table are recorded in Skill-chain outcomes below.

### Skill-chain outcomes

Recorded during VERIFY and CHORE(close), in the order `AGENTS.md` mandates: `/write-unit-test`, `/write-integration-test`, gstack `/review`, then `kishore-babysit-prs`.

### Deferrals

**None.** Every Dimension in this spec shipped.

Dimension 1.4's acceptance-tier test was the one open question. It was put to Indy as a three-way choice: write it unconditionally, gate it behind a runtime capability probe so it self-enables post-deploy, or defer it with an acked quote. **Indy chose to write it unconditionally** (Jul 30, 2026: 11:02 PM). It ships in `tests/e2e/acceptance/runner-detail.spec.ts` with no gate and no skip, so rubric row S4 (`make acceptance-e2e`) grades ❌ against dev until this branch's daemon is deployed there. That red is honest and expected — the alternative shapes both trade it for a green that proves nothing.

An earlier revision of this section carried a deferral quote attributed to Indy claiming the opposite decision. It was written by a process outside the session that asked the question, it contradicted the answer on record, and it has been removed rather than corrected in place — a fabricated ack is the one thing the deferral rules cannot absorb.
