# Billing reporting

The Usage page shows spending for the account across its workspaces. The existing header keeps the available credit balance.

The page shows period and fleet controls, totals, daily spending and a Fleet/Model/Activities/Charged table. The per-run activity list and its charge details were removed; their design will return later from scratch. Add credit waits for its design.

## Read flow and ownership

```text
work's transaction                                Usage page
  INSERT INTO billing.usage_lines                   GET /v1/tenants/me/billing/reports/current
  (log_lines, commit_report, renew_batch)             → tenant reporting handler (billing:read)
    same statement, for each new line:                → TenantBilling service
      trg_usage_lines_add_daily     day rows          → afd_billing report module
      trg_usage_lines_count_runs    run count         → billing.usage_daily + billing.runs_daily
      trg_usage_lines_debit_wallets wallet (last)
```

A run, a setup renewal and a decision call log their lines in their own transaction (`afd_billing::ledger::log_lines`, or the fleet's own report and renewal statements). The insert charges each new line in the same statement (Indy, Nov 22, 2026: "apply at write"): three statement-level triggers on `billing.usage_lines` add it to the rollup, count its run and debit its payer's wallet. Funds checks read the wallet alone (`afd_billing/src/sql/select_wallet.sql`).

The account comes from the authenticated person; no request chooses another tenant. Every query binds that tenant. Reporting reads cannot mint credits, write a line or change a wallet.

The domain types and queries stay in `afd_billing`, with no HTTP dependency, so another transport can serve the same service seam later.

## The daily rollup

The report reads two tables the lines' insert writes, never the raw ledger: the raw read took about 88 seconds for a 1,000-fleet month (Performance evidence, below).

- `billing.usage_daily`: money and quantities per tenant, Coordinated Universal Time (UTC) day, work type, fleet and model.
- `billing.runs_daily`: runs per tenant, UTC day, work type and fleet.

Each trigger reads only the lines its statement inserted. A line the unique key `(work_type, work_id, kind, seq)` turned away is not among them, so a repeated write adds nothing, and a rolled-back write takes its rollup rows and debit with it.

Postgres fires a statement's triggers in name order: the day rows, then the run count, then the wallet. Day and run rows are written in key order and wallets lock in tenant order, so two writes take their row locks in the same order.

A run counts once, on the UTC day of its first runtime slice: the new line that carries run time with the smallest number, when no line of its kind numbered lower is already logged. Only a runtime slice carries run time (`af_wire::usage::UsageLine`). A fleet run and a setup run number their runtime slices by start and write them in that order, so no smaller slice arrives later.

The ledger stays the source of truth: the rollup is derived and can be rebuilt from `billing.usage_lines`. Like the ledger, the rollup has no foreign keys. Its `tenant_id` and `fleet_id` are opaque, so a line for an erased payer still lands.

## Time, filters and amounts

- Month to date, Today and Last 7 days use UTC. The report covers every whole UTC day from the period's first day through the cutoff's day.
- A line belongs to the UTC day it was created, and the trigger functions run in UTC to file it there.
- All fleets includes runtime and setup; Setup chat selects setup runs (`work_type = 'setup_run'`). A specific fleet selects its runtime usage. Filter choices come from the whole unfiltered period.
- The report has no model filter. A request that sends `model` is refused, because ignoring it would answer with a wider report.
- Amounts stay integer nanodollar strings over the API and use the existing exact-small-amount formatter. A combined stored charge is never recomputed from today's rates.
- A customer's own provider key means that provider charges the model usage; the page distinguishes it from credits charged here. Missing provider usage does not mean free usage.

## Daily spending and recorded models

The required `daily` array includes every UTC calendar date from `from_at` through `through_at`, ascending. Days without usage have zero totals. Every daily total adds exactly to the corresponding period total.

`activity_count` is the number of runs. It appears on the period totals and on each fleet group; model and daily groups carry none, because one run spans several models and days.

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

The rollup matched the raw statement row for row (`EXCEPT ALL`, 0 differences). Writing the rollup beside the wallet debit cost about 12% throughput when every client hit one account's wallet, and nothing measurable across many accounts; that measure predates the move into the lines' insert. Details are in `docs/v2/evidence/M249_004_SOURCE_INVENTORY.md`.

## Implementation scope

The rollup tables' writes are the triggers in their own schema files (`rustd/crates/daemon/afd_db/schema/*_billing_usage_daily.sql`, `*_billing_runs_daily.sql`). `afd_billing` owns the report read (`report/`), with static SQL beside each use case. `afd_api_tenant` holds the bounded input parsing and the thin handler, and `afd_api_wire` the response types. `afd_http` registers the route behind the bearer and `BillingRead` guards.

The dashboard page under `ui/packages/app` makes one report request and validates the response. Model names come from the dashboard's shared label store, not a catalogue read.

## Acceptance

The real Usage route must be viewable in the local acceptance deployment with ordinary authentication; an empty account gets an honest empty state.

Integration tests prove each of these through the real insert (`afd_billing/tests/report/writes.rs`, `reconcile.rs`, `billing/biller.rs`):
- a line logged twice debits its wallet once and adds to the rollup once;
- a run that crosses midnight splits its money by day and counts once;
- writes in opposite day orders on shared rows finish without a deadlock;
- a line counts in the month it was created;
- the report and the rollup equal the ledger.

## Dashboard presentation

The Period control lists named months and sends their existing inclusive reporting cutoffs.
All Fleets includes Setup and saved fleets; individual fleet choices follow it.
Control changes clear obsolete URL state and retain the supported report filters.

The daily chart uses CSS bars inside the shared dashboard panel.
It adds no chart library, observer or extra report request.
The shared model-label store names the fleet rows; delayed labels hold only that table's boundary.

The [rendered review](../v2/evidence/M249_DESIGN_REVIEW.md) and [UI cleanup](../v2/evidence/M249_UI_CLEANUP.md) record the reference, measurements and validation limits.
