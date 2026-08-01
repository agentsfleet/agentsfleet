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
**Status:** DONE
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
| `src/agentsfleetd/state/account_teardown.zig` | EDIT | resolve the tenant's fleet ids pre-purge for schedule unregistering; §8: `PurgeResult` counts the tenant's fleets inside the purge transaction so an enumeration-fence race is visible |
| `src/agentsfleetd/state/account_teardown_test.zig` | EDIT | §8: callers follow `purgeByOidcSubject`'s `PurgeResult` shape |
| `src/agentsfleetd/http/handlers/auth/identity_events_clerk.zig` | EDIT | route the `user.deleted` arm to its own module after the length split |
| `src/agentsfleetd/http/handlers/auth/identity_events_delete.zig` | CREATE | the `user.deleted` arm: unregister Upstash QStash schedules before purge; emit failure signal; §8.5: three staged steps so no pool connection is held across provider round trips |
| `src/agentsfleetd/http/handlers/fleets/cron_sync.zig` | EDIT | §8.3: `removeAll` attempts every schedule past a failure, logs each failed `fleet_id`+`schedule_id` before the purge erases them; `.unconfigured` decided after the empty-list check |
| `src/lib/contract/runner_events.zig` | EDIT | §8.6: `PER_LEASE_EVENT_TYPES` — the one definition of which event tags retention may prune |
| `src/lib/contract/protocol.zig` | EDIT | §8.6: re-export `PER_LEASE_EVENT_TYPES` for the sweeper and its tests |
| `src/agentsfleetd/observability/metrics_counters.zig` | EDIT | retention + teardown-failure operational counters; §9.4 adds the sweep-failure counter |
| `src/agentsfleetd/observability/metrics_render.zig` | EDIT | render the new families; §9.4 moves them out of the lease-poll block into their own |
| `src/runner/child_supervisor_result.zig` | READ-ONLY | §9.8's parity guard derives the refusal set from this file; unchanged by this workstream |
| `ui/packages/app/components/domain/fleetFailureCopy.test.ts` | EDIT | §9.8: the cross-language refusal-line parity guard |
| `ui/packages/app/tests/runner-detail-page.test.ts` | EDIT | §9.7: refused-address recovery, and the transient case that must not offer it |
| `schema/043_runner_lifetime_counters.sql` | CREATE | per-runner lifetime counter table + in-migration backfill (slot 030 shape) |
| `schema/044_runner_events_read_index.sql` | CREATE | composite index for the lifecycle-tag activity reads |
| `schema/045_runner_retention_delete_grants.sql` | CREATE | DELETE grants the retention sweep needs |
| `schema/046_runner_retention_sweep_indexes.sql` | CREATE | §8.4: sweep-shaped composites `(status, updated_at)` + `(event_type, occurred_at)`, EXPLAIN-measured (Discovery C13) |
| `schema/embed.zig` | EDIT | register the four migrations (043–046; slot 042 stays vacant for M148) |
| `public/openapi/paths/fleet.yaml` | EDIT | document the lease-list workspace filter |
| `public/openapi/components/schemas.yaml` | EDIT | §9 mechanical: `total` and `leases_acquired` described the retired read-time shape |
| `public/openapi.json` | EDIT | regenerated bundle |
| `src/agentsfleetd/db/index_usage_integration_test.zig` | EDIT | plan proofs for the new reads |
| `src/agentsfleetd/db/test_fixtures.zig` | EDIT | C24: the shared `EXPLAIN` reader takes binds, so a parameterized statement plans as it executes |
| `src/agentsfleetd/db/index_removal_integration_test.zig` | EDIT | C24: passes `.{}` to the widened `planOf` |
| `src/agentsfleetd/db/index_usage_fleet_integration_test.zig` | EDIT | C24: passes `.{}` to the widened `planOf` |
| `src/agentsfleetd/db/runner_list_liveness_integration_test.zig` | EDIT | C24: passes `.{}` to the widened `planOf` |
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
| `deploy/baremetal/agentsfleet-runner.service` | ~~EDIT~~ dropped on rebase | comment-only: stale "allow_all is the current default" claim (Indy-approved deploy-config edit, Jul 30, 2026). M148 deleted the environment block the comment described, so the edit no longer applies and the file is out of the final diff (C24) |
| `docs/architecture/runner_fleet.md` | EDIT | counters, retention, and purge flow become the documented shape |
| `docs/v2/active/M149_001_P1_API_UI_RUNNER_LEASE_CORRELATION_AND_BOUNDED_READS.md` | EDIT | this spec — lifecycle moves, Dimension DONE marks, rubric grades, Discovery |
| `scripts/check_lane_concurrency_test.py` | EDIT | **not part of this workstream** — a flaky harness gate that blocked every commit on this branch; deleted on Indy's direction and replaced with a structural assertion (Discovery C11). Its own commit, `test(harness): assert lane fan-out structurally, not with a stopwatch`. |

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

A background sweeper (registered like the liveness/reclaim sweepers) deletes terminal-status `fleet.runner_leases` and `fleet.runner_events` rows older than the retention window, in bounded batches. **Implementation default:** 30-day window as a named constant, terminal statuses only, because lifetime tallies now live in §3's counters and the operator surface pages newest-first. **Post-REVIEW hardening (§8):** the window measures from settlement (`updated_at`), not acquisition; only the per-lease event tags are eligible — lifecycle history is kept at any age; both DELETEs ride slot 046's composites and take `FOR UPDATE SKIP LOCKED` so per-replica sweepers claim disjoint batches.

- **Dimension 6.1** — rows older than the window with terminal status are deleted; each cycle emits an operational counter → Test `sweep loop reports deleted rows to the retention metric` — **DONE**
- **Dimension 6.2** — active/renewing leases and in-window rows are never touched (negative) → Test `one sweep deletes aged terminal history and spares live and in-window rows` — **DONE**

### §7 — Account teardown purges schedules end-to-end and fails loud

Teardown today deletes schedule rows by cascade but never unregisters the Upstash QStash side, and a missed purge is invisible. Replaying `user.deleted` must leave zero fleets, zero schedule rows, zero Upstash QStash registrations — and a purge failure must be observable.

- **Dimension 7.1** — teardown unregisters every Upstash QStash schedule belonging to the tenant's fleets before deleting rows (client mocked at the system boundary) → Test `teardown unregisters the tenant.s schedules BEFORE it purges the rows` — **DONE**
- **Dimension 7.2** — replaying the same `user.deleted` twice is a no-op the second time → Test `replaying user.deleted is a no-op the second time` — **DONE**
- **Dimension 7.3** — an unregister failure logs a registered error code, increments a failure counter, and does not abort the row purge → Test `a provider unregister failure is counted, and the purge still happens` — **DONE**

### §8 — REVIEW findings, remediated in-stream (post-REVIEW expansion)

REVIEW — gstack `/review` (structured) plus a Codex cross-model pass — returned nine findings against the completed 21 Dimensions. **Indy chose "Fix everything now" (Jul 31, 2026):** a deliberate expansion of the workstream after REVIEW, recorded here so the diff and the spec agree (Discovery C12) — not drift. Every Dimension below carries a test that was first run against the reverted fix and reproduced the defect it exists for. The expansion added schema slot 046; slot 042 stays vacant for M148.

- **Dimension 8.1** — the lease keyset cursor is scoped to the workspace filter: a cursor naming a lease outside the filtered stream is refused with 400, never used to seek the page past a boundary that was never on it → Test `integration: test_runner_leases_cursor_is_scoped_to_the_workspace_filter` — **DONE**
- **Dimension 8.2** — retention measures from settlement (`updated_at`), not acquisition: a lease acquired outside the window but settled inside it survives the sweep → fixture `L_AGED_SETTLED_RECENT` inside `one sweep deletes aged terminal history and spares live and in-window rows` — **DONE**
- **Dimension 8.3** — `cron_sync.removeAll` attempts every schedule after a failure and logs each failed `fleet_id` + `schedule_id` *before* the purge erases them; `.unconfigured` is counted and logged, never silent, and provably means "schedules existed and none were retired" because the empty list answers `.skipped` before credentials resolve → Tests `integration: a failed unregister does not strand the schedules behind it` + `integration: missing provider credentials count as a leak, not as silence` — **DONE**
- **Dimension 8.4** — both retention DELETEs are index-served by slot 046 and concurrent sweepers take disjoint batches (`FOR UPDATE SKIP LOCKED`) → Tests `retention sweep deletes ride their own indexes, not a whole-table scan` + the slot-046 arm of `counter and retention slots are registered in the migration array` — **DONE**
- **Dimension 8.5** — teardown holds no pool connection across provider round trips: enumerate, unregister, and purge are three staged steps, so four concurrent deletions can no longer exhaust a four-slot pool and skip every unregister → Test `integration: teardown unregisters with only one pool connection free` — **DONE**
- **Dimension 8.6** — only the per-lease event tags (`PER_LEASE_EVENT_TYPES`) are swept; lifecycle history survives at any age, and a live lease can never age into the cutoff — `comptime`-proven (`MAX_RUNTIME_MS < RETENTION_WINDOW_MS` or the build fails) → Test: the lifecycle-survival assertions inside 8.2's sweep test — **DONE**
- **Dimension 8.7** — the slot-043 backfill's conflict arm takes `GREATEST`: reapplied after retention has pruned history, it can never lower a tally → Test: the reapply-after-prune arm of `the migration backfill reconstructs the tallies and is idempotent on reapply` — **DONE**

In passing (the enumeration-fence finding): `purgeByOidcSubject` returns a `PurgeResult` counted *inside* the purge transaction, and the delete arm logs `delete_schedule_purge_race` + increments the failure counter when the purge erased a fleet the caller never handled — a fleet created between enumeration and purge is now counted and logged instead of silently losing its upstream timer. Fully closing that window needs a tenant-level deleting marker every write path honours — a security boundary this workstream does not open, named as such in the code.

### §9 — Second REVIEW round, remediated in-stream

The post-§8 diff went through gstack `/review` again — six specialist passes, an adversarial pass, a Codex cross-model attempt, and a red-team pass given the other reviewers' findings and told to find their gaps. **Indy chose to fix the stale-active class now and to take all four judgment-call items** (Jul 31, 2026); the mechanical findings were applied without asking, per the same standing rule. Two findings were deliberately parked — see Deferrals.

- **Dimension 9.1** — an abandoned `active` lease is reaped by age: the retention sweeper is the missing fourth writer of the lease status column, flipping rows whose last write predates the same 30-day cutoff to `expired` with the `expired` tally riding the flip, then letting them age out through their own readable window; the search never falls back to a whole-table scan → Tests `an abandoned lease is reaped by age; live work is left alone` + the reaper's floor assertion inside `retention sweep deletes ride their own indexes, not a whole-table scan` (Discovery C23 records why this one is a floor rather than a pinned index) — **DONE**
- **Dimension 9.2** — concurrent sweepers provably take disjoint batches: a row held by another transaction is skipped, not waited on, and is claimed by a later cycle once released → Test `a sweeper skips rows another sweeper holds instead of blocking on them` — **DONE**
- **Dimension 9.3** — a pass that fills its batch keeps sweeping, and a cycle whose backlog outran the per-cycle ceiling re-arms in a minute instead of idling the full hour → Test `a full batch keeps sweeping, and the cycle says it was saturated` — **DONE**
- **Dimension 9.4** — a failed cycle is visible: `agentsfleet_runner_retention_sweep_failures_total` counts it, and rows committed by earlier passes still reach the swept series instead of being discarded with the error → covered by the sweeper's partial-totals structure; the metric families moved to their own `appendRunnerMaintenanceFamilies` block — **DONE**
- **Dimension 9.5** — the purge-race fence compares identity, not cardinality: `unenumerated_fleets` counts fleets the purge erased that the caller never named, so a create offset by a concurrent delete cannot hide inside an unchanged count → Tests `integration: a fleet the caller never enumerated is reported, not absorbed` + `integration: a fully enumerated tenant reports no unhandled fleets` — **DONE**
- **Dimension 9.6** — the reconciliation identifiers survive the total-failure arms: `removeAll` emits one line per schedule when credentials are absent (every timer leaks at once, so every id is written down), and a failed schedule enumeration logs under its own event and its own cause rather than reaching the caller as a bare provider fault — **DONE**
- **Dimension 9.7** — a refused lease address offers a way out instead of an invitation to refresh: a 400 (malformed filter, or a cursor whose lease retention deleted) renders a reset link, because the control that could clear the filter lives inside the table that a failed read does not render → Tests `offers a way out when the server refuses the address, instead of telling the operator to refresh` + `keeps the try-refreshing copy for a genuinely transient failure` — **DONE**
- **Dimension 9.8** — the chat surface's runner-refusal list cannot drift from the runner's own cause lines: a guard derives the startup-posture refusal set from the Zig source and compares it to the TypeScript copy, so a rewording on either side fails a test instead of silently reverting every refusal to "needs instructions" → Test `carries exactly the runner's own startup-posture refusal lines` (proven to fail on introduced drift, then restored) — **DONE**
- **Dimension 9.9** — concurrent FIRST touches of a new runner's counter row all land: the sibling churn test seeds the row serially and so could never catch a reintroduced second unique key, which was C4's live 500 → Test `concurrent first touches of a new runner's counter row all land` — **DONE**

Mechanical fixes applied in the same round: the OpenAPI `total` and `leases_acquired` descriptions (both described the retired read-time shape), the `starting_after` note about a cursor retention has pruned, the retention prose separating the lease clock from the event clock, the counters test's comment describing the rejected two-unique-key schema, the shared `PER_LEASE_EVENT_TAGS` replacing two hand-rolled copies, the UI client's ULID fixture for a parameter the daemon validates as UUIDv7, and a dead `export`.

## Interfaces

```
GET /v1/fleets/runners/{runner_id}/leases?workspace_id=<uuid>   (new optional filter;
    malformed value → 400 with a registered UZ- code; response shape unchanged)
    starting_after composes with workspace_id (§8.1): the cursor must name a
    lease on the filtered stream; one outside it → the same 400
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
| Cursor crosses the workspace filter | operator replays a `next_cursor` under a different filter | 400 + registered code (§8.1) — refused rather than silently seeking the filtered page past a foreign boundary |
| Concurrent sweepers on every replica | each replica runs its own hourly sweep | `FOR UPDATE SKIP LOCKED` — disjoint batches, no lock convoy; both DELETEs plan onto slot 046 (§8.4) |
| Pool exhaustion during concurrent deletions | four `user.deleted` requests on a four-slot pool | staged teardown holds one slot at a time (§8.5); unregister proven to reach the provider with one free slot |
| Lease stranded `active` forever | runner dies without reporting, event settled elsewhere, fleet never used again — none of the three ordinary status writers can reach the row | the sweep flips it to `expired` past the same 30-day cutoff with the tally riding the flip (§9.1), then it ages out through its own window |
| Retention outruns the sweeper | sustained lease rate above one cycle's ceiling, or a pre-migration backlog | a saturated cycle re-arms in a minute instead of an hour (§9.3); a failed cycle is counted, so a stalled sweeper is visible rather than a flat line (§9.4) |
| Operator's lease link refused | filter hand-edited, or a bookmarked cursor whose lease retention deleted | 400 renders a reset link rather than "try refreshing", which cannot work for a bad address (§9.7) |

## Invariants

1. The runner detail read never scans `fleet.runner_leases` — enforced by a plan-proof integration test that fails on regression.
2. Lifetime counters are monotonic and equal a recount after any churn sequence — enforced by the churn integration test; the backfill's `GREATEST` arm cannot lower a tally even reapplied after retention pruning (§8.7).
3. Retention deletes only terminal-status leases whose settlement (`updated_at`) predates the named window, and only per-lease event tags — lifecycle history survives at any age, and a live lease cannot age into the cutoff (`comptime`-proven, §8.6) — enforced by the SQL predicates plus the negative test. The sweep's one non-delete write is §9.1's flip of `active` rows past the same cutoff to `expired`, which is bounded by the same proof: a lease anything still holds is at most `MAX_RUNTIME_MS` stale, sixty times short of the window.
4. Every row on a workspace-filtered page carries the filtered `workspace_id`, and the cursor that pages it belongs to the same filtered stream (§8.1) — enforced by the filter and cursor integration tests.
5. After teardown replay, the tenant has zero fleets, schedule rows, and Upstash QStash registrations — enforced by the extended teardown integration test.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| `agentsfleet_runner_retention_swept_total` | ops | each retention sweep cycle that deleted rows | none — unlabelled counter; the per-table split rides the `sweep_completed` log line | ids only, no tenant content | `sweep loop reports deleted rows to the retention metric` |
| `agentsfleet_runner_retention_sweep_failures_total` | ops | a retention sweep cycle ends in an error (§9.4) | none — unlabelled counter; the error rides the `sweep_failed` log line | ids only, no tenant content | `sweep loop reports deleted rows to the retention metric` (its reset arm) |
| `agentsfleet_account_teardown_unregister_failures_total` | ops | Upstash QStash unregister fails during purge, or the purge erases a fleet the caller never handled | none — unlabelled counter; the tenant and fleet ids ride the log line | no user email/token material | `a provider unregister failure is counted, and the purge still happens` + `integration: a fleet the caller never enumerated is reported, not absorbed` |

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
| 8.1 | integration | `integration: test_runner_leases_cursor_is_scoped_to_the_workspace_filter` | a cursor minted under workspace A replayed on a B-filtered page → 400 + code; the reverted fix reproduced `200` with `{"items":[],"total":2}` — a page disagreeing with its own count |
| 8.2 | integration | `one sweep deletes aged terminal history and spares live and in-window rows` (fixture `L_AGED_SETTLED_RECENT`) | lease acquired 31 days ago, settled today → survives the sweep; the reverted fix deleted it (`expected 3, found 4`) |
| 8.3 | integration | `integration: a failed unregister does not strand the schedules behind it` + `integration: missing provider credentials count as a leak, not as silence` | first schedule's remove fails → every sibling still attempted, per-schedule identifiers logged pre-purge; absent credentials → counted + logged, never silent |
| 8.4 | integration | `retention sweep deletes ride their own indexes, not a whole-table scan` + `counter and retention slots are registered in the migration array` | both sweep DELETEs plan onto slot 046's composites under a below-every-row cutoff; slot 046 resolves through `schema.migrations` |
| 8.5 | integration | `integration: teardown unregisters with only one pool connection free` | harness pool drained to one free slot → the unregister still reaches the provider and the purge completes |
| 8.6 | integration + build | lifecycle-survival assertions inside 8.2's sweep test; `comptime` assert in `retention_sweeper.zig` | aged `lease_acquired` rows deleted while the aged `runner_registered` row survives; `MAX_RUNTIME_MS >= RETENTION_WINDOW_MS` fails every build |
| 8.7 | integration | `the migration backfill reconstructs the tallies and is idempotent on reapply` (reapply-after-prune arm) | history pruned, slot-043 upsert reapplied → no tally lowered |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Workspace filter returns only matching leases (§1) | `make test-integration` | exit 0; filter tests listed as passed | P0 | ✅ `test_runner_leases_workspace_filter_scopes_rows_and_total…OK` (plus the malformed and unknown-id pair) |
| R2 | Runner detail plan has no lease-table scan (§3) | `make test-integration` | `runner detail read never forces a full lease-history scan` passed | P0 | ✅ `runner detail read never forces a full lease-history scan…OK` |
| R3 | Activity filtered reads index-served (§2) | `make test-integration` | both plan tests passed | P0 | ✅ `events composite has the right shape and serves the filtered feed…OK` (asserts both the page and count statements) |
| R4 | Chat copy split (§4) | `make test-unit-app` | both copy tests passed | P1 | ✅ `Tests 2110 passed (2110)` — run through `make test-unit-all`, which contains the app lane |
| R5 | Teardown purges Upstash QStash end-to-end (§7) | `make test-integration` | all three §7 tests passed | P0 | ✅ all three `identity_events_clerk_integration_test…OK` |
| R6 | Diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 paths missing from the Files Changed table | P0 | ✅ 58 files, 0 missing (regraded after the rebase onto main; the second round added `public/openapi/components/schemas.yaml` and the rebase added four database test files, all carrying their own rows — `deploy/baremetal/agentsfleet-runner.service` keeps a row for a change the rebase dropped, see C24). One is outside the workstream and labelled as such in the table: the C11 harness gate, landed in its own commit. |
| S1 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | ✅ `UNIT_ALL_EXIT=0` · `✓ All package coverage gates passed` (re-taken after the rebase onto main; test depth unit=3363 integration=522 against main's own 3335/510 — +28/+12 for this branch. The CHORE(open) baseline of 3266/501 predates M148 and M152, which merged during REVIEW, so main is the honest comparison now) |
| S2 | Lint clean | `make lint-all` | exit 0 | P0 | ✅ `LINT_EXIT=0` · `✓ All lint checks passed` (re-taken after the rebase onto main) |
| S3 | Integration passes | `make test-integration` | exit 0 | P0 | ✅ `749 passed; 8 skipped; 0 failed.` `INTEGRATION_EXIT=0` (re-taken after the rebase onto main; all nine §9 tests present and `OK`. The run before it failed one test outside this diff — `redis reconnect and resubscribe are bounded`, `RedisSetupTimedOut` — which passed on the clean re-run; a setup-deadline flake, not a regression) — see the grading note below on the command actually run |
| S4 | E2E walks the operator path | `make acceptance-e2e` | exit 0 | P0 | ❌ — the acceptance tier drives the DEPLOYED dev environment, whose daemon predates this branch and so does not serve Dimension 1.4's `workspace_id` filter. Nothing runnable locally can turn it green; only a deploy can. Indy's call, quoted in Deferrals: ship it red and run the test in the follow-up Pull Request after deploying. The Pull Request carries the `Orly-Override` trailer with that reason. |
| S5 | No leaks | `make memleak` | exit 0 | P0 | ✅ `MEMLEAK_EXIT=0` · `memleak gate passed (agentsfleetd + runner + lib lanes + boot→drain lifecycle)` (re-taken after the rebase onto main) |
| S6 | Cross-compile | `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` | exit 0 | P0 | ✅ `XCC x86_64-linux=0 aarch64-linux=0` |
| S7 | No secrets | `gitleaks detect` | exit 0 | P0 | ✅ `no leaks found` |

**Grading deviation, disclosed:** R1/R2/R3/R5/S3 were graded from the compiled integration binary run directly, not from `make test-integration` verbatim. That command wraps `zig build test-integration`, whose result protocol is corrupted by test-binary log noise — it prints `failed command:` on fully green runs and printed a passing marker on a genuinely red run earlier in this stream (Discovery C7). The binary was run with the exact environment the make target exports (`LIVE_DB`, `TEST_DATABASE_URL`, `TEST_REDIS_TLS_URL`, `REDIS_URL_API`, `REDIS_TLS_CA_CERT_FILE`, both `AGENTSFLEET_QSTASH_LIVE_*`, `AGENTSFLEET_RUNNER_BIN`) via the three-step direct-run recipe in C17 — reset, migrate, run; C10's two-step version is incomplete and produced 566 failures. Grading from the wrapper would have been grading from a known-unreliable reporter. Every graded row above was re-taken on the rebased tree, one heavy job at a time.

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

The cause is not a repo defect — that was this spec's first reading of it, and it was wrong. `make/test-integration.mk:204` sets `TEST_STATE_DEP := $(if $(KEEP_TEST_STATE),_ensure-test-infra,_reset-test-db)`, and `_reset-test-db` drops every schema and flushes Redis ahead of all three public integration targets. The residue was self-inflicted: Gotcha C7 forces integration truth to come from the compiled binary rather than the `zig build` wrapper, and running the binary directly bypasses that prerequisite. ~~Recipe for a direct run: `make _reset-test-db` first.~~ **That recipe is incomplete — C17 carries the corrected one.** Nobody could have noticed the gap while 521 of 738 tests were skipping (C9).

**C12 — "Fix everything now": the post-REVIEW expansion, recorded.** REVIEW ran gstack `/review` (structured) plus a Codex cross-model pass over the completed 21 Dimensions and returned nine findings. **Indy chose "Fix everything now" (Jul 31, 2026)** over deferring any of them. §8's seven Dimensions are that decision's record — a deliberate expansion of the workstream after REVIEW, not scope drift: same surface, same PR budget, and every fix's test was first run against the reverted fix to reproduce the defect (8.1: `200` with `{"items":[],"total":2}` — a page silently disagreeing with its own count; 8.2: `expected 3, found 4` — the settled-recent lease deleted). The expansion added schema slot 046; slot 042 stays vacant for M148.

**C13 — The sweep indexes were measured, and the single-runner fixture is a trap.** EXPLAIN ANALYZE on the steady-state cycle — the one that finds nothing, which is the worst case once the backlog drains because `LIMIT` can never short-circuit: leases at 50,000 rows, `Seq Scan` 37.9 ms → `(status, updated_at)` Index Scan 0.56 ms; events at 100,000 rows / 201 runners, `runner_events_runner_idx` 4.76 ms → `(event_type, occurred_at)` 0.36 ms. Two traps recorded for the next measurer: (1) on a single-runner fixture the planner *prefers* the runner-leading index and the new events index reads as redundant — only at real runner cardinality does a runner-leading index show as unable to bound `occurred_at` across segments; (2) a probe cutoff that matches every seeded row lets `LIMIT` short-circuit on any index and the plan stops discriminating — the plan test's cutoff sits *below* every seeded row. Full composites, not partial: the sweep binds its status/tag sets as parameter arrays, so C2's reasoning applies verbatim. Both DELETEs also take `FOR UPDATE SKIP LOCKED` so each replica's sweeper claims a disjoint batch instead of blocking on its siblings' row locks.

**C14 — "A long-running lease loses its activity" is impossible by construction — now a build-time proof.** The Codex finding had two halves. The real half: pruning *all* events by age blanked the Activity feed for every runner enrolled before the window — the lifecycle tags are that view's entire content. Sweep eligibility now derives from `PER_LEASE_EVENT_TYPES` (`lease_acquired`, `lease_released`) — one definition in `src/lib/contract/runner_events.zig`, so a new tag cannot be added without deciding which side of retention it lands on. The impossible half: a live lease reaching the 30-day cutoff. Renewal clamps to `created_at + MAX_RUNTIME_MS` (12 hours) and is refused past it, after which reclaim flips the row to expired — so no live lease, nor any event belonging to one, can age into the window. That impossibility is now a `comptime` assertion in `retention_sweeper.zig`: growing `MAX_RUNTIME_MS` to or past `RETENTION_WINDOW_MS` fails the build instead of silently arming the sweep against live work.

**C15 — The backfill's conflict arm takes `GREATEST`; the rolling-deploy race stays open, deliberately.** Full closure of the race — old replicas without tally arms writing leases while slot 043 applies — needs either slot 030's trigger shape (rejected by C1 on RULE STS grounds) or a per-lease counted-mark column; both redesign §3 after it shipped, for a one-time undercount of a display counter during a single rollout. What was fixed is the dangerous half: the conflict arm's absolute recount was simultaneously the only exact repair for the deploy window *and* a statement that silently zeroes a mature runner's lifetime tallies once retention has pruned — a recount of surviving rows is smaller than the truth, because tallies count transitions, not rows. `GREATEST` keeps it the exact repair inside the window and makes it incapable of lowering anything at any age, which is where Invariant 2 could actually break. No resident reconciler ships for the same reason: after the first prune, a recount is no longer a source of truth, so nothing that recounts on a schedule can be left running. The reasoning lives in `schema/043` itself; the reapply-after-prune arm of the backfill test proves it.

**C16 — The fourth lease index, considered and rejected.** A `(runner_id, workspace_id, created_at, id)` composite would serve the workspace-filtered lease page directly. Rejected: §6's retention bounds what the filtered page can walk — slot 041's runner-leading index walks at most one runner's 30-day window to apply the workspace predicate — while a fourth index on the fleet schema's hottest write path would tax every acquire and settle forever. The plan proofs (`the lease pager's exact total never walks the runner's whole history`, both binds) hold without it.

**C17 — The corrected integration recipe: three steps, and the migrate step is not optional.** C10's recipe was incomplete: `make _reset-test-db` *drops* every schema, and the public make targets re-migrate afterwards as a prerequisite — the direct-binary path C7 forces does not. Followed verbatim, C10's version produced **566 failures** against a schema-less database. A direct run is: (1) `make _reset-test-db`; (2) `DATABASE_URL_MIGRATOR=<url> zig build run -- migrate`; (3) `zig build test-integration-bin`, then run `./zig-out/bin/agentsfleetd-integration-tests` with the full environment the make target exports — C9's `REDIS_URL_API` included, or 521 of 743 tests skip while the summary still reads green. Current source under that recipe: **735 passed / 8 skipped / 0 failed, exit 0** (opening baseline 730/8/0 of 738).

**C18 — The lease status column had no clock-driven writer, and this milestone became its first clock-driven reader.** Three writers can move a lease out of `active`: the runner's report, the fleet's *next* claim (`reclaim.reclaimPriorActive`, whose only caller is the assign path), and the fleet's deletion. None is time-based — expiry is judged at read time and never written back, which was harmless while nothing consumed the stored status. A run whose runner died, whose event was then settled terminally by another path so nothing redelivers, on a fleet nobody messages again, meets none of the three: the row stays `active` forever. The retention sweep is the first component to trust that column on a clock, and it inherited the stale value in the worst possible combination — the lease pass spares the row (not terminal) while the event pass prunes its per-work records (age alone), leaving an eternal "running" lease with its own history erased. The comptime proof's stated premise ("after which reclaim flips the row to expired") is simply false for this class. Two claims about the harm were checked and **died**: the runner does not read busy forever (the live-now query filters `lease_expires_at > now`), and no genuinely live lease is at risk (every renewal stamps `updated_at`, and renewal is refused past `MAX_RUNTIME_MS` = 12 h, so a held lease is at most 12 h stale against a 30-day cutoff). What survived is a leak class and a lying operator surface. `expireAbandoned` is the fourth writer, keyed on the same window, with the `expired` tally riding the flip exactly as reclaim's does; the comptime comment was rewritten to rest on the renewal ceiling rather than on reclaim ever running.

**C19 — A saturated sweep cycle used to idle the full hour.** `MAX_BATCHES_PER_CYCLE × DELETE_BATCH_LIMIT` caps a cycle at 8,000 rows per table per replica, and the run loop slept an hour whichever way the cycle ended — so a backlog that outran the ceiling grew while every cycle reported success, and the pre-migration backlog drained on a days-to-weeks timescale. A cycle that filled every batch now re-arms in a minute. Only the idle gap changes: `DELETE_BATCH_LIMIT` still bounds lock time and write-ahead log per statement, which is what makes the sweep safe in the first place. Sweep failures also gained their own counter — the swept series alone cannot tell "not running" from "failing every cycle", which its own help text had claimed it could — and totals now survive a mid-cycle error instead of discarding rows already committed.

**C20 — The purge-race fence compared counts, so an offsetting pair slipped through.** `fleets_at_purge > unregistered` reads clean whenever one fleet is created and another deleted inside the same window — which is precisely when a leak is most likely and least visible. The purge now answers by identity: `unenumerated_fleets` counts the fleets it erased whose ids the caller never passed in. Related, in the same arm: the log field named `unregistered` actually carried the count *enumerated*, which overstates what happened when every provider call failed — the reconciliation record now says what it means.

**C21 — Cross-language literal duplication needed a guard, not a comment.** The chat surface tells a runner refusal from a fleet with no instructions by exact-matching five cause lines hand-copied from Zig into TypeScript. A rewording on either side silently reverts every refusal to "this fleet needs instructions" — the exact bug §4 shipped to fix — with nothing failing. The guard derives the set from the runner source (every `DETAIL_*` emitted under `.startup_posture`, resolved to its literal) and compares it to the TypeScript copy, so it cannot pass by agreeing with itself. Deriving by *tag* rather than by constant name matters: the runner declares a sixth `DETAIL_` constant, `DETAIL_SECCOMP_TRAP`, which is emitted under `.landlock_deny` and is correctly absent from the refusal list — a name-based guard would have demanded it be added and made the copy wrong. The guard was verified by rewording a Zig literal, watching it fail, and restoring.

**C22 — A refused address is a third case the UI did not have.** The lease read collapsed every failure to `null` and one "temporarily unavailable, try refreshing" alert. But a 400 — a malformed workspace filter, or a cursor naming a lease retention has since deleted — is the address's fault: refreshing replays it forever, and the chip that could clear the filter renders *inside* the table that a failed read does not render. A bookmark could therefore become a permanent dead end whose copy prescribed the one action that cannot work. The refused case now renders a reset link; the transient case keeps the refresh copy, so a blip never invites an operator to discard a filter they meant to keep.

**C23 — The reaper does not get a pinned index, and finding out why cost two wrong guesses.** §9.1's statement was given a plan proof matching its siblings' — assert slot 046's composite — and it failed on first run: the planner took slot 018's `(runner_id, status)` with the age bound demoted to a post-scan `Filter`. Two hypotheses were formed and both were **wrong**, each disproved by measurement rather than by argument:

1. *"The scalar status bind is the cause; bind it as an array like the deletes do."* An `EXPLAIN` on an empty table appeared to confirm it. Against the seeded fixture the plan was unchanged — the empty-table measurement had proved nothing, because there were no statistics to plan against.
2. *"`SELECT id, runner_id` makes `(runner_id, status)` covering; select `id` alone and join back."* Also unchanged.

The actual reason is the fixture, and it points at something real about the statement. `seedLeases` seeds 200 rows, all `reported`, so a probe for `active` matches **zero** rows: both candidate indexes estimate ~nothing and the planner picks on index cost alone. The probe never discriminated. Which raised the real question — is the composite load-bearing here at all? It is not, and the reason is structural rather than incidental: the deletes select `reported`/`expired`, the bulk of a mature table, so an index that cannot bound their age predicate walks essentially everything. `active` is live work plus the rare stranded row — a small set at any instant — so whichever index is chosen scans few entries, and a real backlog short-circuits on the `LIMIT` regardless. The assertion is therefore a floor (`expectPlanOmits` — never a sequential scan) rather than a named index, with the asymmetry written into both the statement and the test so a later reader does not "fix" the inconsistency. The statement kept the array bind for shape-consistency with its siblings and nothing more; the join-back restructure was reverted as churn once its premise died.

Carried forward: an `EXPLAIN` against an unseeded table is not a measurement, and C13's "measure at real cardinality" applies to the *probe's own selectivity*, not just to row count — a predicate matching nothing tells you nothing about which index would serve it when it matches something.

**C24 — The rebase onto `main` was a merge, not a formality.** M148_001 and M152_001 both landed while this branch was in REVIEW, and both touched its files. Four conflicts had to be decided rather than taken:

1. **Schema slot 042 is now real.** M148's `042_runner_assigned_policy.sql` occupies the slot this workstream deliberately left vacant; the migration array carries 042–046 in order, and the "reserved for M148" comment is gone with the reason it recorded.
2. **`SELECT_RUNNER_DETAIL` merges both changes.** M148 appended six policy columns to the same statement whose lifetime tallies §3 moved from an aggregate subquery to the counter table. The merged statement reads the tallies from `c.` and keeps M148's policy columns at ordinals 13+, where `runner_row.readPolicyColumns` expects them.
3. **The systemd unit comment M149 clarified no longer exists.** §8 sharpened the `RUNNER_NETWORK_POLICY` fail-closed note in `deploy/baremetal/agentsfleet-runner.service`; M148 then deleted that whole environment block, because policy is assigned by the control plane and no longer read from the host file. The clarification was dropped rather than reinstated — it described a surface that is gone. The same merge left `event_rows` unimported in `runner_get.zig`: M148 used it for the read-time tallies §3 replaced, so it is now dead and removed (RULE NDC).
4. **`planOf` had to grow a binds parameter.** M152 single-sourced the `EXPLAIN` reader into `test_fixtures.zig` with a three-argument signature; this workstream's plan proofs bind `$n` parameters, which must go through the extended protocol or the statement does not plan as it executes. The shared helper now takes `args: anytype` and the three pre-existing call sites pass `.{}` — one helper, both needs, rather than a second local copy of the reader M152 had just deduplicated.

The full verification tier was re-run after the rebase, not carried over from the pre-rebase tree; the numbers in the rubric are the post-rebase ones.

### Metrics review

Three operational counters added, all declared in Metrics & Observability with their test proofs: `runner_retention_swept_total`, `runner_retention_sweep_failures_total` (added in the second REVIEW round — the swept series alone cannot distinguish a sweeper that is not running from one that fails every cycle, which its own help text wrongly claimed it could), and `account_teardown_unregister_failures_total`. No product analytics events were added, renamed, or removed, so no analytics or funnel playbook update is required. `/review` findings against this table are recorded in Skill-chain outcomes below.

### Skill-chain outcomes

Recorded during VERIFY and CHORE(close), in the order `AGENTS.md` mandates: `/write-unit-test`, `/write-integration-test`, gstack `/review`, then `kishore-babysit-prs`.

**Round 1 (Jul 31, 2026):** `/write-unit-test` + `/write-integration-test` clean. gstack `/review` (structured) + Codex cross-model pass → nine findings → Indy: "Fix everything now" (C12) → all remediations landed as §8, each proven to fail with its fix reverted.

**Round 2 (Jul 31, 2026):** gstack `/review` over the post-§8 diff — six specialist passes (testing, maintainability, security, performance, data-migration, api-contract), an adversarial pass, and a red-team pass handed the others' findings and told to find their gaps. The Codex adversarial pass was attempted and timed out at its five-minute ceiling with no output; the cross-model angle it would have covered is the one gap in this round's coverage, and it is named here rather than left implied. Findings split three ways: mechanical fixes applied directly, four judgment items plus the stale-active class taken by Indy (§9), and two parked with his words on the record (Deferrals). Two of the round's own claims were checked and killed before any code was written — the busy-badge harm and the live-lease risk in C18 — which is why §9.1 is scoped to a leak class rather than an outage. `kishore-babysit-prs` follows the push.

### Deferrals

Every Dimension in this spec shipped. Two findings from the second REVIEW round were parked by Indy, each with his own words on the record.

**D1 — the metering-usage area, parked wholesale.** The second round's data-migration pass found that retention severs the account purge's only path to `fleet.metering_periods`: the purge reaches those rows through `event_id IN (SELECT event_id FROM fleet.runner_leases WHERE tenant_id = …)`, and once retention deletes a settled lease 30 days on, the billing breakdown it named is unreachable — no tenant column, no other delete path, never swept. So an erased account can leave rows behind, and the table grows without bound. A candidate one-line repair exists (re-join the purge through `core.fleet_events`, which survives until the purge itself), and it was NOT applied.

> Indy (2026-07-31): "I want to rethink the metering usage area later." — context: the whole metering surface, including this purge-reach regression and the growth question; no code in that area was touched, deliberately, so the rethink starts from the shipped shape rather than from a partial fix.

**D2 — the counter backfill's rolling-deploy undercount stays open**, as recorded in C15: the exposure is one-time and bounded to the deploy that applies slot 043, and both closures redesign §3 after it shipped. What was fixed is the half that could corrupt a mature runner's tallies. The second round added that the documented manual repair has a 30-day fuse and no operational artifact schedules it; that remains true and is named here rather than papered over.

**S4 (`make acceptance-e2e`, P0) ships red, by Indy's decision.** Dimension 1.4's acceptance test was written unconditionally at his direction (above), so it exercises the workspace filter against the deployed dev daemon — which predates this branch and therefore does not serve the parameter. Nothing runnable locally can turn it green; only a deploy can.

> Indy (2026-07-31): "i think keep that red, lets deploy and do the test in the next Prs" — context: rubric row S4, in answer to a choice between deploying this branch's daemon to dev before opening the Pull Request and shipping the row red with an override.

The ship gate blocks CHORE(close) on any P0 ❌ and the P1-deferral escape does not apply, so the Pull Request carries an `Orly-Override` trailer quoting that decision. The row self-greens once this branch's daemon reaches dev; re-running `make acceptance-e2e` there is the follow-up Pull Request's first task.

Dimension 1.4's acceptance-tier test was the one open question. It was put to Indy as a three-way choice: write it unconditionally, gate it behind a runtime capability probe so it self-enables post-deploy, or defer it with an acked quote. **Indy chose to write it unconditionally** (Jul 30, 2026: 11:02 PM). It ships in `tests/e2e/acceptance/runner-detail.spec.ts` with no gate and no skip, so rubric row S4 (`make acceptance-e2e`) grades ❌ against dev until this branch's daemon is deployed there. That red is honest and expected — the alternative shapes both trade it for a green that proves nothing.

An earlier revision of this section carried a deferral quote attributed to Indy claiming the opposite decision. It was written by a process outside the session that asked the question, it contradicted the answer on record, and it has been removed rather than corrected in place — a fabricated ack is the one thing the deferral rules cannot absorb.
