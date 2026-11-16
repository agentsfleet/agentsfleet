# Billing reporting

The Usage page shows settled spending for the account across its workspaces. The existing header keeps the available credit balance.

The page shows period and fleet controls, settled totals, daily spending and a Fleet/Model/Activities/Charged table. The per-run activity list and its charge details were removed; their design will return later from scratch. Add credit waits for its design.

## Read flow and ownership

```text
settle transaction (relay, every 5 s)            Usage page
  settle_lines.sql    stamp lines settled         GET /v1/tenants/me/billing/reports/current
  add_usage_daily.sql add them to day rows          → tenant reporting handler (billing:read)
  count_run.sql       count the run once            → TenantBilling service
  debit_wallet.sql    debit the wallet (last)       → afd_billing report module
                                                    → billing.usage_daily + billing.runs_daily
```

The account comes from the authenticated person; no request chooses another tenant. Every query binds that tenant. Reporting reads cannot mint credits, settle work or change a wallet.

The domain types and queries stay in `afd_billing`, with no HTTP dependency, so another transport can serve the same service seam later.

## The daily rollup

The report reads two tables the settle transaction maintains, never the raw ledger:

- `billing.usage_daily`: settled money and quantities per tenant, Coordinated Universal Time (UTC) day, work type, fleet and model.
- `billing.runs_daily`: settled runs per tenant, UTC day, work type and fleet.

`ledger/settle.rs` runs four statements in one transaction and one fixed order. `settle_lines.sql` stamps the lines settled, guarded by `settled_at IS NULL`, and returns them. `add_usage_daily.sql` adds exactly those lines to their day rows, written in key order.

`count_run.sql` counts the run, and `debit_wallet.sql` debits the wallet last. The rollup reads only the lines the guard just stamped, so each charge enters it once. A repeat pass, a rolled-back settle or a second relay adds nothing.

No statement takes an explicit lock, and every settle takes its row locks in the same order.

A run counts once, on the UTC day of its first runtime line, in the transaction that settles that line. Lease runtime slices are numbered by their start, so no smaller slice can arrive later.

A setup attempt writes one runtime line. Two relays settling different lines of the same run therefore count it once.

The ledger stays the source of truth: the rollup is derived and can be rebuilt from settled `billing.usage_lines`. Like the ledger, the rollup has no foreign keys. Its `tenant_id` and `fleet_id` are opaque, so a settle for an erased payer still lands.

## Time, filters and amounts

- Month to date, Today and Last 7 days use UTC. The report covers every whole UTC day from the period's first day through the cutoff's day.
- A line belongs to the UTC day it was created. A line created just before midnight and settled seconds after it counts on its creation day.
- All fleets includes runtime and setup; Setup chat selects setup attempts. A specific fleet selects its runtime usage. Filter choices come from the whole unfiltered period.
- The report has no model filter. A request that sends `model` is refused, because ignoring it would answer with a wider report.
- Amounts stay integer nanodollar strings over the API and use the existing exact-small-amount formatter. A combined stored charge is never recomputed from today's rates.
- A customer's own provider key means that provider charges the model usage; the page distinguishes it from credits charged here. Missing provider usage does not mean free usage. Unsettled usage is excluded until the relay settles it.

## Daily spending and recorded models

The required `daily` array includes every UTC calendar date from `from_at` through `through_at`, ascending. Days without settled usage have zero totals. Every daily total adds exactly to the corresponding period total.

`activity_count` is the number of settled runs. It appears on the period totals and on each fleet group; model and daily groups carry none, because one run spans several models and days.

Each `spending.by_fleet` item includes sorted, distinct, nonempty recorded model identifiers in `models`, including deleted fleets and setup charges.

The chart keeps nanodollars as integers until formatting or bounded display scaling. Accessible labels name each day and charge; an unavailable report never displays authoritative zero usage.

## Public reads

`GET /v1/tenants/me/billing/reports/current` returns the selected period, bounds, full-period summary, spending groups, daily spend and filter choices. Query parameters are `period`, `fleet_id`, `activity_type` and `through_at`, declared with `garde` before parsing; malformed values return the shared invalid-request response. It requires `billing:read`, and responses declare private, no-store caching.

`GET /v1/tenants/me/billing/activities/{activity_id}/charges` stays in the route table and answers `410 Gone` with `UZ-REQ-003`, so a client still calling it learns the removal is permanent. `GET /v1/tenants/me/billing/charges` lists raw charges.

## Performance evidence

A load simulation on Postgres 19rc2 with 15-minute runs measured the report read, warm. It used three accounts: 10 fleets (86,400 lines a month), 100 fleets (864,000) and 1,000 fleets (8.64M).

| Read | 10 fleets | 100 fleets | 1,000 fleets |
|---|---|---|---|
| Raw ledger, the previous statement | ~465 ms | ~5,030 ms | ~87,900 ms |
| Daily rollup | ~1.2 ms | ~10 ms | ~170–260 ms |

The rollup matched the raw statement row for row (`EXCEPT ALL`, 0 differences). Settling with the rollup cost about 12% throughput when every client hit one account's wallet, and nothing measurable across many accounts. Details are in `docs/v2/evidence/M249_004_SOURCE_INVENTORY.md`.

## Implementation scope

`afd_billing` owns the rollup tables' writes (`ledger/settle.rs`) and the report read (`report/`), with static SQL beside each use case. `afd_api_tenant` holds the bounded input parsing and the thin handler, and `afd_api_wire` the response types. `afd_http` registers the route behind the bearer and `BillingRead` guards.

The dashboard page under `ui/packages/app` makes one report request and validates the response. Model names come from the dashboard's shared label store, not a catalogue read.

## Acceptance

The real Usage route must be viewable in the local acceptance deployment with ordinary authentication; an empty account gets an honest empty state.

Integration tests prove each of these through the real settle path:
- a duplicate settle adds nothing, and a rolled-back settle leaves nothing behind;
- a run that crosses midnight splits its money by day and counts once;
- two concurrent settles of one run count it once, and settles in opposite orders finish without a deadlock;
- a line settled after month end counts in the month it was created;
- the report and the rollup equal the settled ledger.

## Dashboard presentation

The Period control lists named months and sends their existing inclusive reporting cutoffs.
All Fleets includes Setup and saved fleets; individual fleet choices follow it.
Control changes clear obsolete URL state and retain the supported report filters.

The daily chart uses CSS bars inside the shared dashboard panel.
It adds no chart library, observer or extra report request.
The shared model-label store names the fleet rows; delayed labels hold only that table's boundary.

The [rendered review](../v2/evidence/M249_DESIGN_REVIEW.md) and [UI cleanup](../v2/evidence/M249_UI_CLEANUP.md) record the reference, measurements and validation limits.
