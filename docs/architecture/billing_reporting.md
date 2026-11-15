# Billing reporting

The Usage page shows settled spending for the account across its workspaces. The existing header keeps the available credit balance.

The page shows Period and Fleet controls, settled totals, daily spending and a Fleet/Model/Activities/Charged table.
More filters, Charge details and Add credit remain paused; their reusable APIs and components stay intact.

## Read flow and ownership

```text
Signed-in account member
  → Usage page and shared controls
  → tenant reporting handlers, requiring billing:read
  → TenantBilling service
  → afd_billing report module
  → settled billing.usage_lines for that tenant
```

The account comes from the authenticated person; no request chooses another tenant. Every query binds that tenant. Reporting reads cannot mint credits, settle work or change a wallet.

The domain types and queries stay in `afd_billing`, with no HTTP dependency, so another transport can serve the same service seam later.

An activity groups usage by the stored work type and work identifier: one fleet lease or one private setup attempt. Expanding it reads the individual settled lines with bounded pagination. Setup labels reveal no prompt, draft title, private message, checkpoint or summary.

Runtime labels use the ledger's fleet-name snapshot and survive fleet deletion.

## Time, filters and amounts

- Month to date, Today and Last 7 days use Coordinated Universal Time (UTC). Last 7 days starts at midnight six calendar days before the cutoff date. Both bounds are inclusive at stored millisecond precision.
- Only records settled by the upper timestamp contribute.
- Totals cover the whole selected period and filters, never only the displayed activity page. One read transaction gives the report's totals, breakdowns and activity page a consistent image.
- A continuation carries the original time bounds, tenant, filters and page size. Reusing it with different filters or another tenant is refused. The next boundary includes every ordering key, so tied timestamps cannot skip activities.
- All Fleets includes saved-fleet and setup usage. Individual fleet choices follow it; a specific fleet selects its runtime ledger rows. Existing explicit Setup URLs keep their scoped label.
- Model filtering is exact. Filter choices come from the whole period, independent of the displayed page.
- Amounts stay integer nanodollar strings over the API and use the existing exact-small-amount formatter. A combined stored charge is never recomputed from today's rates.
- The ledger stores combined charged money with token and runtime quantities. The page shows those quantities and the actual combined charge. It does not invent separate historical inference, runtime or compaction money.
- Compaction's metered calls contribute to ordinary settled totals.
- A customer's own provider key means that provider charges the model usage; the page distinguishes it from credits charged here. Missing provider usage does not mean free usage. Unsettled or unknown usage can arrive later and is excluded from settled totals.

## Daily spending and recorded models

The required `daily` array includes every UTC calendar date from `from_at` through `through_at`, ascending. Days without matching settled lines have zero totals.
Each line belongs to its `created_at` date and must have settled by `through_at`.

Daily `activity_count` counts distinct work items on that date. An activity spanning midnight counts on both days and once for the period.
Every other daily total adds exactly to the corresponding period total.

Each `spending.by_fleet` item includes sorted, distinct, nonempty recorded model identifiers in `models`.
These models come from all matching ledger rows, including deleted fleets and setup charges. Current fleet definitions and the activity cursor do not affect them.

The chart keeps nanodollars as integers until formatting or bounded display scaling. Accessible labels name each day and charge; an unavailable report never displays authoritative zero usage.

## Shared presentation

The page uses the canonical [UX reference](../ux.html), shared Manrope typography, a named-month Period control and an All Fleets selector.
The owner deferred timezone changes. A past reporting month uses its inclusive `through_at` cutoff; the current month clears it.

Partial snapshots show their through date as a separate choice, allowing a return to the full or live month.
Displayed instants and ranges use shared viewer-local `Time` utilities. Calendar bucket identities stay as supplied by the API.

The daily plot has a fixed 100px height and at most 31 bars. A decoder refusal handles oversized projections without clipping charges.
The shared fleet select scrolls within the available viewport height.


An existing model-filtered URL shows its exact model identity and a clear action; narrowed totals cannot hide behind the removed picker.

The shared model-label request starts once per document load alongside shell reads.
Its pending promise stays in `ModelLabelsProvider`; the named fleet table waits inside its own `Suspense` boundary.

The heading, controls, totals and daily plot can render from the report while labels are pending.
A failed label read resolves to recorded identifiers; there is no identifier/name swap after the table paints.

The active Usage boundary ignores old activity/detail URL parameters so paused charge controls cannot add hidden reads.
The reusable detail loader still validates visible activity identity, captured cutoff and charge pagination for a later design.
Exact charge amounts and combined historical lines retain their recorded meanings.

## Public reads

`GET /v1/tenants/me/billing/reports/current` returns the selected period, bounds, full-period summary, spending groups, available filters and a bounded activity page. The page uses the standard `items`, `total`, `next_cursor` envelope inside the report. The existing balance and raw-charge endpoints keep their behavior.

`GET /v1/tenants/me/billing/activities/{activity_id}/charges` returns the individual settled lines for one work item in the same period and filters. The work type is explicit. Unknown or foreign work produces an empty scoped list without disclosing whether another account owns it.

Both reads require the existing `billing:read` permission. Limits use the shared default and maximum. Query lengths are declared with `garde` before parsing; malformed values and mismatched continuations return the shared invalid-request response.

Responses declare private, no-store caching.

## Query bounds and evidence

Report predicates select one tenant and one day, seven days or month. The existing tenant/time index supports those bounds; PostgreSQL may choose a sequential scan for a small table. The database groups and orders the scoped rows for totals and activity pagination.

Activity grouping precedes keyset pagination; display lookups never run once per result in application code. Detail reads use the existing unique work/sequence index. No new table, materialized report or reporting daemon is required.

The database proof prefixes the production statements and bindings with `EXPLAIN (ANALYZE, FORMAT JSON)`. Growing scoped histories and a larger unrelated tenant must keep the intended index conditions and bounded row scans.

Report reads use a fixed number of queries, independent of page size. Concurrent read proofs check isolation, overlap and connection return; an unavailable datastore returns an error without changing money.

## Implementation scope

`afd_billing`'s report module owns the report types, period selection, aggregates, grouped paging and detail reads, with static SQL beside each use case. `afd_api_tenant` holds the bounded input parsing and thin handlers, and `afd_api_wire` the response types. `afd_http` registers both routes behind the bearer and `BillingRead` guards.

The dashboard page under `ui/packages/app` makes one report request, validates it and retains complete filtered aggregates.
Filter changes reset continuations; fleet changes preserve the reporting cutoff. Model names reuse the shared label store, without a Usage catalogue request.


The charge API and loader remain reusable; the paused UI makes no charge request.

## Acceptance

The real Usage route must be viewable in the local acceptance deployment with ordinary authentication. Existing settled local usage may populate it; an empty account gets an honest empty state. The illustrative design preview stays labeled separately and is never proof of account data or served API behavior.

Prove complete-period totals beyond one page, equal-time activity boundaries, filter and tenant cursor mismatches, and exclusion of pending rows.
Also test deleted-fleet labels, setup privacy, customer-key wording, exact tiny amounts, detail failure and retry.

Check query plans on growing real histories, concurrent reads and connection return.
The unchanged 181-message and ten-fleet continuity scenarios stay independently required for shared compaction.
