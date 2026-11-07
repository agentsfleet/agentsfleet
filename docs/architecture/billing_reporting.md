# Billing reporting

The Billing page shows settled spending for the account across its workspaces. The existing header keeps the available credit balance.

The page adds period, fleet and model filters, settled totals, expandable billed activities and a spending breakdown. Add credits keeps the existing support-email action.

## Read flow and ownership

```text
Signed-in account member
  → Billing page and shared controls
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

- Month to date and Today use Coordinated Universal Time (UTC). A request captures an upper timestamp. The lower bound is inclusive, and the upper bound is inclusive at stored millisecond precision.
- Only records settled by the upper timestamp contribute.
- Totals cover the whole selected period and filters, never only the displayed activity page. One read transaction gives the report's totals, breakdowns and activity page a consistent image.
- A continuation carries the original time bounds, tenant, filters and page size. Reusing it with different filters or another tenant is refused. The next boundary includes every ordering key, so tied timestamps cannot skip activities.
- All fleets includes runtime and setup. Setup chat selects setup attempts. A specific fleet selects its runtime ledger rows.
- Model filtering is exact. Filter choices come from the whole period, independent of the displayed page.
- Amounts stay integer nanodollar strings over the API and use the existing exact-small-amount formatter. A combined stored charge is never recomputed from today's rates.
- The ledger stores combined charged money with token and runtime quantities. The page shows those quantities and the actual combined charge. It does not invent separate historical inference, runtime or compaction money.
- Compaction's metered calls contribute to ordinary settled totals.
- A customer's own provider key means that provider charges the model usage; the page distinguishes it from credits charged here. Missing provider usage does not mean free usage. Unsettled or unknown usage can arrive later and is excluded from settled totals.

## Shared presentation

The page uses the shared title, three equal-width filters, a UTC period label and unboxed period totals.
Activity uses DataTable's plain surface, keeping its column headings and pagination when empty.
Controlled row disclosure reads details only for the selected visible activity; native buttons expose the expanded state and support keyboard activation.

The disclosure uses the shared DescriptionList, with left-aligned labels and right-aligned values on the card surface.
Exact positive charge amounts keep up to nine decimal places. The total always covers the whole activity, including undisplayed charge pages.

Inference-only and runtime-only ledger lines receive their fixed labels. Combined lines keep an explicit combined label without inferred price splits.

Spending uses the shared List and UsageBar primitives for full-period fleet and model groups.
Money stays an exact integer until display; bar ratios are reduced with integer arithmetic before conversion to bounded percentages.

Billing uses the shared `font-system` utility to match its approved mock.
Activity text uses 12 pixels, names and totals use medium weight, and spending rows use 13 pixels with normal weight.

The app imports date formatting from the design system's `time-utils.ts` exports.
`Time` keeps each timestamp's canonical value while the shared UTC helpers align period and activity labels.
The existing app source check rejects page-local date formatters.

## Public reads

`GET /v1/tenants/me/billing/reports/current` returns the selected period, bounds, full-period summary, spending groups, available filters and a bounded activity page. The page uses the standard `items`, `total`, `next_cursor` envelope inside the report. The existing balance and raw-charge endpoints keep their behavior.

`GET /v1/tenants/me/billing/activities/{activity_id}/charges` returns the individual settled lines for one work item in the same period and filters. The work type is explicit. Unknown or foreign work produces an empty scoped list without disclosing whether another account owns it.

Both reads require the existing `billing:read` permission. Limits use the shared default and maximum. Query lengths are declared with `garde` before parsing; malformed values and mismatched continuations return the shared invalid-request response.

Responses declare private, no-store caching.

## Query bounds and evidence

Report predicates select one tenant and one day or month. The existing tenant/time index supports those bounds; PostgreSQL may choose a sequential scan for a small table. The database groups and orders the scoped rows for totals and activity pagination.

Activity grouping precedes keyset pagination; display lookups never run once per result in application code. Detail reads use the existing unique work/sequence index. No new table, materialized report or reporting daemon is required.

The database proof prefixes the production statements and bindings with `EXPLAIN (ANALYZE, FORMAT JSON)`. Growing scoped histories and a larger unrelated tenant must keep the intended index conditions and bounded row scans.

Report reads use a fixed number of queries, independent of page size. Concurrent read proofs check isolation, overlap and connection return; an unavailable datastore returns an error without changing money.

## Implementation scope

`afd_billing`'s report module owns the report types, period selection, aggregates, grouped paging and detail reads, with static SQL beside each use case. `afd_api_tenant` holds the bounded input parsing and thin handlers, `afd_api_wire` the response types, and `afd_http` registers both routes behind the bearer and `BillingRead` guards.

The dashboard page under `ui/packages/app` validates the responses, keeps the captured report cutoff across page moves, and loads grouped activity and detail rows. Shared table, disclosure and UTC date helpers live in the design system.

## Acceptance

The real Billing route must be viewable in the local acceptance deployment with ordinary authentication. Existing settled local usage may populate it; an empty account gets an honest empty state. The illustrative design preview stays labeled separately and is never proof of account data or served API behavior.

Prove complete-period totals beyond one page, equal-time activity boundaries, filter and tenant cursor mismatches, and exclusion of pending rows.
Also test deleted-fleet labels, setup privacy, customer-key wording, exact tiny amounts, detail failure and retry.

Check query plans on growing real histories, concurrent reads and connection return.
The unchanged 181-message and ten-fleet continuity scenarios stay independently required for shared compaction.
