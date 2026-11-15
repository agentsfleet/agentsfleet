# M249_004 source inventory

Every fact the refreshed screens display, where it is recorded, who may read it, and what it costs to show.
Base: `a1dcc222488e4522f8b92208a4f7df8461fe7fbf`, main containing PR #844.

## Field inventory

| Screen fact | Status | Producer and storage | Authorization | Adapter | Requests |
|---|---|---|---|---|---|
| Usage totals, daily bars, fleet and model groups | Available | `billing.usage_daily` and `billing.runs_daily`, maintained by the settle transaction and read by `afd_billing/src/report/sql/aggregates.sql` | `billing:read`, tenant from the session | `lib/api/billing_report.ts` | One report request |
| Model display names | Available | `model.platform_models.display_name`, through `GET /v1/models` | Any signed-in person; the catalogue is platform-wide | `lib/model-labels{,-server}.ts`, `ModelLabelsProvider` | One catalogue walk per sign-in or refresh, in the dashboard layout |
| API key display suffix | Available | `identity.api_keys.display_suffix`, written at creation (M249_003 §5.6) | `apikey:read` | `lib/api/api_keys.ts` | One bounded page |
| API key last use | Producer missing | None: authentication is one read-only lookup (`docs/AUTH.md:760-762`) | — | Shows "Last used: unavailable" | — |
| Secret "Available to" | Backend projection | `fleet.revisions.definition` → `{config,x-agentsfleet,credentials}` of each active fleet's head revision and every unfinished task's accepted revision; the list `afd_credential::secrets::declared` resolves | `secret:read`, plus `fleet:read` for the names; otherwise omitted | `fleetAccessOf` in `lib/api/secrets.ts` | The secret list and one access read, in parallel |
| Plugin card revision number | Backend projection | `authoring.plugin_revisions.revision`, already selected by `afd_authoring/src/plugin/sql/page.sql:8` | `fleet:read` on the workspace | `PluginCardSchema` in `lib/api/draft-plugin.ts`; the Plugin UI stays disabled | No extra request |
| Summary changed files | Available | Typed `args.path` of succeeded write tools (`recorded-changes.ts:5-19`) | Fleet read | Existing | Existing |
| Summary line totals | Producer missing | Write tools report bytes only (`afr_tools/src/sandbox/files/mod.rs:164-262`) | — | Says totals are not available | — |
| Summary published outputs | Available for existing pull requests | `fleet.task_effects` receipt with a confirmed commit (`afd_fleet/src/task/sql/read.sql:24-31`) | Fleet read | `WorkOutputs` requires a confirmed receipt on the accepted target | Existing |
| Pull request created by a run | Producer missing | No receipt is stored for `POST /pulls` | — | Not shown | — |
| Context budget | Parked | See `docs/v2/done/M249_004_SECTION_5_MEASURED_CONTEXT_BUDGET.md` | — | Unavailable | — |

## Usage request count

| Load | Before | After |
|---|---|---|
| Usage-specific reads on first load | Report and catalogue in `Promise.all`; the page waited for both | One report request |
| Model names | First catalogue page, non-advanced models only, keyed by identifier alone | Complete catalogue, once per sign-in or refresh, shared by every screen |
| Activity list and charge lines | Activity page in every report; charges when a row is opened | Removed with their route; redesigned later |
| Page frame | Waited for every read | Header and actions stream first; the data region follows under `Suspense` |

The shell's own reads (workspaces, balance, invites, and now model labels) are common to every dashboard page and are not Usage requests.

The earlier M249_003 controlled review measured the old loader dependency.
The report answered in 40 ms; the page waited 1,217 ms for a catalogue delayed by 1,200 ms.
Those are injected local delays, not live machine timings.

## Query plans

These are historical measurements of the earlier aggregate optimization.
Claude reverted that optimization in `431b2ab62`; the current parent `ab4fe8937` uses the prior scoped-row statement.
The timings below do not establish a speedup for the current SQL or measure the Usage UI.

The worktree Postgres 19 held 2,240,000 settled lines.
One account's month contained 224,000 across six fleets, setup work and ten models.
Times are `EXPLAIN (ANALYZE, BUFFERS)` runtimes in milliseconds.

| Statement | Runs | Times (ms) |
|---|---|---|
| Aggregate before | 6, then 6 | 1,052 / 1,076 / 1,066 / 1,041 / 1,019 / 1,099; 1,027 / 1,001 / 1,006 / 1,046 / 1,006 / 1,065 |
| Aggregate after (grouped by work, fleet, model and day first) | 6, then 6, interleaved | 409 / 466 / 452 / 399 / 399 / 399; 403 / 416 / 388 / 393 / 411 / 433 |

The machine's load average was 4.6–5.4 during both rounds; interleaving the two statements controls for it.

The earlier plan scanned the account's lines in 300 ms.
It then externally sorted all 224,000 rows three times for grouping sets and distinct aggregates.

Grouping first passes grouped rows to the grouping sets.
Filters and grouping sets still key on those grouped columns, so sums stay unchanged.

Each full result contained 48 rows.
`EXCEPT ALL` found zero differences in both directions.
Checks covered unfiltered reads and fleet, model and setup-only filters.

## Label design

An earlier design put `model_labels` in the report.
The owner moved names into a shared store and required names before the table paints.

The layout starts one server label read per sign-in or refresh and forwards its promise through the provider.
Next retains the layout across client navigation (`next/dist/docs/01-app/02-guides/authentication.md:1358`).

The frame, report totals and graph stream while the fleet table waits for names.
This preserves the shared read and avoids a visible identifier/name swap.

An identifier gets a name only when every catalogue listing of it agrees on one nonempty name. Retired models are absent from the priced catalogue and show their recorded identifier.

## Load simulation and the daily rollup

A throwaway Postgres 19rc2 container held the real `schema/*.sql` and three accounts. Every fleet billed every 15 minutes through November 2026, with two model lines and one runtime line per run:

| Account | Fleets | Lines in the month | `usage_daily` rows | `runs_daily` rows |
|---|---|---|---|---|
| Typical | 10 | 86,400 | 900 | 300 |
| Mid | 100 | 864,000 | 9,000 | 3,000 |
| Large | 1,000 | 8,640,000 | 90,000 | 30,000 |

The ledger held 5,545 MB, the rollup 28 MB. Rebuilding the rollup from all 9.59M settled lines took 51 s.

Report reads, warm, `EXPLAIN (ANALYZE)` duration:

| Statement | 10 fleets | 100 fleets | 1,000 fleets |
|---|---|---|---|
| Raw aggregates, main's statement | ~465 ms | ~5,030 ms | ~87,900 ms |
| Raw aggregates, the pre-group rewrite (reverted) | ~555 ms | — | ~87,000 ms |
| Raw aggregates, a hash-only prototype | ~87 ms | ~1,060 ms | ~24,000 ms |
| Activity page (now removed) | ~134 ms | — | ~20,000 ms |
| Daily rollup read | ~1.2 ms | ~10 ms | ~170–260 ms |
| Secret access (1,000 fleets × 10 secrets) | — | — | ~12 ms |

One plain pass over the large account's month took 8,545 ms cold. Most of the raw statement's time went to `COUNT(DISTINCT …)` and `ARRAY_AGG(DISTINCT …)` sorting and spilling inside its grouping sets.

Equivalence used `EXCEPT ALL` in both directions.
The prototype matched main's statement at 45 and 1,035 rows; the rollup read matched at 135 rows (100 fleets).
Each found 0 differences.

Settlement, `pgbench`, 8 clients for 30 s. Each transaction inserted a run's 3 lines, then settled, rolled up and debited:

| Shape | Without rollup | With rollup |
|---|---|---|
| Every client on one wallet | 421 tx/s, 19.0 ms | 371 tx/s, 21.6 ms |
| 1,000 fleets across 100 wallets | 848 tx/s, 9.4 ms | 1,104 tx/s, 7.2 ms (run-order noise) |
| One client, serial | ~240 tx/s, ~4.2 ms | ~190–250 tx/s, ~4.0–5.2 ms |

Write-ahead log per transaction was ~11–12 KB either way. No transaction failed, so no deadlock aborted any of the ~44,700 runs.

Adversarial checks against the settle statement found that a duplicate pass settled 0 and added nothing.
A rolled-back settle left nothing; a run across midnight split its money by day and counted once.

Two relays settling different lines of one run at the same moment kept money exact.
They counted the run twice when the count keyed on "the first batch".
Counting on the run's smallest runtime slice, which only one transaction can settle, removes that without a lock.

The integration suite repeats every one of these checks.

