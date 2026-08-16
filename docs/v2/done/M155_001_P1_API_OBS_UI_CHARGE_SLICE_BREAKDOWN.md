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

# M155_001: A Run charge on the Usage tab opens into the burn that produced it

**Prototype:** v2.0.0
**Milestone:** M155
**Workstream:** 001
**Date:** Aug 14, 2026
**Status:** PARKED — never started; parked in `docs/v2/done/` with no branch and no code. Re-activates once Indy has settled how time-series storage and atomic usage charges should work (see Parked below)
**Priority:** P1 — the Usage tab ships a Run charge that is the accumulated sum of every renewal slice, and the drill-down that used to explain it was removed with the table it read
**Categories:** API, OBS, UI
**Batch:** B1 — §1 writes what §2 reads and §3 renders; §4 is a measurement gating separate work
**Depends on:** M154_001 — it deleted `fleet.metering_periods` and the two routes that read it, which is the state this workstream answers
**Provenance:** LLM-drafted (Claude Opus 5, Aug 14, 2026), rewritten from the payload-offload framing after reading the emit path, the metric dimensions and the Usage tab on `main`
**Canonical architecture:** `docs/architecture/billing_and_provider_keys.md` · `docs/architecture/observability.md` · `docs/architecture/data_flow.md`

---

## Parked (Aug 15, 2026) — read this before reactivating

**Nothing here was ever started.** No branch, no worktree, no code, no `DONE`
marks. This file went straight from `pending/` to `done/` as a design record.
Read every Section below as a proposal, not as a claim about the repository.

**The problem it describes is still true on `main`.** A Run charge on the Usage
tab is the accumulated sum of every renewal slice, and the two routes that used
to open it are still pinned 404s in `router_test.zig`. A tenant disputing a
charge still stops at the total, and so does the operator answering them. That
cost is accepted for as long as this stays parked.

**What is undecided is the storage shape, not the need.** Fixed-width buckets
accumulated in place are one answer to a broader question Indy wants to settle
once: how this system stores time-series data at all, and how an atomic usage
charge relates to the series that explains it. `fleet.metering_periods` was the
first answer and M154 deleted it for unbounded growth; a bucket table decided
milestone-by-milestone risks being the second table retired rather than the last
one built. The decision belongs upstream of this workstream.

**Two questions this spec left open are inputs to that decision, not leftovers.**
The RULE LDC (Legacy-Design Consult) A/B/C consult in Discovery is unanswered —
re-introducing a per-event history table M154 deliberately removed needs Indy's
call before any edit. Bucket width is likewise unresolved, and it fixes both the
derived row bound and the resolution a tenant sees.

**§4 does not depend on any of that.** Measuring event-body storage against the
live dataset is a query and a recorded number, and it gates the object-storage
offload rather than this workstream. It can be lifted into its own workstream
whenever that offload becomes a live question.

---

## Overview

**Goal (testable):** A tenant opening a Run charge on the Usage tab sees that charge broken into time buckets whose credits sum exactly to the row's total, for any run up to the maximum runtime, with the number of stored buckets per event bounded by a compile-time constant.

**Problem:** Credits are metered incrementally. Every runner renewal prices the work done since the last renewal and debits it, and the settle prices the final slice — so a long run is charged hundreds of times. All of those debits accumulate into one `billing.usage_ledger` row per `(event_id, charge_type)` under `ON CONFLICT … DO UPDATE … + EXCLUDED`, which is what bounds the ledger and what the Usage tab renders. The tab is correct and already shows per-event cost, fleet, model, token totals and amount. What it cannot show is the shape of the burn inside one Run row: a single accumulated figure with one timestamp. The two routes that used to open it — `charges/{event}/metering-periods` and `charges/{event}/telemetry` — are now pinned 404s in `http/router_test.zig`, removed with the per-slice table M154 deleted for writing a row roughly every twenty seconds of every run. So a tenant looking at an unexpectedly large charge, and an operator answering their question, both stop at the total. A run that finished its real work early and then idle-looped to the runtime cap is indistinguishable from one that worked the whole time.

**Solution summary:** Accumulate each priced slice into a fixed-width time bucket keyed on the event, using the same accumulate-in-place upsert the ledger already uses. Bucket width is a named constant, so the stored rows per event are bounded by `maximum runtime ÷ bucket width` rather than by how often a runner renews — the growth that killed the old table is removed by construction, not by a sweeper. A read endpoint returns an event's buckets, and the Usage tab's Run row expands into them. The emit rides beside the credit metric that already fires per slice at each of the three charge sites, so the numbers cannot disagree with the money. Moving event bodies to object storage stays unbuilt behind a measurement (§4); the tenant usage rollup stays unbuilt on M154's reasoning, recorded in Out of Scope.

## PR Intent & comprehension handshake

- **PR title (eventual):** Open a Run charge into its per-bucket burn on the Usage tab
- **Intent (one sentence):** A tenant who thinks a charge is wrong can see where the credits went without asking anyone, and an operator answering that question reads the same data.
- **Handshake** — the implementing agent fills this at PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `src/agentsfleetd/fleet/renewal.zig` — `RENEW_METER_SQL` prices the slice inside its guard Common Table Expression (CTE) and returns only `charged_nanos`. §1's write rides this statement; its money arithmetic is M154's and must not change.
2. `src/agentsfleetd/fleet/renewal_settle.zig` — the terminal claim that prices the final slice. The second write site, and the one that closes an event's last bucket.
3. `src/agentsfleetd/fleet/service_renew.zig` — the post-commit emit site. `recordCreditConsumed` already fires once per successful metered slice here; §1's observability rides the same position, after the same commit.
4. `src/agentsfleetd/observability/otel_metrics_cardinality.zig` — states why event and workspace identity deliberately never reach a metric label. §1 must not weaken that; the bucket row is where identity lives.
5. `src/agentsfleetd/http/handlers/tenant_billing.zig` — the live charges endpoint the new read hangs off, including its paging and tenant-scoping posture.
6. `ui/packages/app/app/(dashboard)/settings/billing/components/BillingUsageTab.tsx` and its sibling `lib/charges.ts` — the table §3 expands and the formatting vocabulary the expansion reuses.
7. `schema/710_usage_ledger.sql` — the accumulate-in-place shape §1 mirrors, and the charge-type vocabulary the buckets share.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `schema/730_*.sql` | CREATE | The bounded per-event bucket table in the money layer, plus its read index |
| `schema/embed.zig` | EDIT | One migration entry; the version is the slot number |
| `src/agentsfleetd/fleet/renewal.zig` | EDIT | The priced slice accumulates into its bucket inside the existing fenced statement |
| `src/agentsfleetd/fleet/renewal_settle.zig` | EDIT | The final slice accumulates into its bucket inside the terminal claim |
| `src/agentsfleetd/fleet/service_renew.zig` | EDIT | Post-commit observability for a bucket write that failed or was skipped |
| `src/agentsfleetd/state/usage_bucket_store.zig` | CREATE | The read for one event's buckets, sibling to the telemetry store |
| `src/agentsfleetd/http/handlers/tenant_billing.zig` | EDIT | The new read hangs off the existing charges surface with the same tenant scoping |
| `src/agentsfleetd/http/router.zig`, `routes.zig` | EDIT | One route; the two M154-removed spellings stay 404 |
| `src/agentsfleetd/errors/error_registry.zig` | EDIT | A registered code for a charge whose buckets cannot be read |
| `ui/packages/app/lib/api/tenant_billing.ts` | EDIT | The typed fetcher for the new read |
| `ui/packages/app/app/(dashboard)/settings/billing/**` | EDIT | The Run row expands; formatting reuses `lib/charges.ts` |
| `public/openapi/paths/*.yaml`, `public/openapi.json` | EDIT | One new documented path |
| `docs/architecture/billing_and_provider_keys.md` | EDIT | Records where per-slice charge history lives and what bounds it |
| `~/Projects/docs/changelog.mdx` | EDIT | User-visible: a charge on the Usage tab opens |

## Applicable Rules

- **`~/Projects/dotfiles/docs/greptile-learnings/RULES.md`** — **UFS** (bucket width, retention bound and the derived maximum-rows constant are named constants, shared verbatim with the schema comment that depends on them), **NDC** (no payload-offload code until §4's measurement authorises it), **NLR** (M154-era comments in `renewal.zig`, `sql_budget_drain.zig` and `otel_metrics_cardinality.zig` that assert per-slice detail exists nowhere stop being true), **FLL** (the billing handler and the renewal statement both grow — split before the cap), **LDC** (this re-introduces a per-event history table M154 deleted; the A/B/C consult is recorded in Discovery before EXECUTE), **ERR** (the unreadable-buckets failure gets a declared `UZ-` code), **LOG** (the bucket-write failure is an emit surface), **ITF** (integration tests run against the real schema), **PUB** (the new store's surface), **UIS** and **DTK** (the expansion uses design-system primitives and token utilities), **TSC** / **TSJ** (the fetcher and component conventions).
- **`~/Projects/dotfiles/docs/SCHEMA_CONVENTIONS.md`** — a new `7xx` money-layer slot; no `ALTER`, no patch-only slot, no static strings in the schema.
- **`dispatch/write_zig.md`** — errdefer ladder on the new store, drain discipline on its query, cross-compile both linux targets.
- **`~/Projects/dotfiles/docs/REST_API_DESIGN_GUIDELINES.md`** — the new read is a sub-resource of an existing charge; tenant scoping lives inside the statement, matching the sibling endpoint.
- **`~/Projects/dotfiles/docs/LOGGING_STANDARD.md`** — scope, event naming and error-code embedding for the bucket-write failure.
- **`dispatch/write_ts_adhere_bun.md`** — the expansion is a user-interface surface; primitive substitution and token discipline apply.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | yes — new store, two edited statements, one handler | errdefer ladder on the store; `PgQuery` drain on the bucket read; cross-compile `x86_64-linux` and `aarch64-linux` |
| PUB / Struct-Shape | yes — the bucket store is a new surface | shape verdict per new `pub`; no `pub` without an in-tree consumer |
| File & Function Length (≤350/≤50/≤70) | yes — `tenant_billing.zig` and `renewal.zig` both grow | extract the bucket read to its own store before the cap, mirroring the telemetry store split |
| UFS (repeated/semantic literals) | yes — bucket width, maximum rows, bucket-count cap | named constants; the schema comment cites the constant rather than restating the number |
| SCHEMA GUARD | yes — a new table in the money layer | new `7xx` slot, one `embed.zig` entry, no `ALTER`, app-enforced vocabularies |
| UI Substitution / DESIGN TOKEN | yes — the Run row expands | design-system primitives and token utilities only; no raw markup, no arbitrary values |
| LOGGING / LIFECYCLE / ERROR REGISTRY | yes — new emit surface and new failure class | declared `UZ-` code for unreadable buckets; the write-failure log follows the standard |

## Prior-Art / Reference Implementations

- **Reference:** `schema/710_usage_ledger.sql` with `fleet/renewal.zig`'s ledger arm — the accumulate-in-place upsert that bounds a table by key rather than by write frequency. §1 is the same shape with a coarser key.
- **Reference:** `src/agentsfleetd/state/fleet_event_detail_store.zig` — the single-row read whose scoping predicate lives inside the statement rather than in the handler. The bucket store mirrors it.
- **Reference:** `src/agentsfleetd/fleet/credit_metric_reconciliation_integration_test.zig` — the existing proof that the three emit sites sum exactly to committed debits. §1's reconciliation test mirrors its arrangement, including the zero arms.
- **Reference:** `ui/packages/app/app/(dashboard)/settings/billing/lib/charges.ts` — the approved amount, model and timestamp vocabulary. The expansion reuses these rather than formatting money a second way.

## Sections (implementation slices)

### §1 — Every priced slice accumulates into a bounded bucket

A slice is priced in two places: the renewal guard and the terminal settle. Both already commit money in one fenced statement, and both already emit a credit sample afterwards. This adds one more arm to each statement, folding the slice's credits and token deltas into the bucket its window falls in, keyed on the event and the bucket start. Bucket start derives from a named width constant, so a run cannot produce more rows than maximum runtime divided by that width no matter how often its runner renews — which is precisely what the retired per-slice table could not promise. **Implementation default:** the bucket write rides inside the same statement as the debit, so a committed charge always has a bucket and a rolled-back one has neither.

- **Dimension 1.1** — a run with many renewals produces buckets whose credits sum exactly to the ledger's accumulated total for that event → Test `test_buckets_sum_to_ledger_total`
- **Dimension 1.2** — a run held at the maximum runtime produces no more rows than the derived maximum, whatever its renewal cadence → Test `test_bucket_rows_per_event_are_bounded`
- **Dimension 1.3** — a lost fence, a capped renewal and a replayed report each contribute no bucket credits, matching the credit metric's zero arms → Test `test_uncommitted_slices_write_no_buckets`
- **Dimension 1.4** — a bucket carries the event, its window bounds, credits and token deltas, so a reader needs no second query to describe it → Test `test_bucket_row_is_self_describing`

### §2 — An event's buckets read back through the billing surface

The charge list already answers "what did this event cost" with tenant scoping inside the statement. This adds the sub-resource that answers "how did it accrue", scoped identically so a sibling tenant's event is indistinguishable from one that does not exist. The two spellings M154 removed stay removed — resurrecting `metering-periods` would name a table that no longer exists and a granularity this deliberately does not store. **Implementation default:** buckets return in window order with no paging; the row count is bounded by §1, so a cursor would be machinery guarding a bound that already holds.

- **Dimension 2.1** — an event's buckets return in window order and their credits sum to the charge row's amount → Test `test_bucket_read_reconciles_with_charge_row`
- **Dimension 2.2** — another tenant's event returns the not-found answer, never a different status that discloses existence → Test `test_bucket_read_refuses_sibling_tenant`
- **Dimension 2.3** — an event with no buckets answers empty rather than failing, so an old charge predating this milestone renders → Test `test_bucket_read_is_empty_not_error`
- **Dimension 2.4** — the two M154-removed route spellings still resolve to nothing → Test `test_removed_metering_routes_stay_404`

### §3 — The Run row on the Usage tab opens

The Usage tab renders each charge with fleet, model, activity and amount, and a Run row's amount is the accumulated total. This makes that row expandable into its buckets, reusing the tab's existing amount and timestamp formatting so one charge is never rendered two ways. The expansion is the whole user-facing outcome of this workstream: the point is that a tenant answers their own question at the moment they have it. **Implementation default:** only Run rows expand — a receive charge is a single debit with no interior, so an affordance on it would promise detail that does not exist.

- **Dimension 3.1** — expanding a Run row shows its buckets with window, tokens and credits, and the displayed values sum to the row's amount → Test `test_usage_row_expands_to_buckets`
- **Dimension 3.2** — a receive row offers no expansion affordance → Test `test_receive_row_does_not_expand`
- **Dimension 3.3** — a Run row whose buckets fail to load keeps every other cell readable and states which part is unavailable → Test `test_bucket_load_failure_degrades_in_place`
- **Dimension 3.4** — the expansion formats credits and timestamps through the tab's existing helpers, not a second implementation → Test `test_expansion_reuses_charge_formatting`

### §4 — Event body offload is measured before any of it is built

M154 deferred moving event bodies to object storage on the grounds that they grow without bound in the primary datastore. The growth is real and the storage cost is unmeasured, while the move would put an external store on five production read paths — the single-event detail read, expired-lease reclaim, the connector reply path, and both repair-verification reads — and would make object-storage credentials mandatory for event ingest, where they are optional today. That trade needs a number. **Implementation default:** record the table's total and heap size and its row count against the live dataset, declare the budget the measurement is judged against in the same breath, and build nothing unless it is missed. RULE NDC forbids writing the offload speculatively.

- **Dimension 4.1** — event-body storage is measured against the live dataset and the number recorded with the query that produced it → Test `test_event_body_storage_is_measured`
- **Dimension 4.2** — no content-hash or object-key code exists in the tree unless 4.1 missed its declared budget → Test `test_no_speculative_offload_code`

## Interfaces

```
NEW      GET /v1/tenants/me/billing/charges/{event_id}/buckets
         Buckets for one Run charge, window order, no paging. Each carries the
         window bounds, credits and token deltas. Tenant scoping lives inside
         the statement; a sibling tenant's event answers not-found. An event
         with no buckets answers an empty collection.

UNCHANGED GET /v1/tenants/me/billing/charges
         Response shape is untouched. Rows stay one per (event_id, charge_type)
         and the Usage tab keeps rendering them exactly as it does now.

STILL 404 charges/{event_id}/metering-periods, charges/{event_id}/telemetry
         The M154-removed spellings are not restored. `router_test.zig` keeps
         pinning them, and this milestone adds no alias for either.

NEW ERROR Buckets unreadable for a charge — a registered UZ- code. The charge
         row itself still renders; only the expansion reports.
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Bucket write fails | The upsert arm errors inside the fenced statement | The whole statement rolls back: no debit, no bucket, and the renewal reports lost. Money and detail cannot diverge |
| Bucket read fails | Datastore unavailable at expansion | The charge row keeps every cell; the expansion states the registered code. The list itself never fails |
| Event has no buckets | A charge predating this milestone, or a run with no metered slice | Empty collection, rendered as "no breakdown recorded", never an error |
| Renewal cadence spikes | A runner renewing far more often than expected | Rows per event stay bounded by the derived maximum; extra renewals accumulate into existing buckets |
| Clock skew across renewals | A slice whose window start precedes the bucket it lands in | The bucket is derived from the committed charge instant the ledger already trusts, so skew cannot create a bucket outside the run's span |
| Sibling tenant probes an event | Enumeration attempt against the sub-resource | Not-found, identical to an unknown identifier — the scoping predicate is inside the statement |
| Offload measured as unnecessary | §4's number meets its budget | Nothing is built and the deferral is recorded, which is a valid outcome of §4 |

## Invariants

1. **Rows per event are bounded by a compile-time constant.** Enforced by a comptime assertion deriving the maximum from maximum runtime and bucket width, plus the test that drives a run to the cap.
2. **A bucket cannot disagree with the charge it describes.** Enforced by writing the bucket in the same fenced statement as the debit, never on a second pass.
3. **Bucket credits sum to the ledger row.** Enforced by the reconciliation test, mirroring the credit metric's existing identity including its zero arms.
4. **Event identity never reaches a metric label.** Enforced by leaving `credit_consumed` dimensions untouched; identity lives on the bucket row.
5. **No offload code exists without §4's measurement.** Enforced by RULE NDC at write time and by the rubric's absence check.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|---|---|---|---|---|---|
| Bucket write outcome | agentsfleetd | A fenced renewal or settle statement commits or rolls back | event id, bucket start, credits, outcome | Charge and identifiers only; no prompt or answer text | `test_uncommitted_slices_write_no_buckets` |
| Bucket read outcome | agentsfleetd | An expansion resolves or fails | event id, outcome, bucket count | No token or credit values in the failure log | `test_bucket_load_failure_degrades_in_place` |
| `credit_consumed` | agentsfleetd | Unchanged — the three existing charge sites | Unchanged; no new dimension | Identity stays off the metric by design | `credit_metric_reconciliation_integration_test.zig` |

Product analytics: the Usage tab gains one expansion interaction. The funnel playbook records it as an engagement signal on the billing surface; no new identifier or monetary value enters an analytics event.

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | integration | `test_buckets_sum_to_ledger_total` | A run with many renewals yields buckets whose credits sum exactly to the ledger's accumulated total |
| 1.2 | integration | `test_bucket_rows_per_event_are_bounded` | A run driven to maximum runtime under a fast renewal cadence stores no more than the derived maximum rows |
| 1.3 | integration | `test_uncommitted_slices_write_no_buckets` | A lost fence, a capped renewal and a replayed report each add zero bucket credits |
| 1.4 | unit | `test_bucket_row_is_self_describing` | A row carries event, window bounds, credits and token deltas with no join required |
| 2.1 | integration | `test_bucket_read_reconciles_with_charge_row` | Buckets return in window order and sum to the charge row's amount |
| 2.2 | integration | `test_bucket_read_refuses_sibling_tenant` | Another tenant's event answers not-found, identically to an unknown identifier |
| 2.3 | integration | `test_bucket_read_is_empty_not_error` | An event with no buckets answers an empty collection |
| 2.4 | unit | `test_removed_metering_routes_stay_404` | Both M154-removed spellings resolve to no route |
| 3.1 | e2e | `test_usage_row_expands_to_buckets` | Expanding a Run row shows buckets whose displayed credits sum to the row's amount |
| 3.2 | unit | `test_receive_row_does_not_expand` | A receive row renders with no expansion affordance |
| 3.3 | e2e | `test_bucket_load_failure_degrades_in_place` | With the read failing, every other cell stays readable and the expansion states the registered code |
| 3.4 | unit | `test_expansion_reuses_charge_formatting` | Expansion amounts and timestamps come from `lib/charges.ts`, not a second formatter |
| 4.1 | integration | `test_event_body_storage_is_measured` | The measurement records size and row count with the query that produced them |
| 4.2 | unit | `test_no_speculative_offload_code` | No content-hash or object-key symbol exists unless 4.1 missed its budget |
| regression | integration | `test_charge_list_shape_unchanged` | The charges list returns the same fields in the same shape as before this milestone |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Buckets reconcile with the ledger | `test_buckets_sum_to_ledger_total` | test passes | P0 | |
| R2 | Rows per event are bounded | `test_bucket_rows_per_event_are_bounded` | test passes | P0 | |
| R3 | The read refuses a sibling tenant | `test_bucket_read_refuses_sibling_tenant` | test passes | P0 | |
| R4 | The Run row opens | `test_usage_row_expands_to_buckets` | test passes | P0 | |
| R5 | Removed routes stay removed | `grep -rn 'metering-periods\|charges/.*telemetry' src/agentsfleetd/http/router.zig` | no output | P0 | |
| R6 | No speculative offload code | `grep -rniE 'request_hash\|response_hash\|body_object_key' src/ schema/ \| grep -vE ':[0-9]+:[[:space:]]*(//\|--)'` | no output unless §4.1 authorised it | P0 | |
| S1 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | |
| S2 | Lint clean | `make lint-all` | exit 0 | P0 | |
| S3 | Integration passes | `make test-integration` | exit 0 | P0 | |
| S4 | No leaks | `make memleak` | exit 0 | P0 | |
| S5 | Cross-compile | `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` | exit 0 | P0 | |
| S6 | No secrets | `gitleaks detect` | exit 0 | P0 | |
| S7 | Orphan sweep | Dead Code Sweep greps below | 0 matches | P0 | |

**Grading protocol (VERIFY):** run the Verify command verbatim; grade ONLY from its output. Graded = ✅/❌ + the one decisive output line; long evidence goes to PR Session Notes with a pointer here. **Ship gate:** every row graded, every P0 ✅ → eligible for CHORE(close); any ❌ or empty cell → return to EXECUTE.

## Dead Code Sweep

**1. Orphaned references — zero remaining live uses.** Each grep drops comment lines, so prose recording a retirement cannot fail the criterion asserting it.

| Deleted symbol/column | Grep | Expected |
|-----------------------|------|----------|
| `metering_periods` | `grep -rn -w 'metering_periods' src/ schema/ ui/ \| grep -vE ':[0-9]+:[[:space:]]*(//\|--\|\*)'` | 0 matches |
| stale "detail exists nowhere" comments | `grep -rn 'no longer exists anywhere\|answered it with a row per slice' src/ \| grep -vE 'bucket'` | 0 matches after RULE NLR cleanup |

## Out of Scope

- **`billing.usage_rollup`** — M154 deferred it because the ledger is bounded at two rows per event, `charge_type` having exactly two values. That reasoning stands and this workstream does not disturb it; the bucket table is a separate, also-bounded surface and is not an aggregation source.
- **Event body offload itself** — §4 measures; building is separate work with its own blast radius across five read paths and a change to whether object-storage credentials are mandatory at boot.
- **Retention for the bucket table** — bounded per event by construction, so retention is a datastore-lifetime question and Indy's call, not a code change here.
- **Restoring `metering-periods` or `charges/{event}/telemetry`** — both name granularities this milestone deliberately does not store. No alias, no redirect.
- **Row-Level Security** — its own milestone, with M154_001 as prerequisite.
- **Partitioning, sharding** — M154 left both behind measurements that are not this workstream's.

## Product Clarity (authoring record)

1. **Successful user moment** — a tenant sees a Run charge larger than they expected, opens it, and reads where the credits went without contacting anyone.
2. **Preserved user behaviour** — the Usage tab renders exactly as it does today: same rows, same columns, same amounts, same paging. The expansion is additive.
3. **Optimal-way check** — the optimal shape stores charge history at the coarsest granularity that still answers the question, keyed so the row count cannot grow with runner chattiness. Per-slice rows answered it and grew without bound; one accumulated row bounds growth and answers nothing.
4. **Rebuild-vs-iterate** — iterate. The metering, the fenced statements and the Usage tab all stay; this adds one arm, one read and one expansion.
5. **What we build** — a bounded per-event bucket table, a read for one charge's buckets, an expansion on the Run row, and a measurement gating body offload.
6. **What we do NOT build** — a per-slice table, a rollup, a retention sweeper, the body offload, or any alias for the removed routes.
7. **Fit with existing features** — reuses the ledger's upsert shape, the billing surface's scoping posture and the Usage tab's formatting vocabulary. No new external dependency.
8. **Surface order** — no reordering. The expansion opens in place beneath its own row; every other row keeps its position.
9. **Dashboard restraint** — no new page, no new navigation entry. One row becomes openable on a surface that already exists.
10. **Confused-user next step** — a charge whose buckets cannot load keeps every other cell on screen and states which part is unavailable, so the tenant knows the amount is still trustworthy.

## Decomposition & alternatives (patch vs refactor)

- **Chosen — fixed-width buckets accumulated in place.** Bounded by `maximum runtime ÷ bucket width`, a compile-time number, so growth is removed by construction rather than by a sweeper. Reuses an upsert shape already proven in the money path, keeps the read local and exact, and adds no external dependency to a user-facing surface.
- **Rejected — restore the per-slice table.** This is what M154 deleted for writing a row roughly every twenty seconds of every run. Adding retention to it trades unbounded growth for a sweeper that must never touch live work.
- **Rejected — emit slice records to the structured-log export and read them back.** The exporter is a fixed-capacity ring that drops under pressure and is installed only when its endpoint is configured, so the breakdown would be lossy exactly when a run is busiest. It also puts an external observability vendor on a user-facing read path, and the metric half deliberately carries no event identity.
- **Rejected — put event identity on the `credit_consumed` metric.** `otel_metrics_cardinality.zig` removed workspace identity from metric labels on purpose; per-event labels are strictly worse, and series accumulate across replicas and restarts where no process-local guard can bound them.
- **Rejected — append slices to a JSON column on the ledger row.** Keeps the row count flat but rewrites a growing value on every renewal, so bytes written grow quadratically across a long run.
- **Deferred — event body offload.** RULE NDC: no speculative code. §4 measures first and is permitted to build nothing.

## Discovery (consult log)

- **Consults** — Architecture / Legacy-Design / gate-flag triage: the question asked + Indy's decision.
- **Metrics review** — events added, extra events found during `/review`, analytics/funnel playbook update or the explicit no-change reason.
- **Skill-chain outcomes** — `/write-unit-test`, `/write-integration-test`, `/review`, `kishore-babysit-prs` results (order per `AGENTS.md` CHORE(close)).
- **Deferrals** — every "deferred to follow-up" needs an **Indy-acked verbatim quote** here, format `> Indy (YYYY-MM-DD HH:MM): "<quote>" — context: <which item, why>`.

  > Indy (2026-08-15 16:58): "if this is something we will defer later say its parked, since i wanna think thru how to do timeseries storage and these atomic usage charges later. and move it to docs/v2/done" — context: the whole workstream. Parked before CHORE(open) with no branch and no code; the storage shape is decided upstream of this spec, not inside it. See Parked at the top.

- **Why this spec was rewritten (Aug 14, 2026).** The first draft led with moving event bodies to object storage and framed per-slice charge detail as an audit trail that never existed. Reading `main` corrected both premises. The detail read is not the only production reader of the body columns — reclaim, the connector reply path and both repair-verification reads also select them, so the offload's blast radius was understated. And the per-slice charge is already emitted: `service_renew.zig` calls `recordCreditConsumed` once per metered slice. What is missing is narrower and more concrete than the draft claimed — the emitted sample deliberately carries no event identity, and the drill-down that used to open a charge was removed with M154's table. The user-visible gap is one unopenable row on a surface that already ships, which is what this workstream now addresses.

- **Required consult before EXECUTE — RULE LDC (Legacy-Design Consult).** This re-introduces a per-event charge-history table that M154 deliberately deleted. The distinction is that the retired table's row count grew with renewal frequency while this one is bounded by a compile-time constant, but re-adding a removed surface is exactly the case the A/B/C consult exists for. **A** — build the bounded bucket table as specified. **B** — leave the Run row unopenable and answer charge disputes by re-running. **C** — restore the per-slice table with a retention sweeper. The spec assumes **A**; Indy's decision is recorded here before CHORE(open).

- **Open decision — bucket width.** Named constant under RULE UFS, and it fixes the derived row bound and the resolution a tenant sees. Narrower reads better and stores more; the invariant holds at any width, so this is a product-resolution call rather than a correctness one.
