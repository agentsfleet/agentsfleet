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

# M154_001: Rebuild the schema from empty — one identity key, money behind foreign keys, payload-free list reads

**Prototype:** v2.0.0
**Milestone:** M154
**Workstream:** 001
**Date:** Jul 31, 2026
**Status:** IN_PROGRESS
**Priority:** P1 — the events list detoasts full payloads on every page today, and a public endpoint ships with no consumer
**Categories:** API, DOCS, SQL, UI
**Batch:** B1 — single stream; the rebuild is atomic by construction
**Branch:** feat/m154-schema-rebuild
**Test Baseline:** unit=3344 integration=510
**Depends on:** M149_001 (landed) — shipped slots 043–046, the runner retention sweeper, and the lifetime-counter table whose single-unique-index shape §2 generalises
**Provenance:** LLM-drafted (Claude Opus 5, Jul 31, 2026), from a live audit of all 45 schema slots and their call sites
**Canonical architecture:** `docs/architecture/scaling.md` §Which recurring Postgres reads are index-served · `docs/architecture/billing_and_provider_keys.md`

---

## Overview

**Goal (testable):** A database bootstrapped from empty exposes exactly one `id UUID PRIMARY KEY` per table, resolves every money row to a tenant through a foreign key, and serves an events page without reading a single event body.

**Problem:** Three symptoms, all observable today. Opening the events list ships up to 200 full event payloads and 200 full agent answers to render a table of timestamps and costs — so the page slows in proportion to how chatty the agents are, not how many rows exist. Erasing an account depends on a hand-maintained fourteen-statement delete order, because the money rows carry no foreign key to a tenant; a table added without one is silently missed. And a public billing endpoint returns per-renewal accrual detail that no product surface has ever called.

**Solution summary:** The dev database is torn down, so the schema is re-authored from empty rather than patched. Every table drops the `uid` plus duplicate-twin pattern for a single `id UUID PRIMARY KEY`; the money tables consolidate into `billing` behind real foreign keys so erasure becomes a cascade; the unread accrual table and its endpoint are removed rather than carried; the events list stops selecting bodies and gains a single-event detail read for the expand interaction. Slots are renumbered by dependency layer and the fourteen patch-only slots fold into the base statements they patch. M149's runner retention sweeper is **preserved verbatim** — it is the one retention policy that already exists and it keeps its behaviour, window, and comptime proof. Partitioning is deliberately **not** built; only the stable key that keeps it available later.

## PR Intent & comprehension handshake

- **PR title (eventual):** `refactor(m154): rebuild schema from empty — single identity key, money behind FKs`
- **Intent (one sentence):** The events list stops paying for payloads it does not render, account erasure stops depending on a hand-maintained list, and the schema a reader opens says what it is.
- **Handshake** — the implementing agent fills this at PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `~/Projects/dotfiles/docs/SCHEMA_CONVENTIONS.md` — the conventions this spec **amends**: the `uid` rule and the migration model both change. Read it before authoring any statement, and amend it in the same landing.
2. `src/agentsfleetd/cmd/common.zig` — the migration array plus the named slot-version constants renumbering invalidates (RULE MIG).
3. `src/agentsfleetd/state/fleet_events_store.zig` and `account_teardown.zig` — the list query whose select list carries the payload columns, and the hand-maintained delete order the foreign-key work shrinks.
4. `docs/architecture/scaling.md` §Which recurring Postgres reads are index-served — the standard this repository already holds indexes to: asserted against the plan, not merely created.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `schema/001_*.sql` … `schema/046_*.sql` | DELETE | All 45 shipped slots retire; the rebuild re-authors from empty |
| `schema/1*.sql` … `schema/8*.sql` | CREATE | Renumbered by dependency layer, gaps of 10; patch-only slots folded into what they patched |
| `schema/embed.zig`, `src/agentsfleetd/cmd/common.zig` | EDIT | The slot list, the migration array, and the named slot-version constants (RULE MIG) |
| `src/agentsfleetd/state/*.zig` | EDIT | Every store selecting `uid` or a renamed money table; `account_teardown.zig`'s delete order shrinks to what no cascade covers |
| `src/agentsfleetd/state/fleet_metering_store.zig` | DELETE | The accrual read retires with its table |
| `src/agentsfleetd/fleet/*.zig` | EDIT | Lease, renewal, settle and reclaim statements touching renamed tables and the dropped lease payload column |
| `src/agentsfleetd/http/handlers/**` | EDIT | Accrual endpoint dropped (`tenant_billing.zig`); list loses bodies and gains a detail read (`fleets/events.zig`) |
| `src/agentsfleetd/db/test_fixtures*.zig` | EDIT | Fixtures follow the real schema (RULE ITF, RULE TFX) |
| `public/openapi.json` | EDIT | Accrual endpoint removed; event detail added |
| `ui/packages/app/**` | EDIT | The dialog fetches the body on expand instead of reading it off the list row; accrual client retired |
| `schema/530_fleet_keys.sql` | DELETE | The external-caller credential retires with its surface (§8) |
| `src/agentsfleetd/http/handlers/integration_grants/handler.zig` | EDIT | Request route, handler-local authentication and its session arm removed (§8) |
| `src/agentsfleetd/http/handlers/fleets/create.zig` | EDIT | Install seeds the pending grant and raises the approval gate (§8) |
| `src/agentsfleetd/http/handlers/webhooks/grant_approval.zig` | DELETE | Duplicate approval path; the gate webhook already resolves decisions (§8) |
| `src/agentsfleetd/fleet_runtime/notifications/grant_notifier.zig` | DELETE | Its notification and nonce belong to the removed path (§8) |
| `src/agentsfleetd/fleet/service.zig` | EDIT | The lease parks on a missing grant instead of dropping the credential (§8) |
| `src/agentsfleetd/http/handlers/api_keys/*.zig` | EDIT | Fleet-key management retires; tenant-key management stays (§8) |
| `docs/architecture/connectors.md` | EDIT | Records install-time origination and the retired external surface |
| `docs/architecture/data_flow.md` | EDIT | Records the list/detail split and the retained partition option |
| `public/openapi/root.yaml`, `public/openapi/paths/*.yaml`, `public/openapi/components/schemas.yaml` | EDIT | **The bundle source.** `public/openapi.json` is generated (`redocly bundle public/openapi/root.yaml`), so an edit made only to the artefact is reverted by the next `make check-openapi`. The retired request route leaves here (§8) |
| `public/skill.md`, `public/llms.txt`, `public/agentsfleet-manifest.json` | EDIT | Published surface indexes; the retired accrual row (`get_tenant_metering_periods`) leaves with its endpoint (§4) |
| `src/agentsfleetd/errors/error_entries_runtime.zig` | EDIT | Two error hints pointed at routes this milestone retired — `integration-requests` (§8) and `fleet-keys` (§8) |
| `src/agentsfleetd/session/*.zig`, `src/agentsfleetd/http/handlers/auth/sessions*.zig` | EDIT | A caller-supplied identifier that names no session answers 404 rather than 500, and the verify outcome is released with the allocator that duped it |
| `samples/fixtures/model-library/seed.sql`, `scripts/seed-models.mjs` | EDIT | Fixture and seeder follow the renames: `uid` → `id` (§2), `*_at_ms` → `*_at` (§6.3) |
| `scripts/audit_sql.py` | CREATE | Schema audit the rebuild is verified with; found four production defects grep could not |
| `scripts/check_openapi_route_coverage_test.py`, `scripts/check_zig_test_lanes_test.py` | EDIT | Gate self-tests follow the lanes and route set this milestone changes |
| `make/test-unit.mk`, `make/test.mk` | EDIT | Coverage measured over production sources only, with the integration binary merged in, and the gate constant that scores it |
| `src/agentsfleetd/db/schema_shape_integration_test.zig` | CREATE | The catalogue-wide assertions §2.2, §3.2, §6.2 and §6.3 claim; in the integration root because that is the only binary a lane gives a live database |
| `src/agentsfleetd/integration_tests.zig` | EDIT | Registers the file above; an unregistered test file is never discovered |
| `.github/workflows/test.yml` | EDIT | The coverage job needs live datastores now that it measures the integration binary, which a job `container:` cannot reach; `workflow_dispatch` lets a branch exercise the lane before review |
| `cli/**` | EDIT | The fleet-key command surface retires (§8). **Knowingly recorded, not enumerated** — Indy's decision; see Discovery |
| `~/Projects/docs/changelog.mdx` | ~~EDIT~~ | **Not done, by Indy's decision** — see the Discovery quote. Would otherwise be required: an endpoint is removed and a public route changes status |
| `src/runner/engine/runner.zig` | EDIT | The run-failure sites stop collapsing the true error, so the failure detail names a cause (§9.1) |
| `src/runner/engine/run_context_test.zig` | EDIT | A stub provider that acquires but rejects the model call — the only offline way to reach `fleet.runSingle`'s failure (§9.1) |
| `src/agentsfleetd/http/handlers/fleet/sql.zig` | EDIT | All four lease statements gain the NULL-guarded fleet predicate; `TOTAL` and `CURSOR` gain the fleets join the name match needs (§9.2–9.4) |
| `src/agentsfleetd/http/handlers/fleet/runner_leases.zig` | EDIT | Parses and threads the `fleet` filter through total, cursor and page (§9.2–9.4) |
| `src/agentsfleetd/http/runner_read_integration_test.zig` | EDIT | The fleet-filter, intersection and refusal suites (§9.2–9.4) |
| `ui/packages/app/app/(dashboard)/admin/runners/[runnerId]/components/lease-filter-query.ts` | CREATE | The filter grammar, kept free of React so parse and format are testable as the inverse pair they are (§9.5) |
| `ui/packages/app/app/(dashboard)/admin/runners/[runnerId]/components/LeaseFilterBar.tsx` | CREATE | The toolbar that writes both filters to the URL, with a chip per active filter (§9.6) |
| `ui/packages/app/app/(dashboard)/admin/runners/[runnerId]/components/LeaseWorkspaceFilter.tsx` | DELETE | Superseded by the bar; the row funnel it fed is gone (§9) |
| `ui/packages/app/app/(dashboard)/admin/runners/[runnerId]/components/LeaseTable.tsx` | EDIT | Row funnel removed, filter bar rendered (§9.6) |
| `ui/packages/app/lib/api/runners.ts` | EDIT | The `fleet` query param on the lease read (§9.2) |
| `public/openapi/paths/fleet.yaml` | EDIT | Documents the `fleet` filter and its interaction with `workspace_id` (§9.2) |
| `playbooks/operations/teardown/database/03_verify.sh` | EDIT | **Added during the close-out audit** — the table did not list it, but the diff carries it and the cause is this milestone's own role changes. Its app-role check was a hand-maintained list that had rotted: it named `ops_readonly_agent`, retired with the old product noun, while missing `billing_runtime`, `metering_runtime`, `vault_runtime` and `ops_readonly_fleet` — verifying four of eight roles while telling the operator to expect five. Now catalogue-derived, the same shape as the schema loop beside it |
| `public/openapi/paths/billing.yaml` | EDIT | `workspace_id` and `fleet_id` on the charges row become `nullable: true`. Both foreign keys are `ON DELETE SET NULL` (`schema/710`), so a charge outlives the fleet and workspace it was incurred on — the published schema said otherwise |
| `src/agentsfleetd/state/fleet_telemetry_store.zig` | EDIT | The charges row mapper: its column count is reconciled with `TELEMETRY_SELECT`, and the two nullable identity columns are read as optionals |
| `src/agentsfleetd/http/handlers/tenant_billing_integration_test.zig` | EDIT | The first two charges tests that observe a row — order plus every numeric field, and the deleted-fleet null path |

## Applicable Rules

- **`~/Projects/dotfiles/docs/greptile-learnings/RULES.md`** — **MIG** (renumbering invalidates the named slot-version constants), **STS** (value sets stay app-enforced; the partial-index predicates and trigger bodies are the surviving literals this spec must justify or remove), **NSQ** (every statement schema-qualified; unqualified `DROP INDEX` silently no-ops), **SGR** (every created table ends with grants for the roles that query it), **SCH** (pre-2.0 removal is a full teardown — no markers, no `DROP` slots), **ITF** and **TFX** (fixtures use the real schema and production constants), **KYS** (the tenant charges cursor is a composite keyset and its index must carry the tiebreak), **ORP** (renames and deletions get a cross-layer orphan sweep), **NDC** and **HLP** (the accrual reader and its endpoint go with the table), **UFS** (no repeated literals in the new statements)
- `~/Projects/dotfiles/docs/SCHEMA_CONVENTIONS.md` — amended by this spec: the `uid` identity rule and the additive-migration model both change; the amendment lands in the same commit as the statements that break the old rule
- `dispatch/write_zig.md` — every store and handler edited is Zig; pg-drain, `errdefer` placement, and cross-compile apply
- `~/Projects/dotfiles/docs/REST_API_DESIGN_GUIDELINES.md` — one endpoint removed, one added
- `dispatch/write_ts_adhere_bun.md` — the dialog and client changes

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| SCHEMA GUARD | yes — every file under `schema/` is created or deleted | `cat VERSION` (< 2.0.0 → teardown path); emit the `rm:` / `rm-embed:` / `rm-migration:` lines for the retired slots |
| ZIG GATE | yes — stores, handlers, fleet statements | pg-drain verified via `make check-pg-drain`; cross-compile both linux targets |
| PUB / Struct-Shape | yes — the accrual store's public surface is removed, the event detail row is new | shape verdict per new pub surface at PLAN |
| File & Function Length (≤350/≤50/≤70) | yes — schema files are capped at 100 lines, single-concern | one table or one logical group per file; split before the cap, never after |
| UFS | yes — new statements | named constants; no repeated literal across statements |
| UI Substitution / DESIGN TOKEN | yes — the dialog changes | design-system primitives; no arbitrary utilities |
| LOGGING / LIFECYCLE / ERROR REGISTRY | yes — the detail read needs an error code for a missing event | register the code; no bare literal messages |

## Prior-Art / Reference Implementations

- **Reference:** `schema/037_model_catalogue_revision.sql` — the house style this rebuild holds to: every constraint and every omitted index carries the reason it exists or does not. Slots that merely declare a table are the ones this milestone is replacing.
- **Reference:** `schema/033_hot_path_indexes.sql`, `041_runner_leases_operator_read_indexes.sql`, and `043_runner_lifetime_counters.sql` — the index standard (measured plans, named queries, an explicit note on what was left unindexed) and the single-unique-index shape §2 generalises.
- **Divergence:** `~/Projects/dotfiles/docs/SCHEMA_CONVENTIONS.md` is prior art this spec **overrides** on two points (identity key, migration model), argued in Decomposition and landing as a doc amendment, not a silent deviation.

## Sections (implementation slices)

### §1 — Renumber by dependency layer

Slots are ordered by chronology today, so a reader cannot tell what depends on what, and fourteen of the forty-five exist only to patch earlier ones. Renumbering by layer with gaps of 10 makes the dependency order readable and leaves room to insert without renumbering the world. **Implementation default:** layers `1xx` substrate, `2xx` identity, `3xx` secrets, `4xx` catalogue, `5xx` fleets, `6xx` runner control plane, `7xx` money, `8xx` history — because that is the order a fresh database must create them in. Numbering starts at `1xx` rather than `0xx` so that no new slot can share a name with a retired one, which makes retirement a one-glob assertion instead of a judgement call.

- **Dimension 1.1** — every patch-only slot's effect is present in the base statement it patched, and the patch slot is gone → Test `canonical schema bootstrap: every slot sits in the new numbering space` (`cmd/migration_policy_test.zig`) — **DONE**. The assertion ties "starts at 100" to "never reuses 001–046" so the two cannot drift, which is what makes "the patch slot is gone" structural rather than a spot check. That the folded effects survive is proven transitively: every integration run bootstraps from empty and the whole suite reads the resulting shape.
- **Dimension 1.2** — a slot-version constant cannot drift from the file it names, because the version **is** the slot number → Test `canonical migrations: connector install + channel tables registered` (`cmd/migration_policy_test.zig`) — **DONE**
  - **Rewritten during the close-out audit.** The original asked that "the named slot-version constants in `common.zig` resolve to the renumbered slots". Those constants no longer exist: the rebuild tied the version to the filename (`embed.zig` — "the two were independent counters before the rebuild, which is how a named slot-version constant could drift from the file it named; tying them makes that class of drift unrepresentable"). The Dimension asked for the constants to be *correct*; the implementation removed the thing that could be wrong. Marking it against the old wording would have claimed a check on a surface that is gone.
- **Dimension 1.3** — a database bootstrapped from empty applies every slot in order with no failure → Test `fresh bookkeeping: a migration applies once and a re-run is a no-op` + `runMigrations: fully-applied database is idempotent across repeated runs` (`fleet/schema_migration_test.zig`) — **DONE**. Reinforced every run: `make test-integration` migrates the test database from empty before the suite executes, so a slot that could not apply in order would fail the target before a single test ran.

### §2 — One identity key per table

Roughly twenty-two tables carry `uid` plus a duplicate twin holding the same value, costing two btree indexes on the same sixteen bytes and sixteen duplicated heap bytes per row — on the unbounded tables permanently. Nothing reads `uid`: every query selects the twin, and every foreign key points at the `UNIQUE` side.

The waste is the smaller half of the argument. The **correctness** half is recorded upstream in `schema/043_runner_lifetime_counters.sql`, which deliberately ships one unique index because a second one breaks concurrent first-touch upserts: `ON CONFLICT` arbitrates exactly one constraint, so two sessions inserting a brand-new row race to a duplicate-key error on the *other* index instead of taking the update arm. Every table still carrying the dual-key shape has that latent race. This Section generalises the decision slot 043 already made table-by-table. **Implementation default:** one column named `id`, because that is the name the API already exposes and the name every foreign key should reference.

- **Dimension 2.1** — no table declares a generated identity column or a unique constraint duplicating its primary key → Test `no table carries a second unique key over its own primary key columns` (`db/schema_shape_integration_test.zig`) — **DONE**. The `GENERATED ALWAYS` half stays covered by R2's source grep; this closes the duplicate-unique half, catalogue-wide, over unique INDEXES so a constraint-less `CREATE UNIQUE INDEX` cannot slip the same race past it.
- **Dimension 2.2** — every foreign key references the primary key, a unique constraint that strictly *contains* it (the tenant-scoping superkeys, each commenting the reference it serves), or a declared domain key in the test's allowlist — never a duplicate-identity twin → Test `every foreign key resolves to a primary key, a superkey, or the one declared domain key` (`db/schema_shape_integration_test.zig`) — **DONE**
- **Dimension 2.3** — identifiers remain application-generated version 7, and the public field name is unchanged at the API boundary → Test `types/id_format_test.zig` (the generator: byte layout, version nibble, variant bits, and its own validator) plus the per-table `ck_*_id_uuidv7` CHECK constraints, which refuse a non-v7 id at the database rather than trusting the generator — **DONE**. The twins that made "which id?" ambiguous are asserted gone by `core key schemas: the retired identity twins are gone and stay gone` (`fleet/schema_migration_test.zig`).

### §3 — Money consolidated behind referential integrity

Money lives in three schemas and the ledger carries `workspace_id` and `fleet_id` as text with no foreign key — which is why the counter trigger runs a regular expression on every renewal, and why erasure needs a hand-maintained delete order. Consolidating the wallet and ledger into `billing` with real foreign keys makes ownership a database fact. The *privilege* half of the same idea — `api_runtime` holding direct grants on the wallet and the secret store — is M154_002, landing in this PR.

- **Dimension 3.1** — the ledger resolves to a tenant, a workspace and a fleet through foreign keys, all typed as identifiers → Test `the ledger resolves every identity through a typed foreign key` (`db/schema_shape_integration_test.zig`) — **DONE**. Also pins the delete behaviours apart, one cascade and two SET NULL, so widening either is a red test rather than a silent hole in the money record.
- **Dimension 3.2** — the counter trigger carries no pattern match, because its input is no longer text → Test `no trigger body pattern-matches, because its input is no longer text` (`db/schema_shape_integration_test.zig`) — **DONE**
- **Dimension 3.3** — erasing a tenant leaves zero rows anywhere, with the explicit delete order reduced to what no cascade covers → Test `integration: erasing a tenant leaves no row behind in any tenant-scoped table` (`state/account_teardown_test.zig`) — **DONE**, with its strength stated plainly below.
  - **Written during the close-out audit** — the Dimension named a test that did not exist, and the nearest existing ones (`a mid-purge failure rolls back`, `a fully enumerated tenant reports no unhandled fleets`) prove rollback safety and enumeration completeness, never "zero rows anywhere".
  - **It is a regression guard, not a proof.** The sweep reads every `tenant_id`-carrying table from `information_schema`, so a table added later is covered automatically. But it cannot fail against today's schema: every tenant-scoped row is reachable by some cascade. Established by mutation — removing the purge's explicit `fleet.runner_leases` DELETE left the suite green, because that table cascades from `core.fleets` even though it carries no foreign key to `core.tenants`. The delete is belt-and-braces. The test's value is that it fires the day a tenant-scoped table arrives without a cascade or a delete, which is the failure nobody notices until an erased account is found to have left data behind.

### §4 — Retire the accrual detail table

`fleet.metering_periods` gains a row per renewal — roughly one per twenty seconds of every run — and its only endpoint is called by no product surface. **Correction made during implementation: it had a second, live reader.** The budget drain joined it to attribute a run's spend to a rolling window using per-slice timestamps, because the accumulating ledger row pins one instant at ~run start; with `MAX_RUNTIME_MS` at 12h against a 24h window, collapsing onto that instant would let a long run's spend fall out of the window and under-enforce the daily cap. The table still goes — but `billing.usage_ledger` gains `last_charged_at`, so the drain apportions the accumulated total across `[created_at, last_charged_at]` by overlap, and gains `token_count_cached_input`, without which the charge cannot be recomputed from the row. The per-slice detail becomes an M155 concern on the durable event stream.

- **Dimension 4.1** — the table, its store, its handler and its endpoint are gone, with no orphaned reference → Test `test_accrual_surface_fully_removed` + `test_accrual_reader_and_route_are_gone_from_the_sources` (`state/accrual_surface_integration_test.zig`)
  - **Written during the close-out audit** — the Dimension named a test that did not exist. Absence is what an ordinary behavioural test cannot assert (there is no handler left to call), so it reads the two places the surface could return from: the catalogue (no `metering_periods` table in any schema, no index named for it) and the sources (`route_table.zig`, `routes.zig`, `handlers/tenant_billing.zig` carry none of `fleet_metering_store`, `get_tenant_metering_periods`, `innerGetTenantBillingTelemetry`, `metering_periods`). Same shape as the fleet-key retirement suite, for the same reason. Note `state/fleet_telemetry_store.zig` is the *ledger* store and legitimately survives — it backs the charges list, which is still served.
- **Dimension 4.2** — the wallet drain still reconciles against the ledger across a metered run with renewals → Test `the wallet drain equals the ledger sum across a metered run` (`fleet/credit_metric_reconciliation_integration_test.zig`) — **DONE**. Receive + two renewals + settle drains 35920 nanocredits and the ledger records exactly that. Distinct from the sibling metric reconciliation, which would still pass if the ledger and the wallet disagreed with each other.
- **Dimension 4.3** — the ledger carries the originating event's creation time, so a later partitioning decision has a stable key available → Test `every ledger row for one event carries the event's creation time, not its own` (`fleet/credit_metric_reconciliation_integration_test.zig`) — **DONE**, with the column's NOT NULL shape asserted in `db/schema_shape_integration_test.zig`. A run is what proves the value is the event's instant rather than the row's.

### §5 — Correct the indexes

Three indexes serve no live query and one is short by the column its only reader needs. Each surviving index states the query it serves and the growth that justifies it, per the standard slot 033 set.

- **Dimension 5.1** — no index appears or disappears without a deliberate edit to the declared roster, so a reader deleted by a later milestone cannot leave its index behind → Test `every index in the schema is declared, and every declared index still exists` (`db/index_usage_integration_test.zig`)
  - **Rewritten during the close-out audit.** The original wording claimed "the *two* ledger indexes whose reader was deleted are gone". That is not what happened: `(fleet_id, recorded_at)` was deleted outright, while `(workspace_id, recorded_at)` survives in narrower form as `idx_usage_ledger_workspace_id`, re-justified by the workspace `SET NULL` referential action rather than by a query. One deleted, one re-grounded.
  - The original also asserted a *comment convention* — "a named reader" is prose in the slot file, which the catalogue cannot read. That is the same defect Dimension 6.1 carries. Indy's call was to assert the enforceable half across every table: a roster of all 36 discretionary indexes (constraint-backed indexes excluded — a primary key's index is justified by its constraint, not by a query), checked in both directions. An undeclared index in the catalogue fails; a declared index missing from the catalogue fails.
- **Dimension 5.2** — the tenant charges keyset pages without a sort node, because its index carries the tiebreak column (RULE KYS) → Test `the tenant charges keyset pages without sorting, because its index carries the tiebreak` (`db/index_usage_integration_test.zig`)
  - **This test found a live defect on the money read path.** `TELEMETRY_SELECT` emits `id::text`, which names an *output* column `id`; a bare `ORDER BY … id DESC` resolves against the output list before the table, so every page of `GET /v1/tenants/me/billing/charges` sorted the text cast and the planner added an Incremental Sort — the precise per-page cost `schema/720_usage_ledger_indexes.sql` reshaped the index to remove, and which that file already claimed was "asserted against the plan". The `WHERE` clause was never affected: output aliases are invisible there, so the seek always bound to the real column, which is why the Index Cond looked correct and no existing check caught it. Fixed by table-qualifying to `ORDER BY created_at DESC, usage_ledger.id DESC`, and the two page queries are now named `pub const`s the test plans against directly so a transcription cannot drift from production.
- **Dimension 5.3** — the memory retention sweep is served by a composite leading with the fleet identifier rather than a bare low-cardinality column → Test `memory composite has the right shape and serves the fleet filter` + `retention sweep deletes ride their own indexes, not a whole-table scan` (`db/index_usage_integration_test.zig`) — **DONE**

### §6 — Schema hygiene

Value literals moved out of `DEFAULT` and `CHECK` under RULE STS but reappeared in partial-index predicates and trigger bodies, where they drift from the application constants exactly the same way. A schema is granted and revoked but holds no tables. Timestamp columns use four different names for the same concept.

- **Dimension 6.1** — every surviving literal in an index predicate or trigger body names the application constant it mirrors, or the predicate is gone → Test `test_schema_literals_carry_named_provenance` — **NOT DONE, no test.** Four partial-index predicates survive; two carry value literals: `fleet.runner_events` on `event_type = 'runner_offline'` and `core.fleet_approval_gates` on `status = 'pending'`. The Dimension as written asserts a *comment convention* — that the source names the constant it mirrors — which the catalogue cannot see, so a test would have to parse `schema/*.sql` text and would pass on a comment that names the wrong constant. The invariant actually worth pinning is drift: those two literals must equal the application constants they mirror, the way `audits/cross-tier-rates.sh` already pins the rate constants across four files. That is a different test from the one this Dimension names, so it needs Indy's call before it is written rather than a quietly substituted assertion.
- **Dimension 6.2** — no schema is created that holds no tables → Test `no schema is created that holds no table` (`db/schema_shape_integration_test.zig`) — **DONE**
- **Dimension 6.3** — row lifecycle timestamps are named consistently across every table → Test `row lifecycle timestamps carry one naming form across every table` (`db/schema_shape_integration_test.zig`) — **DONE**

### §7 — The events list stops reading bodies

The list select carries the event body and the full agent answer on every row, up to two hundred per page. Three rendered surfaces read them: the events table's prose cell, the fleet header's outcome line, and the fleet thread's transcript — so the original claim that bodies are wanted only on expansion was wrong about this tree. Indy's call: the list drops them anyway and those surfaces state the outcome instead of quoting the answer, because a page of two hundred unbounded bodies is the wrong price for one hundred and sixty rendered characters. Splitting list from detail removes the cost from the common read; dropping the lease's duplicate copy of the same body removes it from the write path too. **Implementation default:** reclaim reads the body by joining the event row on its existing unique key — both tables cascade from the same parent, so the join cannot dangle.

- **Dimension 7.1** — the list query selects no body column and its plan touches no oversized-attribute storage; the table's prose cell states the outcome (a failure sentence, else `No result recorded`) rather than quoting a reply it no longer receives → Test `test_events_list_selects_no_payload_columns` — **DONE**
- **Dimension 7.2** — a single-event detail read returns the body and the answer, scoped to the caller's workspace → Test `test_event_detail_returns_body_scoped_to_workspace` — **DONE**
- **Dimension 7.3** — the lease carries no body copy, and reclaiming an expired lease still re-delivers the original event → Test `runner schema: fleet.runner_leases is migrated with its columns and constraint` (`fleet/schema_migration_test.zig`, whose exact column count refuses a re-added body column) + `reclaim sweep recovers a stranded delivery from a dead consumer and re-leases it` (`fleet/event_lifecycle_reclaim_integration_test.zig`) — **DONE**. The two halves are deliberately separate: the first proves the copy is gone, the second proves redelivery still works without it, and only together do they mean the column was removable.
- **Dimension 7.4** — reclaim's lifetime-tally arm still rides the same statement as the status flip, so the counter cannot drift from the rows it counts → Test `reclaim tallies the expiry even when the event body is gone and it returns nothing` (`fleet/runner_counters_integration_test.zig`) — **DONE**. Deleting the event body makes the INNER join return nothing, which is the only case where a tally written by a SECOND statement would behave differently from one riding the flip.

### §8 — A grant can be born, and `core.fleet_keys` retires — **DONE**

`core.integration_grants` is the enforcement spine for internal credential minting — three readers gate on it, and the App ingress routing query inner-joins it on `status = 'approved'`. Yet the only production statement that ever *creates* a grant row sits behind `POST …/integration-requests`, authenticated by an `agt_a` fleet key that exists for external callers (LangGraph, CrewAI, Composio) and that no internal fleet ever holds. So an internally-installed fleet declaring `required_credentials: ["github"]` can never obtain a grant: the ingress join excludes it, no event row is written, no lease is issued, and nothing anywhere reports that a decision was owed. The fleet is silently inert, and every test that exercises minting hand-seeds an approved row, which is why the gap survived.

The origination path belongs where the requirement becomes known — at install, from the bundle fields the catalogue already stores — and the decision belongs in the approval-gate machine this codebase already ships: an inbox, a detail page with an evidence tree, resolve buttons, a Slack webhook, a timeout sweeper and an append-only audit. A gate is a per-event decision; a grant is the standing answer that outlives the run. The gate asks; the grant remembers. With origination moved there, the external surface has no internal dependant and retires whole — the handler-local authentication, its dead session arm, the fleet-key table, and a second approval path that duplicated the gate's own webhook down to the notifier.

**Implementation default:** the grant seed runs synchronously in the create handler, alongside `INSERT core.fleets` — not in `create_install_steps.zig`'s progression, whose every sub-step is best-effort by design. A best-effort seed reproduces the exact defect this section removes: the row flips to `active` carrying no grant.

- **Dimension 8.1** — installing a fleet whose bundle declares a MINTABLE credential creates a pending grant recording why it exists, and raises one approval gate naming the service and the credential that classified to it; a credential that cannot mint raises nothing → Test `test_install_seeds_pending_grant_and_gate` — **DONE**
  - The bundle carries no free-text justification field, so the grant's `requested_reason` states the origin once (`create_grants.S_DEFAULT_REASON`) rather than quoting an author. A bundle-authored reason is a feature, not a gap in this Dimension.
- **Dimension 8.2** — resolving that gate as approved flips the grant to approved, and the fleet's webhook events then route → Test `test_gate_approval_arms_webhook_routing` — **DONE**
- **Dimension 8.3** — a lease whose resolved credential has no approved grant parks the event and re-evaluates on the next poll, instead of dropping the credential and issuing a lease that cannot work → Test `test_lease_parks_on_missing_grant` — **DONE**
- **Dimension 8.4** — no route authenticates outside the middleware chain; the handler-local fleet authentication and its session arm are gone → Test `test_no_handler_local_authentication` — **DONE**
- **Dimension 8.5** — `core.fleet_keys` is absent from the catalogue and unreferenced across the tree → Test `test_fleet_keys_surface_fully_removed` — **DONE**

### §9 — The operator can see why a run failed, and can narrow the feed to find it

Added mid-stream at Indy's direction, from two defects he hit on the deployed surface. Both are operator-visibility failures on the runner plane, which is why they land together rather than as separate specs.

The first: a crashed lease's `{}` Details expander was empty, so `FleetRunFailed` — the outcome *label* — was the only thing the row could say. The detail plumbing was never the problem; `execute` already wrote `.detail = @errorName(err)`. The run-failure sites collapsed the true error into `RunnerError.FleetRunFailed` before it reached that line, so the detail restated the label and the operator learned nothing. Propagating the real error is safe precisely because `mapError`'s `else` arm classifies any unmapped error as `.runner_crash` — the same class `FleetRunFailed` carried — so the failure class is unchanged and only the detail improves. The `Fleet.fromConfig` site keeps its mapped error: init failures are `.startup_posture`, which that same fallback would lose.

The second: the lease feed could be narrowed to a workspace only by clicking a funnel on a row, which requires already having found a row from the workspace you wanted — the affordance assumed the answer to the question it was there to ask. There was no fleet filter at all. Indy's call: a toolbar filter in the shape GitHub's issue search uses, both filters addressable in the URL, and the row funnel deleted rather than kept beside it.

**Implementation default:** the fleet filter matches an id **or** an exact, case-insensitive fleet name, because the table shows operators names and no one should have to transcribe a UUID to filter by what they can already read. It intersects with the workspace filter rather than replacing it, and it scopes the pager's total and the keyset cursor on the same terms the workspace filter already did (RULE KYS — a cursor names a position in one ordered stream, and the filters are part of what defines it).

- **Dimension 9.1** — a run that fails at the model call reports the error that actually stopped it, not the outcome label → Test `a run failure propagates its own error instead of collapsing to FleetRunFailed` (`runner/engine/run_context_test.zig`). Proven by mutation: restoring the collapse turns it red with the stub provider's rejection reaching `fleet.runSingle` first.
- **Dimension 9.2** — the fleet filter scopes both the rows and the pager's total, by id and by exact name, case-insensitively; a value no fleet matches is an empty page, not an error → Test `test_runner_leases_fleet_filter_scopes_rows_and_total`
- **Dimension 9.3** — the workspace and fleet filters intersect rather than one overriding the other: pairing a fleet with a workspace it does not belong to matches nothing → Test `test_runner_leases_workspace_and_fleet_filters_intersect`
- **Dimension 9.4** — an empty or over-long fleet filter is refused, while an unmatched name is an ordinary empty page — the boundary between "refused" and "matched nothing" is explicit → Test `test_runner_leases_unbounded_fleet_filter_is_refused`
- **Dimension 9.5** — the filter query parses as GitHub's does: both tokens in any order, quoted values kept whole, unrecognised tokens dropped rather than guessed at, last occurrence winning on a repeat → Test `lease-filter-query.test.ts`
- **Dimension 9.6** — applying a filter drops the cursor trail walked through the old result set, and clearing one filter leaves the other in place → Test `LeaseTable.test.tsx` (`applies both filter tokens and drops the cursor trail with the old result set`, `clears one filter without disturbing the other`)

## Interfaces

```
REMOVED    GET /v1/tenants/me/billing/charges/{event_id}/telemetry
           Per-renewal accrual detail; no product surface calls it. Pre-2.0, so
           removed outright rather than answering 410.

REMOVED    POST /v1/workspaces/{workspace_id}/fleets/{fleet_id}/integration-requests
           Grant origination for external callers, authenticated by a fleet key
           outside the middleware chain (§8). Origination moves to install.

REMOVED    POST /v1/webhooks/{fleet_id}/grant-approval
           Second approval path with its own Redis nonce; the approval-gate
           webhook already resolves decisions for this workspace.

REMOVED    GET|POST /v1/workspaces/{workspace_id}/fleet-keys
           DELETE   /v1/workspaces/{workspace_id}/fleet-keys/{fleet_key_id}
           Management surface for the external-caller credential. Retires with
           `core.fleet_keys`; external collaboration returns as a first-class
           principal, not a handler-local lookup.

ADDED      GET /v1/workspaces/{workspace_id}/fleets/{fleet_id}/events/{event_id}
           200 → the full event row including request body and response text.
           404 → unknown event, OR one outside the caller's workspace — the two
                 are indistinguishable to the caller (existing practice).
           Authorization: the same workspace check the list read performs.

UNCHANGED  GET /v1/workspaces/{workspace_id}/fleets/{fleet_id}/events
           GET /v1/workspaces/{workspace_id}/events
           Same shape minus `request_json` / `response_text` (see ADDED above).
UNCHANGED  GET /v1/tenants/me/billing/charges
           Shape and cursor semantics untouched; only the index beneath changes.

ADDED      GET /v1/fleets/runners/{id}/leases?fleet=<id-or-name>
           Optional filter (§9). Matches a fleet id or an exact, case-insensitive
           fleet name. Intersects with the existing `workspace_id` filter and
           scopes the pager's `total` and the keyset cursor on the same terms.
           400 → empty or longer than 200 characters.
           200 with an empty page → well-formed but matching no fleet.
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Stale slot constant | A named slot-version constant still points at a retired number | Bootstrap fails loudly at the version assertion, never silently skips a table |
| Partial bootstrap | A slot fails mid-apply on an empty database | The migration runner's per-slot transaction rolls that slot back; the recorded version does not advance |
| Erasure misses a table | A table is added later without a tenant-resolving foreign key | The erasure test enumerates tables from the catalogue and fails on any that retains rows |
| Detail read crosses a workspace | A caller requests an event identifier belonging to another workspace | 404, identical to an unknown identifier — no existence disclosure |
| Reclaim after event delete | An expired lease is reclaimed for an event whose row is gone | Reclaim finds no body and fails the re-delivery cleanly rather than delivering an empty event |
| Index without a reader | A future index is added with no query behind it | The named-reader test fails on any index with no citation |

## Invariants

1. **One identity column per table** — no generated identity column, no unique constraint duplicating the primary key; asserted from the catalogue by test, not by review.
2. **Every money row resolves to a tenant through a foreign key** — enumerated from the catalogue; a money table without that path fails the test.
3. **Every index has a named reader** — each index carries the query it serves; the test fails on any index with no citation in the slot that creates it.
4. **No `ALTER` or `DROP` statement exists in `schema/`, and no new slot reuses a retired number** — pre-2.0 teardown posture (RULE SCH); both grep-backed. **The list read touches no body column** — asserted against the plan and the select list, so a future widening fails rather than silently regressing.
5. **The wallet drain equals the ledger sum** — the reconciliation the accrual table was thought to provide is proven against the ledger directly.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| not applicable — no product or operator signal changes | not applicable | This milestone removes an unread endpoint and reshapes storage; no metric family, span, or product event is added, renamed, or retired | not applicable | not applicable | `test_metric_family_census_unchanged` |

## Test Specification (tiered)

**Reconciled during the close-out audit.** This table originally carried the
authoring plan's invented `test_*` identifiers; the Sections above were repointed
at the tests that actually shipped, and the two rosters then disagreed. Every row
below now names a test that exists — verified by grepping each identifier and
each quoted test name against `src/`, `ui/` and `cli/`. Where a row's original
assertion turned out to be false, the assertion is corrected here rather than
quietly retired; §9's rows were absent entirely and are added.

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit | `canonical schema bootstrap: every slot sits in the new numbering space` (`cmd/migration_policy_test.zig`) | Every slot lands in the new space and none reuses a retired `001`–`046` name, tying "starts at 100" to "never reuses" so the two cannot drift |
| 1.2 | unit | `canonical migrations: connector install + channel tables registered` (`cmd/migration_policy_test.zig`) | The slot version **is** the filename, so a named slot-version constant can no longer drift from the file it names. The original asserted those constants resolve correctly; the rebuild deleted the constants, so the drift class is unrepresentable rather than merely checked |
| 1.3 | integration | `fresh bookkeeping: a migration applies once and a re-run is a no-op` + `runMigrations: fully-applied database is idempotent across repeated runs` (`fleet/schema_migration_test.zig`) | A torn-down database applies every slot in order; a re-run changes nothing. Reinforced every run — `make test-integration` migrates from empty before the suite executes |
| 2.1 | integration | `no table carries a second unique key over its own primary key columns` (`db/schema_shape_integration_test.zig`) | Zero uniques duplicating a primary key, checked over unique INDEXES so a constraint-less `CREATE UNIQUE INDEX` cannot slip the same `ON CONFLICT` race past it. The `GENERATED ALWAYS` half stays covered by R2's source grep |
| 2.2 | integration | `every foreign key resolves to a primary key, a superkey, or the one declared domain key` (`db/schema_shape_integration_test.zig`) | Every foreign key's referenced columns are the primary key, a strict superset of it, or the one allowlisted domain key (`model_library (provider, model_id)`); zero references to a twin |
| 2.3 | unit + integration | `types/id_format_test.zig` (byte layout, version nibble, variant bits) + `core key schemas: the retired identity twins are gone and stay gone` (`fleet/schema_migration_test.zig`) | Generated identifiers carry version nibble 7, and the per-table `ck_*_id_uuidv7` CHECKs refuse a non-v7 id at the database rather than trusting the generator; the twins are asserted gone |
| 3.1 | integration | `the ledger resolves every identity through a typed foreign key` (`db/schema_shape_integration_test.zig`) | Every ledger identity column is a typed foreign key, with the one cascade and two `SET NULL`s pinned apart so widening either is a red test rather than a silent hole in the money record |
| 3.2 | integration | `no trigger body pattern-matches, because its input is no longer text` (`db/schema_shape_integration_test.zig`) | The counter trigger body contains no pattern-match operator |
| 3.3 | integration | `integration: erasing a tenant leaves no row behind in any tenant-scoped table` (`state/account_teardown_test.zig`) | **A regression guard, not a proof** — see Dimension 3.3. Sweeps every `tenant_id`-carrying table read from `information_schema`, so a table added later is covered automatically; it cannot fail against today's schema, and fires the day a tenant-scoped table arrives without a cascade or a delete |
| 4.1 | integration | `test_accrual_surface_fully_removed` + `test_accrual_reader_and_route_are_gone_from_the_sources` (`state/accrual_surface_integration_test.zig`) | No `metering_periods` table in any schema and no index named for it, and none of `fleet_metering_store`, `get_tenant_metering_periods`, `innerGetTenantBillingTelemetry`, `metering_periods` in `route_table.zig`, `routes.zig` or `handlers/tenant_billing.zig` |
| 4.2 | integration | `the wallet drain equals the ledger sum across a metered run` (`fleet/credit_metric_reconciliation_integration_test.zig`) | Receive + two renewals + settle drains 35920 nanocredits and the ledger records exactly that — distinct from the sibling metric reconciliation, which would still pass if ledger and wallet disagreed with each other |
| 4.3 | integration | `every ledger row for one event carries the event's creation time, not its own` (`fleet/credit_metric_reconciliation_integration_test.zig`) | Every ledger row for one event carries the same originating creation time across renewals, with the column's NOT NULL shape asserted in `db/schema_shape_integration_test.zig` |
| 5.1 | integration | `every index in the schema is declared, and every declared index still exists` (`db/index_usage_integration_test.zig`) | A roster of all 36 discretionary indexes (constraint-backed ones excluded — a primary key's index is justified by its constraint, not a query), checked in both directions: an undeclared index in the catalogue fails, and a declared index missing from the catalogue fails |
| 5.2 | integration | `the tenant charges keyset pages without sorting, because its index carries the tiebreak` (`db/index_usage_integration_test.zig`) | The cursor-paged charges plan is an index scan with no sort node. This test found a live defect — see Dimension 5.2 |
| 5.3 | integration | `memory composite has the right shape and serves the fleet filter` + `retention sweep deletes ride their own indexes, not a whole-table scan` (`db/index_usage_integration_test.zig`) | The aged-category delete plans on the composite, not a low-cardinality single column |
| 6.1 | — | **none — deliberately unbuilt** | The Dimension asserts a *comment convention* the catalogue cannot see, so a test would parse `schema/*.sql` text and pass on a comment naming the wrong constant. The invariant worth pinning is literal-vs-constant drift, which is a different assertion — it needs Indy's call, not a quiet substitution. See Dimension 6.1 |
| 6.2 | integration | `no schema is created that holds no table` (`db/schema_shape_integration_test.zig`) | Every created schema holds at least one table |
| 6.3 | integration | `row lifecycle timestamps carry one naming form across every table` (`db/schema_shape_integration_test.zig`) | Lifecycle timestamp columns use one naming form across every table |
| 7.1 | integration | `test_events_list_selects_no_payload_columns` (`http/handlers/fleets/events_payload_free_integration_test.zig`) | A page of 200 events, each holding a 20 kB body on both sides, returns no body or answer field, weighs under 256 kB, and moves `pg_statio_all_tables.toast_blks_*` for `core.fleet_events` by exactly zero |
| 7.2 | integration | `test_event_detail_returns_body_scoped_to_workspace` (`http/handlers/fleets/event_detail_integration_test.zig`) | Expanding a row fetches the body; the same identifier from another workspace answers 404 |
| 7.2 | integration | `test_event_detail_404s_unknown_and_cross_workspace_alike` (`http/handlers/fleets/event_detail_integration_test.zig`) | An unknown identifier and a real event in a sibling workspace return the same status and the same error code; the refused body never carries the stored answer |
| 7.3 | integration | `runner schema: fleet.runner_leases is migrated with its columns and constraint` (`fleet/schema_migration_test.zig`) + `reclaim sweep recovers a stranded delivery from a dead consumer and re-leases it` (`fleet/event_lifecycle_reclaim_integration_test.zig`) | The first proves the body copy is gone — its exact column count refuses a re-added body column; the second proves redelivery still works without it. Only together do they mean the column was removable |
| 7.4 | integration | `reclaim tallies the expiry even when the event body is gone and it returns nothing` (`fleet/runner_counters_integration_test.zig`) | Reclaiming N leases increments the expired tally by exactly N, with no separate statement. Deleting the body makes the INNER join return nothing — the only case where a tally written by a second statement would behave differently |
| 8.1 | integration | `test_install_seeds_pending_grant_and_gate` (`http/handlers/fleets/create_grants_integration_test.zig`) | Installing a bundle declaring `required_credentials:["github"]` yields one `pending` grant for that fleet and one pending gate of kind `integration_grant` carrying the bundle's stated reason |
| 8.1 | integration | `test_install_seeds_no_grant_for_a_static_credential` (`http/handlers/fleets/create_grants_integration_test.zig`) | A bundle credential resolving to a stored value rather than a mintable handle yields zero grants and zero gates — the classifier's other answer |
| 8.2 | integration | `test_gate_approval_arms_webhook_routing` (`http/handlers/fleets/create_grants_integration_test.zig`) | Resolving that gate approved flips the grant to `approved`; the App ingress routing query, which returned zero targets before, now returns the fleet |
| 8.2 | integration | `test_gate_denial_revokes_the_grant` (`http/handlers/fleets/create_grants_integration_test.zig`) | Resolving the same gate denied drives the grant to `revoked` (never back to `pending`, which nothing re-raises) and the ingress read still returns zero |
| 8.3 | integration | `test_lease_parks_on_missing_grant` (`fleet/control_plane_grant_integration_test.zig`) | A fleet whose resolved credential has no approved grant answers no-work and writes no lease; after approval the next poll issues a lease carrying the mintable |
| 8.4 | integration | `test_no_handler_local_authentication` (`http/handlers/fleets/grant_surface_integration_test.zig`) | No handler reads the `authorization` header directly; every route resolves its principal through the middleware registry |
| 8.5 | integration | `test_fleet_keys_surface_fully_removed` (`http/handlers/fleets/grant_surface_integration_test.zig`) | Zero references to `fleet_keys`, `agt_a`, `grant-approval` or `integration-requests` across `schema/`, `src/`, `public/openapi.json`, `cli/` and `ui/` |
| 9.1 | unit | `a run failure propagates its own error instead of collapsing to FleetRunFailed` (`runner/engine/run_context_test.zig`) | A run that fails at the model call reports the error that stopped it, not the outcome label. Proven by mutation: restoring the collapse turns it red |
| 9.2 | integration | `test_runner_leases_fleet_filter_scopes_rows_and_total` (`http/runner_read_integration_test.zig`) | The fleet filter scopes both the rows and the pager's total, by id and by exact name, case-insensitively; a value no fleet matches is an empty page, not an error |
| 9.3 | integration | `test_runner_leases_workspace_and_fleet_filters_intersect` (`http/runner_read_integration_test.zig`) | Pairing a fleet with a workspace it does not belong to matches nothing — the filters intersect rather than one overriding the other |
| 9.4 | integration | `test_runner_leases_unbounded_fleet_filter_is_refused` (`http/runner_read_integration_test.zig`) | An empty or over-long fleet filter is refused, while an unmatched name is an ordinary empty page |
| 9.5 | unit | `lease-filter-query.test.ts` | Both tokens parse in any order, quoted values stay whole, unrecognised tokens are dropped rather than guessed at, and the last occurrence wins on a repeat |
| 9.6 | unit | `LeaseTable.test.tsx` (`applies both filter tokens and drops the cursor trail with the old result set`, `clears one filter without disturbing the other`) | Applying a filter drops the cursor trail walked through the old result set; clearing one filter leaves the other in place |
| regression | integration | `integration: GET /billing/charges returns charges newest-first` + `integration: GET /billing/charges serves a charge whose fleet was deleted` (`http/handlers/tenant_billing_integration_test.zig`) | **Assertion corrected.** The original row claimed the charges response shape was *unchanged* by the rebuild. It is not: `workspace_id` and `fleet_id` became nullable with the `SET NULL` referential actions, and the pre-existing cursor tests never observed a row, so the row mapper had never executed under test. These two do observe rows — one pins the promised order and every numeric field to a distinct multiple, the other pins the deleted-fleet null path |
| regression | — | **retired — the render deliberately changed** | The original row asserted the events table renders identically after the payload columns leave the list read. Dimension 7.1 changed that render on purpose: the prose cell now states the outcome rather than quoting an answer it no longer receives. An "unchanged render" assertion would contradict the Section it was meant to protect |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Schema carries no patch statements (§1, §6) | `grep -rnE 'ALTER TABLE\|DROP (TABLE\|INDEX\|SCHEMA)' schema/` | no output | P0 | ✅ no output |
| R2 | No table carries a duplicate identity column (§2) | `grep -rn 'GENERATED ALWAYS' schema/` | no output | P0 | ✅ no output |
| R3 | The accrual surface is gone (§4) | `grep -rnE 'metering_periods\|get_tenant_metering_periods\|slice_seq' src/ ui/ public/openapi.json \| grep -vE ':[0-9]+:[[:space:]]*(//\|--)'` | no output | P0 | ✅ no output (14 raw hits, all comments recording the retirement) |
| R4 | The list read touches no oversized-attribute storage (§7) | `grep -n 'request_json\|response_text' src/agentsfleetd/state/fleet_events_store.zig` **and** `test_events_list_selects_no_payload_columns` | no output; test passes | P0 | ✅ no output |
| R5 | Diff stays inside Files Changed | `git diff --name-only origin/main...HEAD \| awk -F/ '{print $1}' \| sort -u` | every root appears in Files Changed: `.github cli docs make playbooks public samples schema scripts src ui` | P0 | ✅ eleven roots, all present. **Regraded during the close-out audit** — the earlier grade read "all nine roots present" against a nine-root list, and the diff carries eleven: `.github/` was in the table but absent from the criterion's list, and `playbooks/` was in neither. Both are now listed above and in Files Changed. The twelfth, `HANDOFF.md`, is the ephemeral brief CHORE(close) deletes |
| R6 | A grant can be born without hand-seeding (§8) | `grep -rnE 'core\.fleet_keys\|/(integration-requests\|fleet-keys)' src/ schema/ public/openapi.json \| grep -vE ':[0-9]+:[[:space:]]*(//\|--)'` | no output | P0 | ✅ no output — after removing the retired path from `public/openapi/` and repointing two error hints |
| S1 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | ✅ exit 0 — `test depth gate passed (unit=3409 integration=571)`, `integration suite executed (798 passed; 7 skipped; 0 failed.)`, `merged line coverage passed (86.00% >= 83%)`, and `ui/packages/app` back at its 100% floor on all four metrics (6175/6175, 3676/3676, 1689/1689, 5515/5515). Earlier runs failed three unrelated ways; see Discovery. |
| S2 | Lint clean | `make lint-all` | exit 0 | P0 | ✅ exit 0, `All lint checks passed` — after the REST guide's raw-handler table dropped two rows naming handlers §8 deleted (`webhooks/grant_approval.zig`, `integration_grants/handler.zig`). That guide lives in the dotfiles checkout; fixed and pushed there as `70e7a42` |
| S3 | Integration passes (schema, HTTP and Redis touched) | `make test-integration` | exit 0 | P0 | ✅ exit 0, `Full integration suite passed`, zero leaked allocations — run against a database rebuilt from an empty volume, so it is also the from-empty bootstrap §1.3 claims. Previously exit 2 on a pre-existing allocator-ownership leak; see Discovery. |
| S4 | The expand-a-row detail dialog is covered | `make test-unit-app` | exit 0 | P0 | ✅ exit 0 — `Test Files 213 passed (213)`, `Tests 2160 passed (2160)` |
| S5 | No leaks (allocator wiring touched) | `make memleak` | exit 0 | P0 | ✅ exit 0, `memleak gate passed (agentsfleetd + runner + lib lanes + boot→drain lifecycle)` |
| S6 | Cross-compile | `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` | exit 0 | P0 | ✅ exit 0 — both `x86_64-linux` and `aarch64-linux` |
| S7 | No secrets | `gitleaks detect` | exit 0 | P0 | ✅ `no leaks found` (4130 commits scanned) |
| S8 | Orphan sweep | Dead Code Sweep greps below | 0 matches | P0 | ✅ both files absent; all three reference greps 0 live matches (`uid` 0 raw; the other two are prose-only — 12 and 4 comment hits, 0 after the comment filter) |

**Grading protocol (VERIFY):** run the Verify command verbatim; grade ONLY from its output. Graded = ✅/❌ + the one decisive output line (`342 passed`); long evidence goes to PR Session Notes with a pointer here. **Ship gate:** every row graded, every P0 ✅ → eligible for CHORE(close); any ❌ or empty cell → return to EXECUTE; a P1 ❌ ships only with an Indy-acked deferral quote in Discovery.

## Dead Code Sweep

**1. Orphaned files — deleted from disk and git.**

| File to delete | Verify |
|----------------|--------|
| `src/agentsfleetd/state/fleet_metering_store.zig` | `test ! -f src/agentsfleetd/state/fleet_metering_store.zig` |
| every retired `schema/0NN_*.sql` slot (001–046) | `test -z "$(ls schema/0*.sql 2>/dev/null)"` |

**2. Orphaned references — zero remaining imports/uses.**

Each grep drops comment lines, for the reason R3 and R6 record: a codebase that documents a retirement must not fail the criterion asserting that retirement.

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| `metering_periods`, `fleet_metering_store` | `grep -rnE -w "metering_periods\|fleet_metering_store" src/ ui/ public/ \| grep -vE ':[0-9]+:[[:space:]]*(//\|--\|\*)'` | 0 matches |
| `fleet_execution_telemetry` | `grep -rn -w "fleet_execution_telemetry" src/ \| grep -vE ':[0-9]+:[[:space:]]*(//\|--\|\*)'` | 0 matches |
| `uid` | `grep -rn -w "uid" src/ schema/` | 0 matches |

## Out of Scope

- **Payload offload to object storage** — event bodies stay in Postgres this milestone; the content-hash offload lands in **M155_001** (`docs/v2/pending/M155_001_P1_API_INFRA_SQL_PAYLOAD_OFFLOAD_AND_DURABLE_STREAM.md`) beside the outbox.
- **Partitioning** — only the stable partition key is carried. The machinery is deferred until a measurement demands it; the rationale is in Decomposition.
- **New retention policies** — M149 already ships a thirty-day sweep over runner leases and runner events, with a comptime proof that an age-keyed sweep cannot reach live work. That behaviour, its window, and its proof are **preserved unchanged**; this milestone adds no retention anywhere else.
- **`billing.usage_rollup`** — deferred with partitioning; the ledger is already bounded per event, so the rollup is an optimisation without a forcing reason today.
- **Durable outbox to Elastic or Loki** — **M155_001** §2, together with the accrual detail this milestone removes from Postgres. That spec deliberately does not settle Elastic-versus-Loki either; it pins the record's content and the emit's failure posture, and records the destination as an open decision that blocks its own promotion out of `pending/`. Approval gates get no retention policy at all until Indy sets one; the table is a compliance record.
- **Horizontal sharding** — **measure, then decide.** Out of scope here on the strength of *scoping*: every hot query is already tenant, workspace or fleet scoped, so the shard key is obvious if it is ever needed, and partitioning addresses retention rather than throughput. The earlier wording rejected sharding outright on the grounds that "write rate sits far below single-node capacity" — an assumption this milestone never measured, and too thin to close a door that costs cross-shard reads, rebalancing and distributed transactions to reopen. Record peak write rate against single-node headroom first; reject or build on that number, not on this sentence.
- **Privilege split for the wallet and secret store** — M154_002, same PR.
- **Row-Level Security** — its own milestone. This one is the prerequisite (policies need every row to resolve to a tenant); the cost there is transaction discipline on every pooled read, not the policies.
- **Running the dev teardown** — Indy calls it manually. This milestone is proven against local Docker Postgres only; `playbooks/operations/teardown/database/02_teardown.sh` is not invoked by any step here.

---

## Product Clarity (authoring record)

1. **Successful user moment** — a workspace with a year of chatty runs opens its events list and the table paints immediately, because nothing on the page had to carry a megabyte of agent transcript to render a timestamp.
2. **Preserved user behaviour** — the events table renders identically, expanding a row still shows the full body and answer, the charges list and its cursor behave exactly as before, and every identifier a client already holds keeps working.
3. **Optimal-way check** — the direct route to moment #1 is Section 7 alone. Everything else is here because a teardown is the only moment those changes are cheap; the gap to the unconstrained-optimal shape is that bodies still live in Postgres, closed by M155.
4. **Rebuild-vs-iterate** — rebuild, and only because the database is already being torn down. Nothing here trades run-to-run determinism: the metering, fencing and reconciliation paths keep their existing semantics and their existing tests.
5. **What we build** — a re-authored schema from empty; a single-event detail read; a smaller erasure path; corrected indexes.
6. **What we do NOT build** — partitioning, retention, the rollup, payload offload, the outbox. Each is deferred because it is the same cost later and none is answering a measured problem.
7. **Fit with existing features** — this compounds with the operator lease views and the Live Wall, both of which read the tables being corrected. The one thing it must not destabilize is metering: the wallet, ledger and fencing behaviour are load-bearing and their tests are the guard.
8. **Surface order** — API-first: the detail read must exist before the dialog can stop reading bodies off the list row. The User Interface change follows in the same landing.
9. **Dashboard restraint** — no new controls. The accrual endpoint is removed rather than given the drill-down it never got, because shipping a view over data nobody has asked for is the thing this milestone is correcting.
10. **Confused-user next step** — a client still calling the removed accrual endpoint receives a 404 with the registered error code; the charges endpoint it should use instead is named in the changelog entry and the API reference.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** Sections are split by what a reader can verify independently — renumbering, identity, money integrity, the removal, indexes, hygiene, and the read path. One workstream, because a schema cannot be half-rebuilt: the slots, the embed list and the migration array must land together or the database will not boot.
- **Alternatives considered:** (a) *Patch in place* — keep the additive-migration model and fix defects as new slots. Rejected: it preserves the duplicate identity column and the misleading money names permanently, and the teardown window that makes those free does not come again. (b) *Do the full scale programme now* — partitioning, retention, rollup and payload offload in this milestone. Rejected: every number motivating them is a projection, not a measurement; production is not deployed and the working assumption is roughly a hundred runners. Building the machinery now buys insurance against a problem that has never occurred and taxes every query, fixture and test from now on. The middle path — carry the stable partition key, defer the machinery — keeps the option at the cost of one column.
- **Patch-vs-refactor verdict:** this is a **refactor**, justified narrowly. The scope is bounded to work that is genuinely cheaper during a teardown than after it; everything whose cost is the same whenever is named in Out of Scope and pointed at M155.

## Discovery (consult log)

- **Consults** — Architecture / Legacy-Design / gate-flag triage: the question asked + Indy's decision.
- **Metrics review** — events added, extra events found during `/review`, analytics/funnel playbook update or the explicit no-change reason.
- **Skill-chain outcomes** — `/write-unit-test`, `/review`, `kishore-babysit-prs` results (order per `AGENTS.md` CHORE(close); iteration counts, findings dispositioned).
- **Deferrals** — every "deferred to follow-up" needs an **Indy-acked verbatim quote** here, format `> Indy (YYYY-MM-DD HH:MM): "<quote>" — context: <which item, why>`.

  > Indy (2026-07-31): "1. rollit up to this PR" — `billing.usage_rollup`, accepted while believed required; superseded once that premise was withdrawn.

  > Indy (2026-07-31): "Okay go" — acks the revised scope after the authoring agent argued *against* its own earlier recommendation: (a) delete `fleet.metering_periods` and its endpoint rather than partition and retain it — derived data, no product consumer; (b) defer `billing.usage_rollup` — the ledger is already bounded at two rows per event, so the rollup lost its forcing reason; (c) carry `event_created_at` only, deferring partitioning machinery until a measurement demands it.

  > Indy (2026-08-01): "Just fold it into these" — the `vault` / `billing` privilege split (§3.4–3.5) joins this milestone rather than waiting for the Row-Level Security work.

  > Indy (2026-08-01): "Okay we move the RLS to later" — Row-Level Security is a separate milestone. This one is its prerequisite: policies need every protected row to resolve to a tenant, which §3 delivers.

  > Indy (2026-08-02): "I think this diverges the scenario and make our agentsfleetd more thick for no reason which is on even used. If the integration_requests, integration_grants and the autheenthcateFleet are all used for this purpose thsn they must be completely removed." — opens §8. The premise held for the request route and the fleet key; it did **not** hold for `core.integration_grants`, which three internal readers gate on, so the table stays and only the external surface retires.

  > Indy (2026-08-02): "Is there a path for the fleet to ask for approval? that path must stay. Lets build agentsfleeet first and look at external collaboration later. But first justify that the approval isnt broke" — the justification came back negative: no internal origination path exists, and for webhook-triggered fleets the ingress join means no event is ever written. §8 builds the path rather than assuming one.

  > Indy (2026-08-02): "Okay lets do that, but i feel that when the app(github) an approved grant must appear in the Approvals page for for github automatically. … No extra step is needed." — settles the open product question: the gate is raised automatically at install and lands in the workspace Approvals inbox. No command-line flag, no inline prompt, no second confirmation step.

  > Indy (2026-08-02): "Yes continue, but i want the above decision not ina . new spec but in this PR you are on (agentsfleet-m154-schema-rebuild worktree)" — §8 is folded into this spec and this Pull Request rather than authored as M154_003 or M155.

  > Indy (2026-08-02): "I need 80% coverage the gate must be updated" — settles the
  > basis question: `ZIG_COVERAGE_MIN_LINES` moves to 80. Recorded here because the
  > measurement changes with it — today's 62.20% counts `_test.zig` sources as
  > covered code (22,994 of 51,246 measured lines, themselves 70.6% covered), which
  > is what carries the number over its own 60% gate. Production-only is 55.42%.
  > The gate therefore moves to 80% of a corrected basis (test sources excluded,
  > integration lane merged), not 80% of a figure that rises when test files are
  > added.

- **Coverage loss (recorded, not deferred)** — `list_aggregate_integration_test`
  seeded a `billing.usage_ledger` row under a non-identifier fleet id to prove the
  fleet-list aggregate ignored orphan rows. The rebuild makes that unreachable
  twice: `usage_ledger.fleet_id` is UUID, so the driver refuses to encode a
  non-identifier, and `usage_ledger_fleet_id_fkey` references `fleets(id)`, so a
  well-formed id naming no fleet is refused too. The arm is removed with a comment
  saying why it cannot return. The invariant is not lost — it moved out of the
  aggregate query and into the schema, where it is enforced for every writer
  rather than probed by one test. Same treatment as the settle-failure tests
  dropped earlier in this milestone when their fault seam went away.

- **Upstream landing mid-authoring (M149, PR #584)** — the audit behind this spec ran against a tree eleven commits stale. M149 landed before CHORE(open) and invalidated three claims, all corrected above rather than carried: (a) *"nothing is ever pruned"* is false — a thirty-day retention sweep over runner leases and runner events now ships, with a comptime proof it cannot reach live work; it is preserved verbatim and this milestone adds no retention anywhere else. (b) The slot count is forty-five, of which fourteen — not eleven — are patch-only. (c) `schema/043_runner_lifetime_counters.sql` independently reached §2's conclusion for one table and recorded the *correctness* reason this spec had only argued on cost: a second unique key breaks concurrent first-touch upserts, because `ON CONFLICT` can arbitrate only one constraint. §2 now generalises an upstream decision instead of overriding a convention alone.

  **Premise corrections made during authoring** (recorded so a pickup agent does not re-derive them):
  - The ledger is **not** unbounded per event. `renewal.zig` and `renewal_settle.zig` both accumulate into one row via `ON CONFLICT (event_id, charge_type) DO UPDATE … +=`, capping it at **two** rows per event — `charge_type` has exactly two values, `receive` and `stage` (`state/fleet_telemetry_store.zig`). An earlier proposal to make it append-only was withdrawn: it would have turned two rows into thousands.
  - Partitioning must key on the **event's** creation time, not write time. A renewal firing hours after the receive row would otherwise land in a different partition, miss the conflict target, and silently duplicate ledger rows.
  - Oversized-attribute storage already keeps wide event bodies out of the row. The list read is slow because it **selects** them, not because they exist — which is why §7 is a read-path fix, and payload offload is deferred rather than urgent.

- **R6 was failing for a real reason, not a grep artefact (2026-08-03).** A prior handoff classified every R6 hit as comments, negative tests, or a substring collision on `fleet_keyset.zig`, and concluded the *command* needed fixing. Most hits were exactly that. Two were not, and both were live defects: `public/openapi.json` still published `POST /v1/workspaces/{workspace_id}/fleets/{fleet_id}/integration-requests` — 148 lines, `operationId: request_integration_grant` — for a route already gone from `route_table.zig` and `routes.zig`, so the documented API advertised an endpoint that answers 404; and `error_entries_runtime.zig` told operators to call it (`UZ-GRANT-001`) and to mint at the equally-retired `POST /v1/workspaces/{ws}/fleet-keys` (`UZ-APIKEY-001`, whose `agt_a` prefix `auth/api_key.zig` documents as retired with `core.fleet_keys`). All three are fixed here; `public/openapi.json` was already listed in Files Changed, so this closes declared scope rather than widening it. **Dimension 8.5's DONE marker was therefore unearned**: its Test Specification row claims zero references across `public/openapi.json`, `cli/` and `ui/`, but `grant_surface_integration_test.zig` checks `pg_tables`/`pg_indexes` and `@embedFile`s five Zig routing files for retired *symbols* — it never opens the published document. The replacement R6 command was checked against the pre-fix file and does hit the removed block, so it gates rather than passing vacuously.

- **Three rubric rows named commands that do not exist (2026-08-03).** `S1` ran `make test` and `S4` ran `make test-e2e`; neither is a target in this repository, and both predate this branch. `S1` → `make test-unit-all`. `S4` → `make test-unit-app`, with its criterion reworded: **no end-to-end test walks the expand-a-row path.** `tests/e2e/acceptance/events.spec.ts` asserts only that the events page renders an authenticated heading and list; the real coverage of the detail dialog is `tests/event-details-dialog.test.tsx`, a component suite of ten-plus cases including the §7.1 outcome-vs-quoted-answer behaviour. The row now names what is actually proven instead of claiming a tier that does not exist. `R3` and `R6` gained a comment-stripping filter so prose recording a retirement cannot fail a criterion asserting that retirement.

- **`make test-integration` was already red before this session's work (2026-08-03).** An earlier handoff recorded "Integration 793 passed / 7 skipped / 0 failed". That is not reproducible. `make test-integration` exits **2** on `f9e5d656f` with the working tree stashed to pristine: `sessions_integration_test`'s "approval alone never consumes" leaks 3 allocations, identical addresses-count and identical failing step with or without this session's changes. The cause is an allocator-ownership mismatch, not a test defect: `SessionStore.verifyAndConsume` deep-copies its outcome with the store's long-lived `self.alloc` (`session_store_redis.zig`, `borrowed.dupe(self.alloc)`), while `innerVerifyAuthSession` releases it with the per-request arena (`defer outcome.deinit(hx.alloc)`; the arena is built at `server.zig:278-280`). An arena's `free` is a no-op, so the long-lived allocation is never returned — on every successful verify, in production too, since `cmd/serve.zig` wires the store the same way. The correct shape is two functions above it: `innerCreateAuthSession` already frees with `hx.ctx.auth_sessions.alloc`. **S3 cannot grade ✅ until this is fixed**, and it is not caused by the malformed-identifier work recorded below.

  > Indy (2026-08-03): "there is no docs repo update here." — and, on being asked again: "i dont need updates in the docs/changelog.mdx for this." Acks skipping the `~/Projects/docs/changelog.mdx` entry that this spec's own Files Changed table lists as a required EDIT, despite the branch removing a published endpoint and changing a public route from 500 to 404.

- **§7's UI work had left the app package under its own 100% coverage floor (2026-08-03).** `make test-unit-all` failed with every one of its 2160 tests passing — the *threshold* was red, not the suite. Five files, all in this diff, all from the list/detail split: `getFleetEventAction` and `getFleetEvent` were never called by any test; `use-event-detail.ts` had **no test file at all**; `page.tsx`'s `.catch(() => row)` transcript-degrade arm never ran because every existing route test returned `items: []` for `/events`; and `fleet-stream-frames.ts`'s `?? EMPTY_PAYLOAD` fallback — which exists *precisely because* the list carries no bodies — was never taken, since the test factory always supplied `request_json`. The last two are the instructive ones: the branch's central behaviour change was the thing its tests did not exercise. Ten tests close all five (`Statements/Branches/Functions/Lines 100%`, 2170 tests), including percent-encoding on the single-event read so an identifier like `ev/../admin` cannot open a path segment.

- **`make test-unit-all` is environmentally non-deterministic, and one dependency is by design (2026-08-03).** Four runs of S1's command failed four different ways: the app coverage floor above (real, now fixed); `dashboard-workspace`'s `WorkspaceSwitcher > opens the create dialog` exceeding the default 1s `waitFor` under parallel load (passes 28/28 in isolation); a degraded run that silently executed **203 of 213** test files and reported coverage over the partial set; and `http_pin_test`'s "every production secure endpoint primes its certificate state and pins" failing with `unexpected errno: 49` (`EADDRNOTAVAIL`). That last one is not a flake — the test loops `LIVE_TLS_HOSTS` performing real handshakes against production hosts, with retries but no offline skip, so a P0 ship-gate row depends on the developer's network reaching production. The file's own header says it exercises branches "with no network", which describes its *other* tests. Options are to accept S1 as a CI-only gate, guard the sweep with `error.SkipZigTest` on an unreachable-network errno (the shape the integration suites already use for a missing `TEST_DATABASE_URL`), or move it to a lane allowed to need the internet. Unresolved — it needs Indy's call, and it is not caused by this branch.

- **A database-gated test in the unit graph runs in no gate at all (2026-08-03).** Four Dimensions (2.2, 3.2, 6.2, 6.3) had no test, and the obvious home looked like `fleet/schema_migration_test.zig`, which already reads `pg_catalog`. It is the wrong home. `make test-unit-agentsfleetd` never sets `TEST_DATABASE_URL`, and the coverage lane hands it to the **integration** component only — the five unit binaries run without it. So every database-gated test in the unit graph takes the `orelse return error.SkipZigTest` arm in every lane; they are most of the 278 skips the unit run reports. That includes the pre-existing `core key schemas: the retired identity twins are gone and stay gone`, which means **Dimension 2.1's test has never actually executed in a gate**. The four new tests therefore live in `db/schema_shape_integration_test.zig`, registered in `integration_tests.zig` — the only binary the lanes give a live database. Verified executing rather than assumed: filtering on `schema_shape` runs 4 tests over a 3-test baseline, filtering on one test name runs 1, and neither reports a skip.

  Each asserts something non-vacuous before its real claim, because a catalogue that failed to create its constraints would otherwise satisfy a count-the-violations assertion by having nothing to count. The foreign-key test expects **one** allowed exception and names it: `core.model_library (provider, model_id)`, the declared domain key, reached by the platform provider defaults. The other 34 references resolve to a primary key or a superkey containing one.

- **Dimension 2.1's own test is narrower than the Dimension (2026-08-03).** An earlier handoff recorded that `core key schemas: the retired identity twins are gone and stay gone` is "Dimension 2.1 verbatim". It is not. That test counts three named constraints on **one** table (`core.integration_grants`) and one index. Dimension 2.1 claims *no table* declares a generated identity column or a unique constraint duplicating its primary key, across the whole catalogue. R2's grep covers the `GENERATED ALWAYS` half at source level; the duplicate-unique half is asserted nowhere, and the test that looked like it does never runs (above). Recorded rather than marked DONE.

- **The Continuous Integration coverage job could not have worked, and had never run to find out (2026-08-03).** `test.yml`'s `test-coverage-zig` ran inside a job-level `container:`. That was correct while coverage measured only the five unit binaries; it stopped being correct when the target began measuring the integration binary too, which needs a live Postgres and Redis. A GitHub Actions job container is placed on a network Actions manages and rejects `--network host`, so it cannot reach datastores published on the runner's localhost — the constraint `test-integration.yml` already documents and resolves with `docker run`. The job now follows that same pattern: same image, same digest pin, kcov's `seccomp=unconfined` and `SYS_PTRACE` moved onto the `docker run`, plus `timeout-minutes: 30` and a `docker compose down` teardown, since the datastores are booted by the job rather than by a service block Actions cleans up. `workflow_dispatch` is added alongside so a long branch can exercise these lanes before review — the reason this defect survived 40+ commits is that every trigger required a Pull Request or a merge to `main`. `make check-gh-actions-valid` passes (actionlint + make-target references).

- **`GET /v1/auth/sessions/{id}` and `POST /v1/auth/sessions/{id}/verify` answered 500 on an unrecognisable identifier (2026-08-03).** `formatSessionKey` returned a bare `error.InvalidSessionId`, which is not a member of `session_store.Error`; `failFromStoreError` switches over that set and routes anything unclassified through its `else` arm to `ERR_INTERNAL_OPERATION_FAILED`. So a caller's typo reported as a server fault **by construction**, on the two routes that carry no authentication. The two authenticated routes were already correct — they are the two that wrote `catch return Error.SessionMissing`. Fixed at the source rather than by adding a fourth `catch`: `formatSessionKey` now reports `Error.SessionMissing`, which fixes `/verify` through the shared mapper, and `innerPollAuthSession` now uses that mapper like the other four session handlers instead of a hand-rolled 500. Covered by a table-driven route test over four identifier shapes (too short, version 4, hyphens stripped, variant nibble out of range) asserting exact 404 — exact, because a `>= 400` assertion passes on the very 500 being removed.
