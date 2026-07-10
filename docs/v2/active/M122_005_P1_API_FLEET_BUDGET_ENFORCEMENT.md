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

# M122_005: Enforce the per-fleet spend ceiling `daily_dollars` / `monthly_dollars`

**Prototype:** v2.0.0
**Milestone:** M122
**Workstream:** 005
**Date:** Jul 10, 2026
**Status:** IN_PROGRESS
**Priority:** P1 — `daily_dollars` is a required field in every `TRIGGER.md` and is advertised as a hard spend ceiling that "terminates the in-flight event cleanly." It terminates nothing. A fleet in a tool-call loop bills the tenant's credit pool until the pool empties or the lease's max-runtime cap fires. This is a cost-safety boundary that silently does not exist.
**Categories:** API
**Batch:** B1 — shares one branch and one Pull Request (PR) with M122_001; disjoint file set.
**Branch:** `feat/m122-served-doc-parity` (shared with M122_001, per Indy's Jul 10 2026 same-tree decision)
**Test Baseline:** unit=2402 integration=267
**Depends on:** none. **Blocks:** M122_001 §1 — the docs rewrite describes this enforcement, so it must not merge ahead of this code.
**Provenance:** discovered Jul 10, 2026 during M122_001's §1 golden-path walk (`grep -rn FleetBudget src/` → parsed, stored, read only by `config_parser_test.zig`). Escalated to Indy as a judgment-class gate flag; he chose to build the enforcement rather than delete the documented promise. Decision + rationale recorded in M122_001's Discovery log.
**Canonical architecture:** `docs/architecture/billing_and_provider_keys.md` — defines posture and the tenant-scoped credit pool. It is silent on per-fleet dollar ceilings, so this workstream introduces that concept and owns the corresponding architecture-doc diff.

---

## Overview

**Goal (testable):** a fleet whose rolling-24-hour spend has reached `daily_dollars` (or whose calendar-month spend has reached `monthly_dollars`) has its next event refused at the lease gate with `status=gate_blocked, failure_label=budget_breach`, and an already-running run is killed at its next `/renew` with `status=fleet_error, failure_label=budget_breach`.

**Problem:** `parseFleetBudget` (`config_helpers.zig:251`) validates `daily_dollars` and `monthly_dollars`, `FleetConfig.budget` (`config_types.zig:149`) stores them, and nothing in the daemon or the runner ever reads the field — `grep -rn "\.budget\." src/ --include='*.zig'` returns exactly one hit, in `config_parser_test.zig`. This is RULE DFS (no dead struct fields) on a money path. Three user-facing docs pages promise a `budget_breach` entry; the label has zero hits in `src/`. The only ceilings that actually stop a run are the lease deadline (`timeout_kill`), the `/renew` policy stops (`renewal_terminate` — max-runtime, lease lost, or *tenant* credit exhausted), and the cgroup limits. A tenant with a healthy credit pool has no per-fleet blast-radius guard at all, which is exactly what `daily_dollars` is sold as (`tests/fixtures/fleetbundle/platform-ops/TRIGGER.md:69` — *"`daily_dollars` is a blast-radius guard"*).

**Solution summary:** one new pure module `fleet/budget.zig` (dollars→nanos, the ceiling comparison, and the windowed spend query) plus two call sites that mirror their existing credit-gate siblings exactly. The pre-run gate sits in `service_billing.zig` beside `balanceCoversEstimate`, where `session.config.budget` is already in hand — no extra lookup. The mid-run gate sits in `service_renew.zig` beside `creditsCover`, and refuses renewal with a new `UZ-RUN-015`. Because the runner's `RenewResult.terminal` currently discards the response body, it cannot today tell a budget stop from a no-credits stop; this workstream plumbs a `FailureClass` reason from the refusal body through `RenewDecision` → `ReadOutcome` → `classify`, so the durable `failure_label` says `budget_breach` rather than laundering it into `renewal_terminate`. Spend is a windowed `SUM(credit_deducted_nanos)` over `core.fleet_execution_telemetry`, served by the existing `(workspace_id, fleet_id, recorded_at DESC)` index. **No schema change.**

## PR Intent & comprehension handshake

- **PR title (eventual):** Enforce per-fleet daily and monthly spend ceilings
- **Intent (one sentence):** the `daily_dollars` a fleet author writes into `TRIGGER.md` actually stops that fleet from spending more than that in a rolling day, and the operator can tell from the event's `failure_label` that the budget — not the credit pool, not the clock — is what stopped it.
- **Handshake** — restated: *`FleetBudget` is dead config on a cost path. Make it live at the two points where the platform already asks "may this run proceed?", give the refusal its own durable label so triage is unambiguous, and change nothing else about how runs are metered, billed, or reported.*

  `ASSUMPTIONS I'M MAKING:`
  1. **The ceiling is a floor-check, not a projection.** A run is admitted while `spend < cap` and refused once `spend >= cap`. A single admitted run may overshoot the cap before its next `/renew` tick — bounded by one renewal window, not unbounded. Enforcing a *predicted* end-of-run cost would mean refusing runs that would have finished under budget, and is not what "budget breach terminates the in-flight event" describes.
  2. **`monthly_dollars` is optional** (`FleetBudget.monthly_dollars: ?f64`). Absent ⇒ no monthly ceiling. Only `daily_dollars` is required, so only the daily ceiling is universal.
  3. **The daily window is rolling 24 hours; the monthly window is the Coordinated Universal Time (UTC) calendar month.** This is what `~/Projects/docs/fleets/authoring.mdx` already states ("Rolling 24-hour dollar ceiling" / "Calendar-month ceiling"), and the docs are the spec here.
  4. **Three distinct "no verdict" causes, three answers.** A database (DB) fault admits (fail open), mirroring `balanceCoversEstimate` (`metering.zig:77`: *"Any DB failure returns true (fail-open) so the gate never turns into an availability incident"*). A fleet that declares **no** budget admits — undeclared is unbounded, exactly as before this gate existed. Only a *declared-but-unparseable* budget refuses (fail closed). Conflating the middle case with the last one would kill the in-flight runs of every fleet row written by a path that bypasses `parseFleetConfig`.
  7. **The gate is inert during the free trial.** Every charge is `0` until `FREE_TRIAL_END_MS` (`2026-08-01T00:00:00Z`), so no fleet accrues `credit_deducted_nanos` and no budget is consumed. This is the same property the balance gate has, and the honest one: no charge, no spend. Both gates begin to bite when the window closes. The tests seed telemetry directly, so they exercise the gate regardless.
  5. **Spend means credit drained**, i.e. `SUM(credit_deducted_nanos)` — what the tenant was actually charged — not the metered-but-forgiven amount. On the slice that exhausts a wallet, `charged_nanos < metered` and the remainder is forgiven; a budget must count money, not intent.
  6. **Daemon and runner ship together.** Adding a `FailureClass` variant is additive on the wire (`failure_reason: ?FailureClass = null`), so an older runner that never sends `budget_breach` still reports cleanly against a newer daemon.

## Implementing agent — read these first

1. `src/agentsfleetd/fleet/service_billing.zig:131-181` — the pre-run gate chain: `resolveTenant` → `balanceCoversEstimate` → `debitReceive` → `checkApprovalGate`. The budget check goes **between `balanceCoversEstimate` and `debitReceive`**, so a refused event never pays the receive fee. `blockEvent` (line 58) is the terminal writer.
2. `src/agentsfleetd/fleet/service_renew.zig:79-90` — the mid-run gate chain: lease-active check → `creditsCover` → `completeRenew`. The budget check goes after `creditsCover` (line 87). Note `Lease` (line 46) carries no `fleet_id`/`workspace_id` — `loadLeaseInner` must select them.
3. `src/runner/daemon/control_plane_client.zig:193-255` — `RenewResult.terminal: u16` carries only the HTTP status; `classifyRenew` drops the 4xx body. This is the single obstacle to a granular label, and the file this workstream changes to remove it.
4. `src/runner/child_supervisor_result.zig:40-61` — `classify()`'s precedence ladder; `outcome.terminated` currently collapses every renewal stop to `.renewal_terminate`.
5. `src/agentsfleetd/fleet_runtime/metering.zig:75-99` — the fail-open sibling gate whose shape, logging, and error handling `budget.zig` mirrors.
6. `schema/011_fleet_execution_telemetry.sql` — `credit_deducted_nanos`, `recorded_at` (epoch ms `BIGINT`), and the `(workspace_id, fleet_id, recorded_at DESC)` index the spend query rides.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `src/agentsfleetd/fleet/budget.zig` | CREATE | pure ceiling math (`dollarsToNanos`, `covers`) + the windowed spend query + the renew-side budget fetch, with the pure-half unit tests inline (matching `event_rows.zig` / `context_resolve.zig` in the same directory) |
| `src/agentsfleetd/fleet/budget_integration_test.zig` | CREATE | real-Postgres tests for the windowed sum + both gates' terminal rows |
| `src/agentsfleetd/tests.zig` | EDIT | register `fleet/budget.zig` with the unit-test root |
| `src/agentsfleetd/fleet/event_rows.zig` | EDIT | add `LABEL_BUDGET_BREACH = "budget_breach"` beside the other `gate_blocked` labels (RULE UFS single ownership site) |
| `src/agentsfleetd/fleet/service_billing.zig` | EDIT | pre-run gate between the balance check and the receive debit |
| `src/agentsfleetd/fleet/service_renew.zig` | EDIT | `Lease` gains `fleet_id`/`workspace_id`; budget gate after `creditsCover` |
| `src/agentsfleetd/errors/error_entries.zig` | EDIT | register `UZ-RUN-015` (`.payment_required`) — the `/renew` refusal |
| `src/agentsfleetd/errors/error_entries_runtime.zig` | EDIT | register `UZ-EXEC-015` — the runner-engine mirror `errorCodeForFailure` demands for the new `FailureClass` |
| `src/agentsfleetd/errors/error_registry.zig` | EDIT | `ERR_RUN_BUDGET_EXCEEDED = "UZ-RUN-015"`, `ERR_EXEC_BUDGET_BREACH = "UZ-EXEC-015"` |
| `src/runner/engine/client_errors.zig` | EDIT | the runner's mirror of both codes + the exhaustive `errorCodeForFailure` arm (the compiler forces this: adding a `FailureClass` variant fails the build until the arm exists) |
| `src/agentsfleetd/fleet_runtime/config_helpers.zig` | EDIT | rename the file-local `MS_PER_SECOND` (a milliseconds name bounding *dollars*) to `MAX_DAILY_BUDGET_UNITS`; value unchanged (RULE UFS / RULE NLR) |
| `src/lib/common/clock.zig` | EDIT | `startOfUtcMonthMillis(now_ms)` — pure, injectable `now`, no wall-clock read |
| `src/lib/contract/execution_result.zig` | EDIT | `FailureClass.budget_breach` |
| `src/runner/daemon/control_plane_client_renew.zig` | CREATE | the pure `/renew` classification half — `TerminalRenew`, `RenewResult`, `classifyRenew`, `isTerminalRenewStatus`, and the body's `error_code` → `FailureClass` map. Split out because the parent sat AT the 350-line cap (RULE FLL), mirroring the existing `control_plane_client_mint.zig` split |
| `src/runner/daemon/control_plane_client.zig` | EDIT | re-export the renew half; `renew()` keeps the I/O |
| `src/runner/daemon/renew_driver.zig` | EDIT | map the terminal reason onto `RenewDecision.terminate`; log the class-specific `UZ-EXEC-*` code |
| `src/runner/child_supervisor_read.zig` | EDIT | `RenewDecision.terminate: FailureClass`; `applyTick` returns `?FailureClass`; both call sites carry the reason |
| `src/runner/child_supervisor_result.zig` | EDIT | `ReadOutcome.terminate_reason` (defaulted); `classify` honours it |
| `src/runner/child_supervisor_test.zig` | EDIT | scripted hooks return the widened decision; assert `budget_breach` propagation and the default |
| `src/runner/child_supervisor_concurrency_test.zig` | EDIT | same widened decision in the concurrency harness |
| `src/runner/daemon/control_plane_client_test.zig` | EDIT | `UZ-RUN-015` → `budget_breach`; every other cause (incl. `UZ-RUN-012` on the same 402, and unparseable bodies) stays `renewal_terminate` |
| `src/runner/daemon/renew_driver_test.zig` | EDIT | a budget refusal rides through `onTick` as `.terminate = .budget_breach` |
| `src/runner/daemon/renew_driver_concurrency_test.zig` | EDIT | widened decision in the concurrency harness |

`src/runner/daemon/lease_run.zig` needs no edit after all — its `TickFanout.onTick` forwards `driver.tick(...)` verbatim, so the widened `RenewDecision` passes through untouched. `src/runner/child_supervisor.zig` likewise: it re-exports `read_mod.RenewDecision` by alias, which widens for free.
| `docs/architecture/billing_and_provider_keys.md` | EDIT | document the per-fleet ceiling as a distinct concept from the tenant credit pool |

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — **DFS** (no dead struct fields — `FleetConfig.budget` is the exact violation this workstream retires), **UFS** (`budget_breach`, every SQL statement, and the window constants are named constants with a single ownership site), **NLR** (touch-it-fix-it: the `MS_PER_SECOND`-as-dollar-bound misnomer is corrected because this workstream touches `parseFleetBudget`'s neighbourhood), **ORP** (that rename gets a cross-layer orphan sweep), **NDC** (no dead code: every new `pub` fn has a production caller by the end of the diff), **TIM** (the rolling-24h and calendar-month windows are explicit, injectable `now_ms` — never a hidden wall-clock read), **TST-NAM** (test identifiers milestone-free), **IDMP** (a `gate_blocked` row is terminal; a re-delivered stream entry must not re-block or re-charge).
- **`dispatch/write_zig.md`** — ZIG / PUB / LIFECYCLE gates: `budget.zig` returns a tagged union rather than a bare `bool` where the caller must distinguish "over budget" from "could not tell"; every `conn.query()` drains in the same function before `deinit()`; file ≤ 350 lines, fn ≤ 50.
- **`docs/LOGGING_STANDARD.md`** — both refusals log with an `error_code` field, matching `lease_balance_exhausted` / `renew_no_credits`.
- **`docs/architecture/billing_and_provider_keys.md`** — the credit pool is tenant-scoped; the budget is fleet-scoped. The two gates are independent and both must pass.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | yes | memory safety on the arena-allocated budget JSON; `errdefer` on the parse; cross-compile both linux targets |
| PUB / Struct-Shape | yes | `budget.zig` exports exactly `Spend`, `Verdict`, `dollarsToNanos`, `covers`, `spendForFleet`, `fetchBudgetAndSpend` — a `FILE SHAPE DECISION` is emitted at PLAN |
| LIFECYCLE GATE | yes | `conn.query()` in `spendForFleet`/`fetchBudgetAndSpend` → `PgQuery.from(...)` + `defer q.deinit()` + drain in-fn; verified by `make check-pg-drain` |
| File & Function Length (≤350/≤50/≤70) | yes | `budget.zig` is small by construction; `service_renew.zig` gains ~12 lines — check it stays ≤350 |
| UFS (repeated/semantic literals) | yes | `LABEL_BUDGET_BREACH`, `SELECT_SPEND_SQL`, `SELECT_BUDGET_AND_SPEND_SQL`, `ROLLING_DAY_MS` as named constants |
| ERROR REGISTRY | yes | `UZ-RUN-015` added to `error_entries.zig` **and** `error_registry.zig`; comptime asserts enforce prefix/uniqueness/non-empty hint |
| LOGGING | yes | `lease_budget_breach` / `renew_budget_breach` carry `error_code`, `fleet_id`, `event_id` |
| SCHEMA GUARD | no | no `schema/*.sql` touched; the spend query rides an existing index |
| UI Substitution / DESIGN TOKEN | no | no UI |
| MILESTONE-ID | yes | no `M122_005` string in any test identifier (RULE TST-NAM) |

## Prior-Art / Reference Implementations

- **Pre-run gate (§1):** `metering.balanceCoversEstimate` + `service_billing.blockEvent` — same signature shape (`pool, alloc, …) bool`), same fail-open-on-DB-fault posture, same `blockEvent(… LABEL_*)` terminal write. Divergence: the budget is fleet-scoped and already parsed on `session.config`, so there is no tenant lookup.
- **Mid-run gate (§2):** `service_renew.creditsCover` + `hx.fail(ERR_RUN_LEASE_RENEWAL_NO_CREDITS)` — the exact refusal shape, including the 402 status so the runner's existing `isTerminalRenewStatus` set is unchanged. Divergence: the refusal body's `error_code` is now read by the runner.
- **Reason plumbing (§3):** `ExecutionResult.failure: ?FailureClass` already flows runner → `/reports` → `event_rows.markTerminal` → `failure_label`. This workstream adds one variant and one carrier field; it invents no new transport.
- **Windowed aggregate (§1):** `http/handlers/fleets/list.zig:174` runs the all-time `SUM(credit_deducted_nanos)` per fleet. The windowed form adds `AND recorded_at >= $N` and rides `idx_fleet_execution_telemetry_workspace_id_fleet_id_recorded_at`.

## Sections (implementation slices)

### §1 — `budget.zig` and the pre-run gate

The pure half first: `dollarsToNanos(f64) i64` (× `tenant_billing.NANOS_PER_USD`, rounded, saturating — the parser already bounds inputs to `(0, 1000]` daily and `(0, 10000]` monthly, so the product cannot approach `i64` max, and the saturation is a belt-and-braces guard rather than a live path); `covers(FleetBudget, Spend) Verdict` returning `.ok` / `.day_exceeded` / `.month_exceeded`. Then `spendForFleet(pool, alloc, workspace_id, fleet_id, now_ms) ?Spend` — one statement with two `FILTER`ed sums over `core.fleet_execution_telemetry`, `null` on any DB fault so the caller can fail open explicitly rather than by coincidence. `startOfUtcMonthMillis` lands in `clock.zig` as a pure function of `now_ms` (RULE TIM — the tests pass a fixed `now`, never read the clock).

The gate slots into `runBilling` between `balanceCoversEstimate` and `debitReceive`, so a refused event is never charged the receive fee. On refusal: `blockEvent(hx, fleet_id, event_id, rows.LABEL_BUDGET_BREACH)` → `status=gate_blocked, failure_label=budget_breach`, guarded on `status=received`, and the caller returns no-work without XACKing (identical to `balance_exhausted`).

- **Dimension 1.1** — `covers` admits at `spend < cap` and refuses at `spend == cap` and `spend > cap`, for the daily ceiling → Test `covers admits below the daily cap and refuses at or above it` → **DONE** (unit green)
- **Dimension 1.2** — `monthly_dollars == null` ⇒ no monthly ceiling; a fleet far past any month figure still admits when the day is clear → Test `covers treats an absent monthly ceiling as unlimited` → **DONE** (unit green)
- **Dimension 1.3** — `dollarsToNanos` rounds to the nearest nano and never wraps: `1.0 → 1_000_000_000`; `0.000000001 → 1`; the parser's max daily (`1000.0`) → `1e12` → Test `dollarsToNanos rounds to nearest and never wraps` → **DONE** (unit green; a non-finite ceiling collapses to 0 and refuses, proven by `a zero-collapsed ceiling refuses every run rather than admitting one`)
- **Dimension 1.4** — `startOfUtcMonthMillis` returns the first instant of the UTC month for a fixed `now_ms`, including a leap-year February and a month-boundary millisecond → Test `startOfUtcMonthMillis truncates to the first instant of the UTC month` + leap-year + pre-epoch clamp → **DONE** (unit green)
- **Dimension 1.5** — `spendForFleet` sums only the target fleet, only inside each window, and only `credit_deducted_nanos` (not metered) → Tests `integration: spend_for_fleet_counts_only_the_rolling_day_inside_the_day_window`, `..._excludes_rows_before_the_calendar_month_start`, `..._is_scoped_to_one_fleet_and_one_workspace`, `..._reports_zero_for_a_fleet_that_has_never_run` → **DONE** (4 integration tests green against live Postgres; stable across 3 consecutive runs)
- **Dimension 1.6** — a fleet whose 24h spend has reached `daily_dollars` gets its next event written `status=gate_blocked, failure_label=budget_breach`, and **no receive debit is taken** → Test `test_lease_gate_blocks_over_budget_fleet` (integration)
- **Dimension 1.7** — a spend that could not be read admits the event (fail-open) and logs `error_code` → **DONE** (the decision is now a pure function, `budget.verdictOrAdmit(null, tight_budget) == .ok`; test `verdictOrAdmit fails OPEN when the spend could not be read`)

### §2 — the mid-run `/renew` gate

`loadLeaseInner` gains `fleet_id::text, workspace_id::text` (both already columns on `fleet.runner_leases`); `Lease` gains the two fields, and both are read, so RULE DFS holds. `budgetCovers(hx, lease)` calls `budget.fetchBudgetAndSpend(conn, fleet_id, workspace_id, now_ms)` — one statement returning the fleet's `config_json->'x-agentsfleet'->'budget'` subobject alongside the two windowed sums. The subobject is parsed by **`config_helpers.parseFleetBudget`**, the same function that validates it at ingest, so the ceiling can never be interpreted two ways. On refusal: `hx.fail(ERR_RUN_BUDGET_EXCEEDED, …)` → HTTP 402, already in the runner's `isTerminalRenewStatus` set.

Reading the budget live (rather than pinning it onto the lease row at issue) is deliberate: lowering a runaway fleet's `daily_dollars` takes effect at its next renewal tick instead of at its next run. It also keeps this workstream free of a schema migration.

- **Dimension 2.1** — `/renew` for a lease whose fleet is over its daily ceiling answers 402 with `error_code=UZ-RUN-015` and does not extend the lease → Test `test_renew_refuses_over_budget_lease` (integration)
- **Dimension 2.2** — `/renew` for a fleet under budget renews exactly as before; the added gate costs one query and changes no response → Test `test_renew_under_budget_unchanged` (integration, regression)
- **Dimension 2.3** — the budget the renew gate reads equals the budget `parseFleetBudget` accepts: a declared-but-invalid stored budget refuses the renewal rather than silently admitting it → **DONE** (`refusalFor fails CLOSED on a stored budget it cannot parse` + integration `a declared-but-malformed budget still refuses (fail closed)`); superseded Tests `integration: fetch_budget_and_spend_refuses_an_unparseable_stored_budget`, `..._refuses_when_the_budget_key_is_absent`, `..._returns_null_when_the_fleet_row_is_gone` → **DONE** (integration green; the reuse of `parseFleetBudget` is what makes the two ceilings one number)
- **Dimension 2.5** — a fleet whose `config_json` declares no `budget` renews normally; only a *declared-but-malformed* budget refuses → Tests `integration: fetch_budget_and_spend_admits_a_fleet_that_declares_no_budget`, `integration: a declared-but-malformed budget still refuses (fail closed)`
- **Dimension 2.4** — an unavailable database renews (fail-open), mirroring `creditsCover` → **DONE** (pure decision: `budget.refusalFor(.unavailable) == null`; test `refusalFor fails OPEN on an unavailable database and on an absent fleet`)

### §3 — carry the reason to the durable `failure_label`

Today every renewal stop lands as `renewal_terminate`, because `RenewResult.terminal` carries an HTTP status and `classifyRenew` discards the body. Widen `terminal` to `{status: u16, reason: FailureClass}`, parse `error_code` from the refusal body (`UZ-RUN-015` → `.budget_breach`; anything else, including an unparseable body, → `.renewal_terminate`). Widen `RenewDecision.terminate` to carry that `FailureClass`, `applyTick` to return `?FailureClass`, and `ReadOutcome` to carry `terminate_reason` (defaulted to `.renewal_terminate`, so untouched call sites keep today's behaviour). `classify` then returns `failed(outcome.terminate_reason)`.

`metrics_runner.incRunnerFailure(runner_id, body.failure_reason)` buckets the new variant with no change (`service_report.zig:123`).

- **Dimension 3.1** — a `/renew` refusal carrying `UZ-RUN-015` produces `ExecutionResult{exit_ok: false, failure: .budget_breach}` → Test `classify: a fleet-budget terminate reports budget_breach, not renewal_terminate` → **DONE** (runner unit green)
- **Dimension 3.2** — a `/renew` refusal with any other `error_code`, or an unparseable body, still produces `.renewal_terminate` → Test `classifyRenew: any other refusal cause stays renewal_terminate` (covers UZ-RUN-012 on the same 402, empty/absent/unparseable bodies) → **DONE** (runner unit green)
- **Dimension 3.3** — a terminate that did not come from a renewal refusal (hook returns terminate with no reason) defaults to `.renewal_terminate` → Test `classify: an unset terminate_reason defaults to renewal_terminate` → **DONE** (runner unit green)
- **Dimension 3.4** — the report path persists `failure_label='budget_breach'` on the event row for a budget-killed run → Test `test_report_persists_budget_breach_label` (integration)
- **Dimension 3.5** — `FailureClass.budget_breach` serialises as the exact string `budget_breach` on the wire → Test `budget_breach serialises as the exact durable failure_label` → **DONE** (unit green)

### §4 — retire the dead field and the misnamed bound

`FleetConfig.budget` stops being a dead struct field the moment §1 lands, satisfying RULE DFS. Separately, `config_helpers.zig:13` declares `const MS_PER_SECOND = 1000.0` whose *only* use (line 258) is as the upper bound on `daily_dollars` — a milliseconds name doing duty as a dollar ceiling. Rename to `MAX_DAILY_BUDGET_UNITS`, value unchanged, sweep for orphans (RULE ORP). Document the per-fleet ceiling in `docs/architecture/billing_and_provider_keys.md` as a fleet-scoped guard distinct from the tenant-scoped credit pool.

- **Dimension 4.1** — `grep -rn "config\.budget" src/ --include='*.zig' | grep -v _test` returns at least one production hit (the field is live) → **DONE** (`service_billing.zig:96` — `budget.covers(session.config.budget, spend)`; zero hits at `origin/main`). *(Amended Jul 10 2026: the criterion originally grepped `\.budget\.`, which requires a field access THROUGH `budget` and matches nothing — the real reader is `session.config.budget`, with no trailing dot. As written the row would have reported 0 and "proved" the field was still dead.)*
- **Dimension 4.2** — `MS_PER_SECOND` no longer appears in `config_helpers.zig`, and the daily bound is unchanged at 1000.0 → Test existing `config_parser_test.zig` bounds cases → **DONE** (rename is value-preserving; `grep -c MS_PER_SECOND config_helpers.zig` → 0)
- **Dimension 4.3** — the architecture doc names the per-fleet ceiling and its two gates → verified by `make lint-all` (`check_architecture_doc.sh`) and the R8 grep

## Interfaces

```
src/agentsfleetd/fleet/budget.zig
  pub const Spend = struct { day_nanos: i64, month_nanos: i64 };
  pub const Verdict = enum { ok, day_exceeded, month_exceeded,
                             pub fn refused(self: Verdict) bool };
  pub const BudgetError = error{UnreadableBudget};

  pub fn dollarsToNanos(dollars: f64) i64;   // non-finite/<=0 -> 0 (a cap that refuses)
  pub fn covers(budget: FleetBudget, spend: Spend) Verdict;

  /// null on any DB fault — the caller fails open explicitly. No allocator: the
  /// two sums come back as scalars.
  pub fn spendForFleet(pool: *pg.Pool, workspace_id: []const u8,
                       fleet_id: []const u8, now_ms: i64) ?Spend;

  /// Renew-side: the fleet's stored budget + both windowed sums in one statement.
  /// `BudgetError.UnreadableBudget` on a malformed stored budget (fail closed);
  /// propagates DB errors (caller fails open); null when the fleet row is gone.
  pub fn fetchBudgetAndSpend(conn: *pg.Conn, alloc: Allocator,
                             fleet_id: []const u8, workspace_id: []const u8,
                             now_ms: i64) !?struct { budget: FleetBudget, spend: Spend };

src/lib/common/clock.zig
  pub fn startOfUtcMonthMillis(now_ms: i64) i64;   // pure; no wall-clock read

src/lib/contract/execution_result.zig
  pub const FailureClass = enum { …, renewal_terminate, budget_breach };

src/agentsfleetd/fleet/event_rows.zig
  pub const LABEL_BUDGET_BREACH = "budget_breach";

src/agentsfleetd/errors/error_registry.zig
  pub const ERR_RUN_BUDGET_EXCEEDED = "UZ-RUN-015";    // 402, the /renew refusal
  pub const ERR_EXEC_BUDGET_BREACH  = "UZ-EXEC-015";   // the runner-engine mirror

src/runner/daemon/control_plane_client_renew.zig   (re-exported by the parent)
  pub const TerminalRenew = struct { status: u16,
                                     reason: FailureClass = .renewal_terminate };
  pub const RenewResult = union(enum) { renewed: i64, terminal: TerminalRenew };

src/runner/child_supervisor_read.zig
  pub const RenewDecision = union(enum) { keep, extend: i64, terminate: FailureClass };

src/runner/child_supervisor_result.zig
  pub const ReadOutcome = struct { …, terminated: bool = false,
                                   terminate_reason: FailureClass = .renewal_terminate };

HTTP surface: no new route, no new verb. POST /v1/runners/me/leases/{lease_id}/renew
gains one refusal cause (402, error_code UZ-RUN-015). Its OpenAPI entry is an
allowlisted internal control-plane route (M122_001 §3), so no spec regeneration.
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Fleet at its daily ceiling wakes again | webhook/cron/steer arrives after spend ≥ `daily_dollars` | lease gate refuses before the receive debit; event row `gate_blocked` + `budget_breach`; no charge, no run (§1.6) |
| Long run crosses its ceiling mid-flight | tokens billed at each `/renew` push spend past the cap | next `/renew` answers 402 `UZ-RUN-015`; runner kills the child; report writes `fleet_error` + `budget_breach` (§2.1, §3.4) |
| Database fault while reading spend | pool acquire fails, or the SUM errors | gate **fails open** (admits the event / renews), logs `error_code`; mirrors `balanceCoversEstimate` — a metering outage must not halt every fleet (§1.7, §2.4) |
| Stored budget is unparseable | a `budget` key IS present in `config_json` and its value is nonsense (`{"daily_dollars": "five"}`) | renew gate refuses (fail-closed on *invalid data*, distinct from fail-open on *unavailable data*): a ceiling we cannot read is not a ceiling we may ignore (§2.3) |
| Fleet declares **no** budget at all | a `core.fleets` row whose `config_json` carries no `budget` key — a fixture, or any row written by a path that bypasses `parseFleetConfig` | renew gate **admits**. "No ceiling declared" is not "ceiling we cannot read": undeclared is unbounded, exactly as before this gate existed, and the tenant credit pool still bounds it. Refusing would kill healthy in-flight runs to enforce a limit nobody wrote (§2.5) |
| Old runner, new daemon | runner never sends `budget_breach` | `failure_reason: ?FailureClass = null` — the daemon records the coarse `fleet_error`; no parse error (§3.3) |
| Refusal body unparseable at the runner | truncated 402 body | `classifyRenew` falls back to `.renewal_terminate`; the run still dies, only the label is coarser (§3.2) |
| Re-delivered stream entry for a blocked event | Pending Entries List (PEL) redelivery after `gate_blocked` | `markBlocked` is guarded on `status = received` → 0 rows affected, no second block, no second charge (RULE IDMP) |
| Clock skew across the month boundary | `now_ms` near month start | `startOfUtcMonthMillis` is a pure function of the passed `now_ms`; both sums derive from one `now_ms` read per gate call, so day and month windows can never disagree (§1.4) |

## Invariants

1. A fleet's admitted spend in any rolling 24 hours is bounded by `daily_dollars` plus at most one renewal window's worth of overshoot — enforced by the two gates, not by review discipline (§1.6, §2.1).
2. A refused event is never charged: the budget gate runs strictly before `debitReceive` — enforced by call order in `runBilling` and asserted by `test_lease_gate_blocks_over_budget_fleet` (no telemetry receive row).
3. The ceiling that admits a run and the ceiling that kills it are the same number, read through the same parser (`config_helpers.parseFleetBudget`) — enforced by §2's reuse, not by a duplicated literal.
4. Both windows derive from a single `now_ms` per gate invocation, passed in — enforced by `startOfUtcMonthMillis` and `spendForFleet` taking `now_ms` as a parameter with no internal `clock.nowMillis()` call (RULE TIM).
5. Every `gate_blocked` write is idempotent under stream redelivery — enforced by `markBlocked`'s `WHERE status = $6` guard.
6. `FleetConfig.budget` has at least one production reader — enforced by `test_fleet_budget_field_has_production_reader` (RULE DFS).
7. A budget refusal is distinguishable from a credit refusal in the durable record — enforced by `failure_label = 'budget_breach'` vs `'renewal_terminate'` / `'balance_exhausted'` (§3.4).

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| `failure_label = 'budget_breach'` on `core.fleet_events` | `event_rows.markBlocked` / `markTerminal` | a fleet is refused at the lease gate, or killed at `/renew`, for exceeding its own ceiling | `fleet_id`, `event_id`, `status`, `failure_label` | no dollar amounts, no tenant identifiers in the label | `test_lease_gate_blocks_over_budget_fleet`, `test_report_persists_budget_breach_label` |
| `metrics_runner.incRunnerFailure(runner_id, .budget_breach)` | `service_report.zig:123` | a budget-killed run is reported | `runner_id`, `failure_reason` | in-memory, per-runner; no fleet identity | `test_classify_maps_budget_refusal_to_budget_breach` |
| `lease_budget_breach` / `renew_budget_breach` log lines | `service_billing.zig` / `service_renew.zig` | each refusal | `error_code`, `fleet_id`, `event_id` | spend figures logged at debug only, never at warn | LOGGING gate (`audits/logging.sh`) |

No existing signal is renamed or removed. No analytics/funnel playbook changes — `budget_breach` is an operator signal, not a product-funnel event.

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit | `test_budget_covers_daily_boundary` | cap `$1.00` (1e9 nanos): spend 999_999_999 → `.ok`; 1e9 → `.day_exceeded`; 1e9+1 → `.day_exceeded` |
| 1.2 | unit | `test_budget_absent_monthly_is_unlimited` | `monthly_dollars = null`, `month_nanos = 1e15`, `day_nanos = 0` → `.ok` |
| 1.3 | unit | `test_budget_dollars_to_nanos_rounds_and_saturates` | `1.0 → 1_000_000_000`; `0.000000001 → 1`; `1000.0 → 1_000_000_000_000` |
| 1.4 | unit | `test_clock_start_of_utc_month` | `2026-07-10T16:04:00Z` → `2026-07-01T00:00:00Z`; `2024-02-29T23:59:59.999Z` → `2024-02-01T00:00:00Z`; exact month start → itself |
| 1.5 | integration | `test_spend_for_fleet_windows_and_scopes` | rows at `now-23h` and `now-25h` for fleet A, one for fleet B → `day_nanos` counts only A's 23h row |
| 1.6 | integration | `test_lease_gate_blocks_over_budget_fleet` | seed spend = cap → next lease attempt → `fleet_events.status='gate_blocked'`, `failure_label='budget_breach'`, and **zero** new `charge_type='receive'` telemetry rows |
| 1.7 | integration (fault injection) | `test_lease_gate_fails_open_on_db_fault` | telemetry table dropped/unreachable → event proceeds, `error_code` logged |
| 2.1 | integration | `test_renew_refuses_over_budget_lease` | over-budget fleet → `POST /renew` → 402, body `error_code='UZ-RUN-015'`, `lease_expires_at` unchanged |
| 2.2 | integration (regression) | `test_renew_under_budget_unchanged` | under-budget fleet → 200 + extended deadline, byte-identical response shape to today |
| 2.3 | integration (negative) | `test_renew_invalid_stored_budget_refuses` | `config_json` budget set to `{"daily_dollars": -1}` → 402, not 200 |
| 2.4 | integration (fault injection) | `test_renew_fails_open_on_db_fault` | budget query errors → 200 renewed, `error_code` logged |
| 3.1 | unit | `test_classify_maps_budget_refusal_to_budget_breach` | `ReadOutcome{terminated=true, terminate_reason=.budget_breach}` → `ExecutionResult{exit_ok=false, failure=.budget_breach}` |
| 3.2 | unit (negative) | `test_classify_unknown_refusal_stays_renewal_terminate` | 402 body `{"error_code":"UZ-RUN-012"}` and a truncated body → `.renewal_terminate` |
| 3.3 | unit | `test_read_outcome_terminate_reason_defaults` | a scripted hook returning `.terminate` with no reason → `.renewal_terminate` |
| 3.4 | integration | `test_report_persists_budget_breach_label` | report `{outcome: fleet_error, failure_reason: budget_breach}` → event row `failure_label='budget_breach'` |
| 3.5 | unit | `test_failure_class_budget_breach_tag_name` | `FailureClass.budget_breach.label()` == `"budget_breach"` |
| 4.1 | unit (grep) | `test_fleet_budget_field_has_production_reader` | `.budget.` appears in a non-test `src/**/*.zig` |
| 4.2 | unit | `test_parse_fleet_budget_daily_bound_unchanged` | `daily_dollars = 1000.0` accepted; `1000.1` → `InvalidBudget` |

Regression: §2.2 proves an under-budget renewal is unchanged. Idempotency/replay: the redelivered-entry case is covered by `markBlocked`'s guarded transition (Invariant 5) and asserted in §1.6's second delivery.

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | The dead field is live (§4) | `grep -rn "config\.budget" src/ --include='*.zig' \| grep -v _test \| wc -l` | output ≥ 1 | P0 | ✅ `1` — `service_billing.zig:96` |
| R2 | Both gates + the pure math pass (§1/§2) | `make test` | exit 0 | P0 | |
| R3 | Both gates behave against real Postgres (§1/§2/§3) | `make test-integration` | exit 0 | P0 | |
| R4 | The budget label reaches the durable record (§3) | `grep -rn "budget_breach" src/agentsfleetd/fleet/event_rows.zig src/lib/contract/execution_result.zig` | both files match | P0 | |
| R5 | The new error code is registered in both halves | `grep -c "UZ-RUN-015" src/agentsfleetd/errors/error_entries.zig src/agentsfleetd/errors/error_registry.zig` | each ≥ 1 | P0 | |
| R6 | The misnamed dollar bound is gone (§4) | `grep -c "MS_PER_SECOND" src/agentsfleetd/fleet_runtime/config_helpers.zig` | output = 0 | P1 | |
| R7 | No connection leaks on the new query paths | `make _lint_zig_pg_drain` | exit 0 | P0 | |
| R8 | Architecture doc names the per-fleet ceiling (§4) | `grep -c "daily_dollars" docs/architecture/billing_and_provider_keys.md` | output ≥ 1 | P0 | |
| R9 | Cross-compiles for both runner targets | `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` | exit 0 | P0 | |
| R10 | No leaks under the error paths | `make memleak` | exit 0, evidence pasted into PR Session Notes | P0 | |
| S1 | Lint clean | `make lint-all` | exit 0 | P0 | |
| S2 | No secrets | `gitleaks detect` | exit 0 | P0 | |
| S3 | No oversize source file | `git diff --name-only origin/main \| grep '\.zig$' \| xargs wc -l 2>/dev/null \| awk '$1>350 && $2!="total"'` | no output | P0 | |

**Grading protocol (VERIFY):** run the Verify command verbatim; grade ONLY from its output. Graded = ✅/❌ + the one decisive output line. **Ship gate:** every row graded, every P0 ✅ → eligible for CHORE(close); any ❌ or empty cell → return to EXECUTE; a P1 ❌ ships only with an Indy-acked deferral quote in Discovery.

## Dead Code Sweep

**1. Orphaned files — deleted from disk and git.**

N/A — no files deleted; this workstream creates three and edits seventeen.

**2. Orphaned references — zero remaining imports/uses.**

| Deleted symbol/prose | Grep | Expected |
|-----------------------|------|----------|
| `MS_PER_SECOND` (misnamed dollar bound in the config parser) | `grep -c "MS_PER_SECOND" src/agentsfleetd/fleet_runtime/config_helpers.zig` | 0 matches |
| `RenewResult.terminal` as a bare `u16` | `grep -rn "terminal: u16" src/runner/` | 0 matches |
| `applyTick` returning bare `bool` | `grep -rn "fn applyTick.*) bool" src/runner/` | 0 matches |

Note: `MS_PER_SECOND` remains a legitimate, genuinely-milliseconds constant elsewhere — `auth/jwks.zig`, `auth/oidc.zig`, `state/fleet_events_filter.zig`, and `fleet_runtime/approval_gate.zig` (a 24-hour gate timeout). The sweep is scoped to `config_helpers.zig`, the one file where the name bounded dollars.

## Out of Scope

- **Projected-cost admission** — refusing a run whose *estimated* total would breach the cap. Assumption 1 fixes the semantics at an admitted-while-under check; a predictive gate is a separate product decision.
- **Workspace- or tenant-wide dollar budgets** — `~/Projects/docs/billing/budgets` describes those separately, and the tenant credit pool already bounds them.
- **Surfacing budget state on any HTTP or Command-Line Interface (CLI) surface** — no `GET /v1/.../budget`, no `agentsfleet budget` verb, no dashboard tile. The operator reads `failure_label` on the event. A read surface is a separate workstream.
- **Backfilling or migrating `config_json`** — `daily_dollars` is already a required field, so every stored fleet has one.
- **Changing the `(0, 1000]` daily and `(0, 10000]` monthly validation bounds** — §4 renames the constant that expresses the daily bound; it does not move it. Whether $1000/day is the right ceiling is Indy's product call, not this workstream's.
- **Retiring the ingest-only `EVENT_TYPE_CONTINUATION`** — M122_001's Out of Scope, unchanged.

---

## Product Clarity (authoring record)

1. **Successful user moment** — a fleet author writes `daily_dollars: 5.0`, a bug sends their fleet into a tool-call loop overnight, and they wake to a `gate_blocked` event labelled `budget_breach` and a $5 charge — not a drained credit pool. The number they wrote is the number that held.
2. **Preserved user behaviour** — every under-budget fleet behaves byte-identically: same lease, same renewals, same report, same billing. The gates only fire past a ceiling that today fires never. No endpoint changes shape; `/renew` gains one refusal cause on a status it already returned.
3. **Optimal-way check** — the two gates reuse the two places the platform already asks "may this proceed?", and the spend figure is a windowed sum over a table that already exists and is already indexed for it. No new table, no new column, no counter to keep in sync, no cache to invalidate.
4. **Rebuild-vs-iterate** — iterate. The dead `FleetBudget` field, the parser, the validation bounds, and the terminal-label transport all already exist; this workstream connects them.
5. **What we build** — `budget.zig` (pure math + one windowed query), two gate call sites, one error code, one `FailureClass` variant, and the reason-plumbing that lets the label survive the trip from the refusal to the event row.
6. **What we do NOT build** — projected-cost admission, a budget read API, workspace/tenant budgets, a dashboard surface, or any change to the validation bounds (all in Out of Scope).
7. **Fit with existing features** — the tenant credit pool (`balance_exhausted`) bounds what a *tenant* can spend; the per-fleet budget (`budget_breach`) bounds what *one fleet* can spend inside that. The two gates are independent and both must pass; neither weakens the other.
8. **Surface order** — daemon and runner land together in one PR (the wire carries a new `FailureClass`); the architecture doc lands with them; M122_001's user-facing docs describe the result.
9. **Dashboard restraint** — no UI. The signal is a `failure_label` an operator greps, plus the existing per-runner failure bucket. Building a budget widget before anyone has hit a budget would be inventing demand.
10. **Confused-user next step** — an operator seeing `budget_breach` reads the label, checks the fleet's `TRIGGER.md`, and raises `daily_dollars` or fixes the loop. `~/Projects/docs/fleets/troubleshooting.mdx` §5 already anchors that path; M122_001 §1 makes its prose true.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** four Sections — the pure module + pre-run gate (§1), the mid-run gate (§2), the reason plumbing that makes the label honest (§3), and the dead-field/misnomer cleanup the first three earn (§4). Each is independently testable; §3 is the only one that touches the runner.
- **Alternatives considered:**
  - *(a) Pre-run gate only.* Rejected: an in-flight run would never be stopped, so a single runaway run could spend without bound, and the docs' "terminates the in-flight event cleanly" would still be false. Indy chose both gates explicitly.
  - *(b) Reuse `renewal_terminate` for the mid-run kill, skipping §3.* Tempting — it removes six runner files from the diff. Rejected: `renewal_terminate` already means "lease lost, max-runtime, or credit exhausted," so a budget stop would be indistinguishable from a billing failure in the durable record, and no operator could answer "did my budget hold?" from the event row. Indy chose the granular label.
  - *(c) Pin the resolved ceiling onto `fleet.runner_leases` at lease issue.* Would let §2 read the budget from the lease it already loads, at the cost of a schema migration and of making a mid-run budget change take effect only on the next run. Rejected: a cost ceiling should bite as soon as an operator lowers it, and avoiding a migration keeps the blast radius to Zig.
  - *(d) Compute spend from `fleet.metering_periods` instead of `core.fleet_execution_telemetry`.* Rejected: `metering_periods` is the per-renewal drill-down keyed by `event_id` with no `fleet_id` column, so a fleet-scoped window would need a join; the telemetry table carries `fleet_id` and the exact index this query wants.
- **Patch-vs-refactor verdict:** a **patch** with one narrow widening. Nothing is restructured; one union variant, one struct field, and one enum variant are added so an existing signal can carry one more meaning.

## Discovery (consult log)

- **Regression caught by the existing suite (Jul 10 2026)** — the first `make test-integration` run after wiring the mid-run gate failed `service_token_splits_wire_test`: `/renew` answered `402 UZ-RUN-015` where it had answered `200`. That fixture seeds `config_json = "{}"` — a fleet with **no declared budget** — and `fetchBudgetAndSpend` mapped the resulting SQL `NULL` onto `BudgetError.UnreadableBudget`, i.e. fail-closed. That was not a test problem. It was a production hazard: every `core.fleets` row whose `config_json` lacks a `budget` key would have had its in-flight runs killed to enforce a ceiling nobody wrote. `parseFleetConfig` requires `budget`, so live fleets have one — but nothing guarantees that of rows written by other paths, and "no ceiling declared" is not "ceiling we cannot read". The read now classifies into `.found` / `.absent` / `.unreadable` / `.unavailable`, and only `.unreadable` refuses. The failing test is why the distinction exists.

- **Design change during EXECUTE** — the fail-open/fail-closed decisions originally lived inside `catch` blocks beside a live connection, which made Dimensions 1.7, 2.3 and 2.4 reachable only with a database fault to inject. They were extracted into two pure functions (`verdictOrAdmit`, `refusalFor`) over an explicit `BudgetRead` union. The asymmetry is now one switch, unit-tested without a database, and the call sites read as policy rather than as error handling.

- **Consults** — Architecture (`dispatch/name_architecture.md`, Jul 10 2026): read `docs/architecture/billing_and_provider_keys.md` before naming the flow. It defines posture and the credit pool as tenant-scoped and never mentions a per-fleet ceiling, so this workstream introduces the concept and carries the architecture-doc diff (§4). No naming conflict with `balance_exhausted` (tenant credit) or `renewal_terminate` (lease policy). Legacy-Design Consult Guard: not triggered — no legacy machinery is patched or retained; the dead field is made live.
- **Gate-flag triage** — one flag raised during M122_001's golden-path walk, judgment class (weakened guarantee on a cost boundary, more than one possible fix). Escalated rather than resolved unilaterally. Indy's decisions, Jul 10 2026: build the enforcement (not delete the doc promise); enforce at both the pre-run gate and mid-run `/renew`; carry a new `budget_breach` label; author a separate spec on the same worktree and PR. A second, mechanical flag (`MS_PER_SECOND` naming a dollar bound) is auto-fixed in §4 per the mechanical-fix rule.
- **Metrics review** — one new operator signal (`failure_label = 'budget_breach'`) reusing the existing `failure_label` column and the existing per-runner failure bucket. No product/funnel event added, renamed, or removed. Declared in Metrics & Observability above.
- **Skill-chain outcomes** — `/write-unit-test`, `/review`, `kishore-babysit-prs`: pending (run at VERIFY / CHORE(close)).
- **Deferrals** — none. Every Section lands in this workstream's PR. The `(0, 1000]` daily bound is Out of Scope by design, not deferred: it is a product question, not incomplete work.
