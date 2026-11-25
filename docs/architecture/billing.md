---
type: explanation
audience: contributor
verified: 2026-11-25
product_version: 0.90.0
executable: false
---

# Billing: how an account pays for its fleets' runs

> Read this when you need to know when an account is charged, what a charge costs, or why a turn was refused for money. Parent: [`README.md`](./README.md) · Model choice and provider keys: [models](./models.md).

## What it is

Every account (a tenant) has one wallet: a single balance in nanos, where 1 USD is 1,000,000,000 nanos.
A fleet's run writes a usage line for each model request and each slice of run time.
The statement that writes a line also debits the wallet, so usage and payment commit together.

| Question | Answer | Where in code |
|---|---|---|
| What unit is money in? | Nanos, as a signed 64-bit integer; below zero is a debt | `billing.wallets.balance_nanos` |
| Who pays? | The account that owns the fleet's workspace | `afd_billing` |
| When is money taken? | When a usage line is inserted; receiving a turn costs nothing | the triggers on `billing.usage_lines` |
| When is a turn refused? | At assignment, and every third renewal, when the balance cannot cover a small floor | `money_gates` in `afd_fleet`; `FUNDS_CHECK_TICKS` (3) in `afd_runner` |
| Can a balance go negative? | Yes; a debit is never clamped, and a debt refuses new work until a grant repays it | `billing.usage_lines_debit_wallets()` |
| What does a new account get? | A one-time starter grant, 5 USD | `STARTER_CREDIT_NANOS` in `afd_tenant`'s signup module |
| Is there a per-fleet budget? | No; the payer's balance is the only ceiling | `WorkKey::FleetRun` is the only kind of work |
| Where do prices live? | In the model catalogue, as dated tiers per model | `model.platform_model_prices` |

This page quotes no dollar rates; the authenticated billing surface serves them.

## Why it exists

One number that drains is the simplest money model a person can predict.
There are no plan tiers and no included-turn quota, so the charge functions take no plan parameter.
A refill is a grant that adds to the balance; a future paid plan would be a larger grant, never a branch in pricing.

Charging at the insert keeps one source of truth.
A run, its usage and the wallet debit commit or roll back in one transaction, so nothing settles later and nothing can be charged twice.

## How it behaves

### How a run is charged

```text
assign ──► renew ──► renew ──► … ──► report
  │          │                         │
  │          │ every third tick:       │
  │          │ balance ≥ floor?        │
  │          │                         │
  │          └─ each renewal and the report carry the runner's usage lines
  │
  └─ balance ≥ floor? else the turn settles `balance_exhausted`

inside the run's own transaction, per pass:
  METER_RUN advances the run's cursor (moves no money)
  log_lines inserts the lines into billing.usage_lines
    → row trigger: add the line to fleet.activity_counters.budget_used_nanos
    → statement triggers, in name order:
        trg_usage_lines_add_daily     → billing.usage_daily
        trg_usage_lines_count_runs    → billing.runs_daily
        trg_usage_lines_debit_wallets → billing.wallets (locked last, in tenant order)
```

The runner reports one line per provider request: `{seq, at, input, cached input, three cache-write classes, output}` (`af_wire::usage::RequestUsage`).
The daemon measures run time itself, from the run's `last_metered_at` cursor on the database clock (`METER_RUN` in `afd_fleet`).

Each line carries a kind (`af_wire::usage::LineKind`): `model`, `runtime` or `decision`.
The unique key is `(work_type, work_id, kind, seq)`, so a retried renewal or report inserts nothing new and debits nothing.
A renewal's answer names `recorded_through`, and the runner forgets only the lines up to it.

Renewals ride the runner stream as frames, not HTTP requests ([runner protocol](./runner_protocol.md)).
A renewal and a report carry every unrecorded line, oldest first, with no count cap; the frame size is the only bound.

| Step | What moves | Where |
|---|---|---|
| Assign | Nothing; the balance gate reads the wallet | `afd_fleet`'s run `start_check` module |
| Renew | On a checked tick, the balance must cover one more floor; every tick logs and charges its slice | `afd_fleet`'s run `renew` module |
| Report | The final slice is logged and charged in the report's transaction | `afd_fleet`'s run `report` module |

A person's **Stop** marks the run and tells it to end.
It does not undo provider work already started, and the usage already spent is still logged and charged.

### How a line is priced

`afd_billing::price::Rater` prices every line from one model's price book.

```rust
let book = PriceBook::load(executor, provider, model).await?;
let rated: RatedLine = Rater::new(&book, posture, window).rate(&line)?;
// RatedLine { usage, price: Option<Uuid7>, charged: Nanos, unpriced_tokens: u64 }
```

| Part | Platform model | Your own model key |
|---|---|---|
| Run time | `RUN_NANOS_PER_SEC` × seconds, rounded up | the same |
| Tokens | each category at its rate per million, each rounded up on its own | nothing; you pay your provider |
| Unknown price | tokens counted in `unpriced_token_count`, never charged | not applicable |

A posture is either `platform` or `self_managed`; [models](./models.md) explains which a fleet gets.
Token categories are input, cache read, the three cache-write classes and output.
Rounding each category up means the platform never under-bills and over-bills by at most one nano per category per line.

The price row is chosen by the request's instant and, for context tiers, by its whole prompt size.
The daemon clamps the instant to the run's window, so a runner cannot backdate a request into a cheaper range.
A cache write with no named lifetime bills at the tier's unspecified-lifetime rate, or stays unpriced if the tier has none.

Each daemon replica caches a price book under (posture, provider, model, catalogue version).
A statement that prices from a cached book writes nothing if the catalogue version moved, so a stale rate never charges.
A catalogue read that fails still charges run time and logs the tokens as unpriced (`run_tokens_unpriced`).

### The funds check

`afd_billing::biller::Biller` is the seam every funds check goes through, held as `Arc<dyn Biller>`.

| Method | Called by | Writes |
|---|---|---|
| `balance(&Account)` | the assignment gate | nothing |
| `check(&Ask)` | every third renewal per payer | nothing |

`check` answers `Covered`, `Short { available, asked }` or `Unconfirmed`.
The built-in `LedgerBiller` reads one row, the payer's `billing.wallets` balance (`afd_billing/src/sql/select_wallet.sql`).
Every logged line has already debited that row, so money spent by runs in flight already counts.

The floor is `ESTIMATE_FLOOR_INPUT_TOKENS` (100) and `ESTIMATE_FLOOR_OUTPUT_TOKENS` (100), priced at the issue instant.
A run on your own key has a zero floor, so only a payer in debt fails it.
A model with no price covering the instant estimates to zero, and the gate admits, because an estimate is not a charge.

`FailOpen<B>` wraps the biller where `agentsfleetd` builds it.
A check the biller cannot answer returns `Unconfirmed`, and the run continues.
That covers a lost connection and a statement that hit the API role's bounds.

`Observed<B>` wraps it outside that and counts `agentsfleet_billing_funds_check_total`.

### When the balance runs out

| Moment | What happens |
|---|---|
| Assignment | The turn settles `failure_reason = balance_exhausted`; the fleet stays `active` |
| Renewal | The biller answers `Short`, the renewal is refused `UZ-RUN-012`, and the run ends at its current deadline |
| Line insert | The debit goes below zero if it must; `exhausted_at` records the first crossing |
| Later grant | The grant repays the debt first; blocked turns are not replayed |

A payer with no wallet row is refused like an empty wallet; its lines are still logged and rolled up with nothing debited.
`agentsfleet usage show` prints `⚠ Out of credits` with the billing page link, then the most recent charges.

A balance-blocked turn is final.
To get the missed work answered, the person sends the message again or re-triggers the source.
A continuation follows only a card's answer, and a balance block has no card.

### The usage report

The dashboard's billing page (`/settings/billing`) reads one report covering the account across its workspaces.
The report reads two rollup tables that the line insert keeps, never the raw ledger.

| Table | One row per |
|---|---|
| `billing.usage_daily` | tenant, UTC day, work type, fleet and model: money and token quantities |
| `billing.runs_daily` | tenant, UTC day, work type and fleet: runs counted |

A line belongs to the Coordinated Universal Time (UTC) day it was created.
A run counts once, on the UTC day of its first runtime slice.
The rollups have no foreign keys, so a line for a deleted fleet or payer still lands, and both can be rebuilt from `billing.usage_lines`.

| Route | Scope | What it returns |
|---|---|---|
| `GET /v1/tenants/me/billing` | `billing:read` | the wallet snapshot |
| `GET /v1/tenants/me/billing/reports/current` | `billing:read` | period, bounds, totals, `by_fleet`, `daily` and filter choices, sent `private, no-store` |
| `GET /v1/tenants/me/billing/charges` | `billing:read` | raw usage lines, newest first, keyset-paged by `(created_at, id)` |
| `GET /v1/tenants/me/billing/activities/{activity_id}/charges` | `billing:read` | always `410 Gone`, `UZ-REQ-003` |

The report takes `period` (`month`, `week` or `today`), `fleet_id`, `activity_type` and `through_at`.
`activity_type` accepts only `runtime`; a `model` parameter is refused rather than ignored.
`activity_count` is the number of runs, on the totals and on each fleet group.

Amounts cross the API as integer nanodollar strings and are never recomputed from today's rates.
A line keeps the fleet's name as it was when written, so history survives the fleet's deletion.
Usage on your own model key appears as run time only; your provider bills the tokens.

## Limits

- **Parallel runs overdraw.** Nothing is set aside, so runs that start together all pass. Each can spend about three renewal slices before a check refuses. The test `parallel_runs_overdraw_by_at_most_a_slice_each_then_stop` pins the bound.
- **One fleet can drain the account.** The balance bounds the tenant, not how its fleets share it. A fleet that schedules itself or loops on tools can spend the whole balance.
- **An outage admits.** An unanswerable funds check lets the run continue, and its lines are still charged as they are logged.
- **An expired run's last slice is not billed.** A run that ends unreported leaves its time since the last renewal unlogged. That gap is at most one run lifetime.
- **No purchase flow.** Credits arrive only as the starter grant or a manual grant; the schema has no purchase table.
- **No key test call.** A bad provider key surfaces at the first turn, when the provider refuses it.
- **Declared but unrecorded.** `agentsfleet_billing_unpriced_tokens_total` has no caller.

## Related pages

- [Models and provider keys](./models.md)
- [Runner protocol](./runner_protocol.md): renewal frames and run timing
- [Postgres schemas](./postgres_schemas.md): the `billing` schema
- [Observability](./observability.md): the cost metric families
- [Concurrency](./concurrency.md): the API login role's statement bounds
