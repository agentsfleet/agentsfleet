# M249 UI cleanup and report integration

The owner requested a UI audit after the approved [ux.html](../../ux.html) implementation.
This follow-up preserves the [rendered design review](M249_DESIGN_REVIEW.md) and the paused [Plugin implementation](../parked/README.md).

## Updated base

PR #845 is rebased onto the requested `6edd08d8402747b3468820c50ce1102bb79fe2fc`.
The supplied parent contains the daily rollup and the trimmed report renderer.
Its producers, settlement SQL, report queries and model-label store remain Claude's work.

The report request supplies `period`, `fleet_id`, `activity_type` and `through_at` only.
The dashboard consumes totals, daily charges, fleet/model projections and available fleet choices.
It no longer carries activity disclosure, charge paging or a model filter.

The Period control retains named calendar months and the existing inclusive cutoff.
All Fleets includes Setup and saved-fleet usage; individual fleets follow it.
Changing a control retains supported report filters and removes obsolete URL state.

The decoder requires recorded activity counts for period and fleet totals.
Daily and model rows accept the server's omitted count.
The chart still refuses more than 31 daily buckets, and zero usage draws no positive-height bars.

## SQL scope

The UI follow-up adds no SQL or table changes relative to `6edd08d84`.
That parent already adds `billing.usage_daily` and `billing.runs_daily`, with their settlement and report statements.
Those inherited changes remain distinct from this UI cleanup.

The supported raw charge endpoint and its response types remain available.
Removing an unused frontend wrapper does not remove a server endpoint or the acceptance ledger read.

## Audit method

The first import inventory parsed 1,451 tracked UI modules across the app, website and design system.
It used Bun's syntax parser, local aliases, static imports, re-exports and literal dynamic imports.
Framework routes, package exports, build scripts, declarations and test entry points were classified separately.

The installed Next.js page and layout guides establish the framework entry points.
Script references explain the website text generator and the server-only module checker.
These files were retained rather than mistaken for unused page code.

Exact-symbol searches and caller inspection reviewed 45 additional export candidates.
GPT-6 Astra independently checked the classifications and the cascading unused write validators.
Response types, framework hooks, test observations, shared read schemas and the paused Plugin helper remain available.

The final file inventory contains 1,442 modules, 68 runtime entry points and 654 modules reached from those entries.
Its 787 test entry points include controlled browser fixtures and support files.
Every parsed module has a classified path; parsing reported no errors.

The remaining production-orphan candidate is `fleets/scope.ts`, deliberately retained for the paused Plugin UI.
This static inventory does not prove arbitrary computed import paths or future callers.
Typechecking and the independent source review supplement it.

## Removed leftovers

| Area | Removed | Retained behavior |
|---|---|---|
| Usage | Old spending-bar component, activity/detail tests and obsolete navigation state | Reviewed daily chart, complete fleet rows, exact amounts and one report read |
| Models | Unused table cells, provider-action hook, old sorting/base-price helpers and write-only catalogue validators | Live model labels, model dialogs, read schemas and inference types |
| Navigation | Unused refresh acknowledgement control and old draft-search state controller | Current retry controls, sidebar data and native draft ownership |
| Conversation | Unused work-action presentation, output-display policy, sender initials and group-span helper | Recorded work attribution, active transcript rendering and shared duration formatting |
| Client adapters | Unused single-approval, fleet-event and tenant-charge wrappers; secret classifiers | Supported server endpoints, workspace event reader, secret variants and acceptance reads |
| Copy and fixtures | Unused bounds/copy constants, isolated tests and stale mock exports | Live bounds, actual event-query coverage and full-page filtered-load coverage |

The model-label helper remains: typechecking identified its two live dialog callers during cleanup.
Its tests and the desktop/phone model-selection checks remain in place.
No package, public route, test gate, coverage floor or scanner exception changed.

## Review and checks

The repeated design capture contains 156 states across 13 pages, three viewports and both themes.
It records zero page errors, zero document overflow and font probes identical to the reference.
Recorded geometry matches the earlier corrected matrix.

Astra opened model selection on desktop and phone and reported no model-name regression.
Its final source review found only an empty test group, which was removed.
The standards reviewer identified two deleted tests for retained behavior; both were restored through the current public surfaces.

The final native `make dry-app` passed 4,575 unit tests across 488 files and all 134 browser tests.
The browser report records zero failed, flaky or skipped cases; its run took 6.8 minutes.
The first invocation was stopped before browsers after review found a stale model-filter expectation.

The corrected test proves obsolete model, paging and disclosure fields never reach the report request.
Scale checks verify 101 rows become one selected fleet, with the correct daily high, then restore all rows.
The zero-usage case measures zero bar height.

The app production build and every configured size budget passed.
Shared dashboard JavaScript is 290.57 kB gzipped; Usage adds 33.95 kB and Security adds 41.27 kB.
Their unchanged limits are 294.91 kB shared and 102.4 kB per route addition.

Native Rust lint passed, and the changed-document scanner returned no findings.
The final root gate and new-head GitHub checks remain pending at this checkpoint.
Earlier counts remain historical and do not establish this rebased revision's result.

## Browser performance after trimming

The controlled browser fixture injects HTTP responses without delay and preloads the shared model labels.
Each range covers one dark and one light sample, without CPU or network throttling.
These measurements do not establish live API latency.

| Viewport | Fleets | First totals ms | Open dropdown ms | Select fleet ms | Restore all rows ms | Report bytes |
|---|---:|---:|---:|---:|---:|---:|
| Desktop | 1 | 202–327 | 65–125 | 268–288 | 504–517 | 5,890 |
| Desktop | 100 | 235–360 | 108–122 | 305–339 | 532–592 | 41,949 |
| Phone | 1 | 177–308 | 59–60 | 284–304 | 516–538 | 5,890 |
| Phone | 100 | 238–330 | 100–155 | 307–363 | 568–865 | 41,949 |

Each flow made three report requests: initial load, selected fleet and All Fleets.
It made zero Usage catalogue or charge reads.
Populated October fixtures contain 31 daily buckets within a 100px plot; fleet count changes rows rather than chart dimensions.

With model names delayed by 1,200ms, totals/chart appeared at 518ms desktop and 301ms phone.
The named table followed at 1,838ms and 1,612ms, with one report request each.
The delayed fixture excludes the shared label store's initial server request.

Receipts under `/workspace/artifacts/m249-runtime/` include `rollup-dry-app-final-retry.log`, `rollup-dry-browser-report.json` and `rollup-browser-performance.json`.
Production measurements are in `rollup-production-build.log` and `rollup-production-size.log`.

The previous pushed revision `4b1fe75cf` passed every GitHub check except the Rust coverage gate.
That gate measured 107,152 of 109,546 lines, or 97.8146 percent, against the unchanged 98 percent requirement.
Its changed-line coverage was 1,210 of 1,210, or 100 percent.

A separate Rust addition tests authoritative task fencing, exact refusals, single attempts and the resolver's address policy.
These are unit checks at the daemon transport and resolver boundaries, not authenticated acceptance or HTTP authorization proof.
No coverage increase is claimed before measurement.

The earlier live results and remaining hosted-access limits stay in the [runtime evidence](M249_POST_MERGE_RUNTIME_VALIDATION.md).
Controlled browser data, local integration results, collection results and live acceptance remain separate.

## Empty filtered periods and retained paging

Astra confirmed a P2 when the selected fleet has no usage in a newly selected period.
The URL retained its fleet identifier, but the returned choices omitted it and left the dropdown label blank.
The control now retains that identifier without inventing a name or widening the report.

An empty `fleet=` value now displays All Fleets, matching the request and chart.
All Fleets remains the first choice; clearing a selected fleet keeps the period and cutoff.

Both new regressions failed before their repairs and passed afterward.
The final scoped suite passed 13 report-view tests and nine unchanged shared-pager tests.
The final affected browser suite passed 20 cases in 46.5 seconds, with no failed, flaky or skipped case.

Astra reviewed the four new desktop/phone, dark/light screenshots and the final source without further findings.
The native standards reviewer also found no issue.
The successful final production build measures Usage at 33.96 kB gzipped, below its unchanged 102.4 kB limit.

GitHub's first rebased app coverage job passed all 4,575 assertions but failed at 99.98 percent statements.
The old activity list had been the only caller of the shared pager's optional fixed-parameter argument.
Removing that unused argument and its loops preserves the remaining EventsList caller and its existing nine tests.

The first rebased daemon coverage shard passed 192 cases and failed its unsupported-event end-state check.
Its log names `integration_runner_e2e/refusals.rs:32`.
No assertion, deadline or retry was changed; the final native gate and new-head run must settle that result.

Receipts include `empty-fleet-regression-before.log`, `blank-fleet-regression-before.log`, `empty-fleet-and-pager-final.log` and `final-empty-fleet-browser.log`.
The final bundle checks are `final-empty-fleet-build.log` and `final-empty-fleet-size.log`.

## Final stylesheet sweep

A repository search found activity-table selectors remaining after their only component was deleted.
The cleanup removes those selectors and their mobile labels; the active billing container and shared layout rules remain unchanged.
Astra confirmed that no retained page uses the removed classes.

The subsequent affected browser run passed 26 cases in 1.2 minutes.
It covers Usage, scale checks and the retained resource pages across desktop/phone and both themes.
The reference HTML remains unchanged.

The exact-parent native Rust unit baseline passed 1,731 runner, 172 daemon and 3,674 library tests.
Its clean integration run stopped after 924 passes at the missing rollup-table grant inventory.
These baseline results remain separate from the final branch's checks and authenticated acceptance.

Companion docs PR #249 describes the trimmed API and marks it as upcoming.
Its complete native checks pass against exact API source `6edd08d84`.
Publication remains blocked until that API branch reaches main, whose current document still exposes the removed activity-charge route.
