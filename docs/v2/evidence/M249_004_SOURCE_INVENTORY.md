# M249_004 source inventory

Every fact the refreshed screens display, where it is recorded, who may read it, and what it costs to show.
Base: `a1dcc222488e4522f8b92208a4f7df8461fe7fbf`, main containing PR #844.

## Field inventory

| Screen fact | Status | Producer and storage | Authorization | Adapter | Requests |
|---|---|---|---|---|---|
| Usage totals, daily bars, fleet and model groups | Available | `billing.usage_lines`, read by `afd_billing/src/report/sql/aggregates.sql` in one read-only snapshot | `billing:read`, tenant from the session | `lib/api/billing_report.ts` | One report request |
| Usage activity page | Available | `afd_billing/src/report/sql/activities.sql`, same snapshot | Same | Same | Same request |
| Usage charge lines | Available | `afd_billing/src/report/sql/charges.sql` | Same; foreign work answers an empty page | `listBillingActivityCharges` | One request, only for an activity the page opens; none on the first load |
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
| Charge lines | Requested when a row is opened | Unchanged; never on the first load |
| Page frame | Waited for every read | Header and actions stream first; the data region follows under `Suspense` |

The shell's own reads (workspaces, balance, invites, and now model labels) are common to every dashboard page and are not Usage requests.

The controlled loader review in M249_003 measured the old dependency: the report answered in 40 ms and the page waited 1,217 ms behind a catalogue delayed by 1,200 ms. Those are injected local delays, not live machine timings.

## Query plans

Measured on a worktree Postgres 19 with 2,240,000 settled lines, 224,000 of them in one account's month, across six fleets, setup work and ten models. Times are `EXPLAIN (ANALYZE, BUFFERS)` execution times in milliseconds.

| Statement | Runs | Times (ms) |
|---|---|---|
| Aggregate before | 6, then 6 | 1,052 / 1,076 / 1,066 / 1,041 / 1,019 / 1,099; 1,027 / 1,001 / 1,006 / 1,046 / 1,006 / 1,065 |
| Aggregate after (grouped by work, fleet, model and day first) | 6, then 6, interleaved | 409 / 466 / 452 / 399 / 399 / 399; 403 / 416 / 388 / 393 / 411 / 433 |

The machine's load average was 4.6–5.4 during both rounds; interleaving the two statements controls for it.

The earlier plan showed where the time went: a 300 ms scan of the account's lines, then three external-merge sorts of all 224,000 rows for the grouping sets and the distinct aggregates. Grouping first hands the grouping sets a fraction of the rows; every filter and grouping set keys on the grouped columns, so the sums cannot change.

Equivalence: `EXCEPT ALL` in both directions over the full result returned 48 rows each and 0 differences, unfiltered and under a fleet filter, a model filter and a setup-only filter.

## Label design

The first design added `model_labels` to the billing report. The owner moved names to one shared store so billing reports billing alone, and asked that names never visibly swap after paint. The layout therefore starts one label read on the server per sign-in or refresh and forwards the pending result through its provider; Next keeps the layout across client navigation (`next/dist/docs/01-app/02-guides/authentication.md:1358`).

The visual follow-up lets the frame, report totals and graph stream while the fleet table waits for names; it preserves the shared read and avoids a visible identifier/name swap.

An identifier gets a name only when every catalogue listing of it agrees on one nonempty name. Retired models are absent from the priced catalogue and show their recorded identifier.
