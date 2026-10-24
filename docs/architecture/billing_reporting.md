# Billing reporting

**Delivery status:** Approved for pull request 784; implementation and final acceptance are in progress.

The Billing page shows settled spending for the account across its workspaces. The existing header keeps the available credit balance.

The page adds period, fleet and model filters, settled totals, expandable billed activities and a spending breakdown. Add credits retains the existing support-email action.

## Read flow and ownership

```text
Signed-in account member
  → Billing page and shared controls
  → tenant reporting handlers, requiring billing:read
  → TenantBilling service
  → afd_billing/report
  → settled billing.usage_lines for that tenant
```

The account comes from the authenticated person; no request chooses another tenant. Every query binds that tenant. Reporting reads cannot mint credits, settle work or modify a wallet.

The domain types and queries remain in `afd_billing`, with no HTTP dependency, so another transport can implement the existing service seam later.

An activity groups usage by the stored work type and work identifier: one fleet lease or one private setup attempt. Expanding it reads the individual settled lines with bounded pagination. Setup labels reveal no prompt, draft title, private message, checkpoint or summary.

Runtime labels use the ledger's fleet-name snapshot and survive fleet deletion.

## Time, filters and amounts

- Month to date and Today use Coordinated Universal Time (UTC). A request captures an upper timestamp. The lower bound is inclusive; the upper bound is inclusive at stored millisecond precision.

  Only records settled by the upper timestamp contribute.
- Totals cover the entire selected period and filters. They never sum only the displayed activity page. One read transaction provides a consistent image for the report's totals, breakdowns and activity page.
- A continuation carries the original time bounds, tenant, filters and page size. Reusing it with different filters or another tenant is refused. The next boundary includes every ordering key, so tied timestamps cannot skip activities.
- All fleets includes runtime and setup. Setup chat selects setup attempts.

  A specific fleet selects its runtime ledger rows. Model filtering is exact. Filter choices come from the whole period, independently of the displayed page.
- Amounts remain integer nanodollar strings over the API and use the existing exact-small-amount formatter. A combined stored charge is never recomputed from today's rates.
- The ledger currently stores combined charged money with token and runtime quantities. The page shows those quantities and the actual combined charge. It does not fabricate separate historical inference, runtime or compaction money.

  Compaction's metered calls contribute to ordinary settled totals.
- A customer's own provider key means model usage is charged by that provider; the page distinguishes it from credits charged here. Missing provider usage does not mean free usage. Unsettled or unknown usage can arrive later and is excluded from settled totals.

## Public reads

`GET /v1/tenants/me/billing/reports/current` returns the selected period, bounds, full-period summary, spending groups, available filters and a bounded activity page. The page uses the standard `items`, `total`, `next_cursor` envelope inside the report. The existing balance and raw-charge endpoints retain their behavior.

`GET /v1/tenants/me/billing/activities/{activity_id}/charges` returns the individual settled lines for one work item in the same period and filters. The work type is explicit. Unknown or foreign work produces an empty scoped list without disclosing whether another account owns it.

Both reads require the existing `billing:read` permission. Limits use the shared default and maximum. Query lengths are declared with `garde` before parsing; malformed values and mismatched continuations return the shared invalid-request response.

Responses declare private, no-store caching.

## Query bounds and evidence

Report predicates select one tenant and one day or month. The existing tenant/time index supports those bounds; PostgreSQL may choose a sequential scan for a small table. The database groups and orders the scoped rows for totals and activity pagination.

Activity grouping precedes keyset pagination; display lookups never run once per result in application code. Detail reads use the existing unique work/sequence index. No new table, materialized report or reporting daemon is required.

The database proof prefixes the production statements and bindings with `EXPLAIN (ANALYZE, FORMAT JSON)`. Increasing scoped histories and a larger unrelated tenant must retain the intended index conditions and bounded row scans.

Report reads use a fixed number of queries, independent of page size. Concurrent read proofs check isolation, overlap and connection return; an unavailable datastore returns an error without changing money.

## Implementation scope

| Path | Purpose |
|---|---|
| `rustd/crates/afd_billing/src/report/` | Owned report types, period selection, aggregates, grouped paging and detail reads; static SQL beside each use case. |
| `rustd/crates/afd_billing/src/tenant/mod.rs` | Delegate read methods using the existing database handle. |
| `rustd/crates/afd_billing/src/lib.rs` | Export the reporting module. |
| `rustd/crates/afd_http/src/services/billing.rs` | Add typed read methods to the current transport seam. |
| `rustd/crates/afd_api_tenant/src/handler/tenant/billing_report/` | Bounded input, scope-bound continuation parsing, thin handlers and wire conversion. |
| `rustd/crates/afd_api_tenant/src/handler/tenant/mod.rs` | Register the reporting handlers. |
| `rustd/crates/afd_api_tenant/src/lib.rs` | Mount each new served read. |
| `rustd/crates/afd_api_tenant/src/openapi.rs` | Register the endpoint annotations. |
| `rustd/crates/afd_http/src/route/tenant.rs` | Add both read routes with the existing bearer and BillingRead guards. |
| `rustd/crates/afd_api_wire/src/billing/` | Reporting response types and shared OpenAPI query shapes, separate from storage rows. |
| `rustd/crates/afd_api_wire/src/lib.rs` | Export the reporting response module. |
| `rustd/crates/afd_api/tests/` | Served-route permission, tenant-isolation, malformed-input and response proofs; update exhaustive route assertions. |
| `rustd/crates/afd_billing/tests/` | Real-schema aggregate, paging, settlement, concurrency, failure and query-plan proofs. |
| `ui/packages/app/lib/pagination/use-url-cursor-pages.ts` | Retain the captured report cutoff while moving between pages or changing page size. |
| `ui/packages/app/lib/api/billing_report.ts` | Validate reporting responses and perform authorized reads. |
| `ui/packages/app/app/(dashboard)/settings/billing/` | Integrate the approved page, URL filters, grouped activity and detail loading. |
| `ui/packages/design-system/src/design-system/` | Add a reusable summary primitive only if the existing primitives cannot express it. |
| `ui/packages/app/components/domain/{EventsList,FleetReplyFigures}.tsx`; `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/[id]/components/FleetStatusLine.tsx` | Import money formatting directly from the existing shared money module after removing the old Billing re-export. |
| `ui/packages/app/tests/` | Whole-period totals, exact amounts, filter reset, details, failures and responsive page behavior. |
| `public/openapi.json` | Generated public additions after source annotations are complete. |
| `docs/DESIGN_SYSTEM.md` | Record a new shared primitive if one is introduced. |

## Acceptance

The real Billing route must be viewable in the local acceptance deployment with ordinary authentication. Existing settled local usage may populate it; an empty account gets an honest empty state. The illustrative design preview remains labeled separately and is never used as proof of account data or served API behavior.

Prove complete-period totals beyond one page, equal-time activity boundaries, filter and tenant cursor mismatches, and exclusion of pending rows.
Also test deleted-fleet labels, setup privacy, customer-key wording, exact tiny amounts, detail failure and retry.

Check query plans on growing real histories, concurrent reads and connection return.
The unchanged 181-message and ten-fleet continuity scenarios remain independently required for shared compaction.
