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
| `docs/architecture/data_flow.md` | EDIT | Records the list/detail split and the retained partition option |
| `~/Projects/docs/changelog.mdx` | EDIT | User-visible: an endpoint is removed, the events list gets faster |

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

- **Dimension 1.1** — every patch-only slot's effect is present in the base statement it patched, and the patch slot is gone → Test `test_no_alter_or_drop_statements_in_schema`
- **Dimension 1.2** — the named slot-version constants in `common.zig` resolve to the renumbered slots that carry those tables (RULE MIG) → Test `test_named_migration_versions_match_slots`
- **Dimension 1.3** — a database bootstrapped from empty applies every slot in order with no failure → Test `test_bootstrap_from_empty_applies_all_slots`

### §2 — One identity key per table

Roughly twenty-two tables carry `uid` plus a duplicate twin holding the same value, costing two btree indexes on the same sixteen bytes and sixteen duplicated heap bytes per row — on the unbounded tables permanently. Nothing reads `uid`: every query selects the twin, and every foreign key points at the `UNIQUE` side.

The waste is the smaller half of the argument. The **correctness** half is recorded upstream in `schema/043_runner_lifetime_counters.sql`, which deliberately ships one unique index because a second one breaks concurrent first-touch upserts: `ON CONFLICT` arbitrates exactly one constraint, so two sessions inserting a brand-new row race to a duplicate-key error on the *other* index instead of taking the update arm. Every table still carrying the dual-key shape has that latent race. This Section generalises the decision slot 043 already made table-by-table. **Implementation default:** one column named `id`, because that is the name the API already exposes and the name every foreign key should reference.

- **Dimension 2.1** — no table declares a generated identity column or a unique constraint duplicating its primary key → Test `test_no_duplicate_identity_columns`
- **Dimension 2.2** — every foreign key references a primary key, not a secondary unique constraint → Test `test_foreign_keys_reference_primary_keys`
- **Dimension 2.3** — identifiers remain application-generated version 7, and the public field name is unchanged at the API boundary → Test `test_generated_ids_are_uuidv7_and_api_shape_unchanged`

### §3 — Money consolidated behind referential integrity

Money lives in three schemas and the ledger carries `workspace_id` and `fleet_id` as text with no foreign key — which is why the counter trigger runs a regular expression on every renewal, and why erasure needs a hand-maintained delete order. Consolidating the wallet and ledger into `billing` with real foreign keys makes ownership a database fact. The *privilege* half of the same idea — `api_runtime` holding direct grants on the wallet and the secret store — is M154_002, landing in this PR.

- **Dimension 3.1** — the ledger resolves to a tenant, a workspace and a fleet through foreign keys, all typed as identifiers → Test `test_ledger_rows_resolve_to_tenant_by_foreign_key`
- **Dimension 3.2** — the counter trigger carries no pattern match, because its input is no longer text → Test `test_counter_trigger_has_no_regex`
- **Dimension 3.3** — erasing a tenant leaves zero rows anywhere, with the explicit delete order reduced to what no cascade covers → Test `test_tenant_erasure_leaves_no_rows`

### §4 — Retire the accrual detail table

`fleet.metering_periods` gains a row per renewal — roughly one per twenty seconds of every run — and is read by one endpoint that no product surface calls. It is derived data: the ledger row already carries the accumulated total the wallet reconciles against, and the table cannot identify its own owner without joining the ledger to do it. It is removed rather than carried, partitioned, and given a retention policy. The per-slice detail becomes an M155 concern on the durable event stream.

- **Dimension 4.1** — the table, its store, its handler and its endpoint are gone, with no orphaned reference → Test `test_accrual_surface_fully_removed`
- **Dimension 4.2** — the wallet drain still reconciles against the ledger across a metered run with renewals → Test `test_wallet_reconciles_to_ledger_without_accrual_table`
- **Dimension 4.3** — the ledger carries the originating event's creation time, so a later partitioning decision has a stable key available → Test `test_ledger_carries_event_created_at`

### §5 — Correct the indexes

Three indexes serve no live query and one is short by the column its only reader needs. Each surviving index states the query it serves and the growth that justifies it, per the standard slot 033 set.

- **Dimension 5.1** — no index exists without a named reader; the two ledger indexes whose reader was deleted are gone → Test `test_every_index_has_a_named_reader`
- **Dimension 5.2** — the tenant charges keyset seeks without a sort node, because its index carries the tiebreak column (RULE KYS) → Test `test_tenant_charges_keyset_plan_has_no_sort`
- **Dimension 5.3** — the memory retention sweep is served by a composite leading with the fleet identifier rather than a bare low-cardinality column → Test `test_memory_retention_sweep_uses_composite_index`

### §6 — Schema hygiene

Value literals moved out of `DEFAULT` and `CHECK` under RULE STS but reappeared in partial-index predicates and trigger bodies, where they drift from the application constants exactly the same way. A schema is granted and revoked but holds no tables. Timestamp columns use four different names for the same concept.

- **Dimension 6.1** — every surviving literal in an index predicate or trigger body names the application constant it mirrors, or the predicate is gone → Test `test_schema_literals_carry_named_provenance`
- **Dimension 6.2** — no schema is created that holds no tables → Test `test_no_empty_schemas`
- **Dimension 6.3** — row lifecycle timestamps are named consistently across every table → Test `test_timestamp_column_naming_is_uniform`

### §7 — The events list stops reading bodies

The list select carries the event body and the full agent answer on every row, up to two hundred per page, to render a table of timestamps, statuses and costs. The bodies are only needed when a row is expanded. Splitting list from detail removes the cost from the common read; dropping the lease's duplicate copy of the same body removes it from the write path too. **Implementation default:** reclaim reads the body by joining the event row on its existing unique key — both tables cascade from the same parent, so the join cannot dangle.

- **Dimension 7.1** — the list query selects no body column, and the rendered table is unchanged → Test `test_events_list_selects_no_payload_columns`
- **Dimension 7.2** — a single-event detail read returns the body and the answer, scoped to the caller's workspace → Test `test_event_detail_returns_body_scoped_to_workspace`
- **Dimension 7.3** — the lease carries no body copy, and reclaiming an expired lease still re-delivers the original event → Test `test_reclaim_redelivers_event_without_lease_payload_copy`
- **Dimension 7.4** — reclaim's lifetime-tally arm still rides the same statement as the status flip, so the counter cannot drift from the rows it counts → Test `test_reclaim_tally_stays_in_the_status_flip_statement`

## Interfaces

```
REMOVED    GET /v1/tenants/me/billing/charges/{event_id}/telemetry
           Per-renewal accrual detail; no product surface calls it. Pre-2.0, so
           removed outright rather than answering 410.

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

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit | `test_no_alter_or_drop_statements_in_schema` | Scanning every file under `schema/` yields zero `ALTER TABLE` and zero `DROP` statements |
| 1.2 | unit | `test_named_migration_versions_match_slots` | Each named slot-version constant resolves to the renumbered slot creating that table |
| 1.3 | integration | `test_bootstrap_from_empty_applies_all_slots` | A torn-down database applies every slot in order; recorded version equals the last slot |
| 2.1 | unit | `test_no_duplicate_identity_columns` | Catalogue query returns zero generated identity columns and zero uniques duplicating a primary key |
| 2.2 | integration | `test_foreign_keys_reference_primary_keys` | Every foreign key's referenced columns equal the referenced table's primary key |
| 2.3 | unit | `test_generated_ids_are_uuidv7_and_api_shape_unchanged` | Generated identifiers carry version nibble 7; a fleet response still exposes `id` |
| 3.1 | integration | `test_ledger_rows_resolve_to_tenant_by_foreign_key` | Inserting a ledger row for an unknown tenant is rejected by the database, not by the handler |
| 3.2 | unit | `test_counter_trigger_has_no_regex` | The counter trigger body contains no pattern-match operator |
| 3.3 | integration | `test_tenant_erasure_leaves_no_rows` | After erasing a tenant with fleets, events and charges, every table enumerated from the catalogue holds zero rows for it |
| 4.1 | unit | `test_accrual_surface_fully_removed` | Zero references to the accrual table, store, handler or operation identifier anywhere in the tree |
| 4.2 | integration | `test_wallet_reconciles_to_ledger_without_accrual_table` | A run with three renewals plus settle: wallet delta equals the summed ledger rows exactly |
| 4.3 | integration | `test_ledger_carries_event_created_at` | Every ledger row for one event carries the same originating creation time across renewals |
| 5.1 | unit | `test_every_index_has_a_named_reader` | Each created index cites a query in its slot; zero uncited indexes |
| 5.2 | integration | `test_tenant_charges_keyset_plan_has_no_sort` | The cursor-paged charges plan is an index scan with no sort node |
| 5.3 | integration | `test_memory_retention_sweep_uses_composite_index` | The aged-category delete plans on the composite, not a low-cardinality single column |
| 6.1 | unit | `test_schema_literals_carry_named_provenance` | Every literal in an index predicate or trigger body names its application constant |
| 6.2 | integration | `test_no_empty_schemas` | Every created schema holds at least one table |
| 6.3 | unit | `test_timestamp_column_naming_is_uniform` | Lifecycle timestamp columns use one naming form across every table |
| 7.1 | integration | `test_events_list_selects_no_payload_columns` | A page of 200 events returns no body or answer field; the plan reads no oversized-attribute storage |
| 7.2 | e2e | `test_event_detail_returns_body_scoped_to_workspace` | Expanding a row fetches the body; the same identifier from another workspace answers 404 |
| 7.3 | integration | `test_reclaim_redelivers_event_without_lease_payload_copy` | An expired lease is reclaimed and the re-delivered event body matches the original byte for byte |
| 7.4 | integration | `test_reclaim_tally_stays_in_the_status_flip_statement` | Reclaiming N leases increments the expired tally by exactly N, with no separate statement |
| regression | integration | `test_charges_response_shape_unchanged` | The charges endpoint returns the same fields and cursor semantics as before the rebuild |
| regression | e2e | `test_events_list_renders_identically` | The rendered events table is unchanged after the payload columns leave the list read |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Schema carries no patch statements (§1, §6) | `grep -rnE 'ALTER TABLE\|DROP (TABLE\|INDEX\|SCHEMA)' schema/` | no output | P0 | |
| R2 | No table carries a duplicate identity column (§2) | `grep -rn 'GENERATED ALWAYS' schema/` | no output | P0 | |
| R3 | The accrual surface is gone (§4) | `grep -rn 'metering_periods\|get_tenant_metering_periods\|slice_seq' src/ ui/ public/openapi.json` | no output | P0 | |
| R4 | The list read carries no body columns (§7) | `grep -n 'request_json\|response_text' src/agentsfleetd/state/fleet_events_store.zig` | no output | P0 | |
| R5 | Diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 paths missing from the Files Changed table | P0 | |
| S1 | Unit tests pass | `make test` | exit 0 | P0 | |
| S2 | Lint clean | `make lint-all` | exit 0 | P0 | |
| S3 | Integration passes (schema, HTTP and Redis touched) | `make test-integration` | exit 0 | P0 | |
| S4 | e2e walks the expand-a-row path | `make test-e2e` | exit 0 | P0 | |
| S5 | No leaks (allocator wiring touched) | `make memleak` | exit 0 | P0 | |
| S6 | Cross-compile | `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` | exit 0 | P0 | |
| S7 | No secrets | `gitleaks detect` | exit 0 | P0 | |
| S8 | Orphan sweep | Dead Code Sweep greps below | 0 matches | P0 | |

**Grading protocol (VERIFY):** run the Verify command verbatim; grade ONLY from its output. Graded = ✅/❌ + the one decisive output line (`342 passed`); long evidence goes to PR Session Notes with a pointer here. **Ship gate:** every row graded, every P0 ✅ → eligible for CHORE(close); any ❌ or empty cell → return to EXECUTE; a P1 ❌ ships only with an Indy-acked deferral quote in Discovery.

## Dead Code Sweep

**1. Orphaned files — deleted from disk and git.**

| File to delete | Verify |
|----------------|--------|
| `src/agentsfleetd/state/fleet_metering_store.zig` | `test ! -f src/agentsfleetd/state/fleet_metering_store.zig` |
| every retired `schema/0NN_*.sql` slot (001–046) | `test -z "$(ls schema/0*.sql 2>/dev/null)"` |

**2. Orphaned references — zero remaining imports/uses.**

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| `metering_periods`, `fleet_metering_store` | `grep -rnE -w "metering_periods\|fleet_metering_store" src/ ui/ public/ \| head` | 0 matches |
| `fleet_execution_telemetry` | `grep -rn -w "fleet_execution_telemetry" src/ \| head` | 0 matches |
| `uid` | `grep -rn -w "uid" src/ schema/ \| head` | 0 matches |

## Out of Scope

- **Payload offload to object storage** — event bodies stay in Postgres this milestone; the content-hash offload lands in M155 beside the outbox.
- **Partitioning** — only the stable partition key is carried. The machinery is deferred until a measurement demands it; the rationale is in Decomposition.
- **New retention policies** — M149 already ships a thirty-day sweep over runner leases and runner events, with a comptime proof that an age-keyed sweep cannot reach live work. That behaviour, its window, and its proof are **preserved unchanged**; this milestone adds no retention anywhere else.
- **`billing.usage_rollup`** — deferred with partitioning; the ledger is already bounded per event, so the rollup is an optimisation without a forcing reason today.
- **Durable outbox to Elastic or Loki** — M155, together with the accrual detail this milestone removes from Postgres. Approval gates get no retention policy at all until Indy sets one; the table is a compliance record.
- **Horizontal sharding** — explicitly rejected: every hot query is already tenant, workspace or fleet scoped, and write rate sits far below single-node capacity. Partitioning addresses retention, never throughput.
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

  > Indy (2026-07-31): "Okay go" — acks the revised scope after the authoring agent argued *against* its own earlier recommendation: (a) delete `fleet.metering_periods` and its endpoint rather than partition and retain it — derived data, no product consumer; (b) defer `billing.usage_rollup` to M155 — the ledger is already bounded at three rows per event, so the rollup lost its forcing reason; (c) carry `event_created_at` only, deferring partitioning machinery until a measurement demands it.

  > Indy (2026-08-01): "Just fold it into these" — the `vault` / `billing` privilege split (§3.4–3.5) joins this milestone rather than waiting for the Row-Level Security work.

  > Indy (2026-08-01): "Okay we move the RLS to later" — Row-Level Security is a separate milestone. This one is its prerequisite: policies need every protected row to resolve to a tenant, which §3 delivers.

- **Upstream landing mid-authoring (M149, PR #584)** — the audit behind this spec ran against a tree eleven commits stale. M149 landed before CHORE(open) and invalidated three claims, all corrected above rather than carried: (a) *"nothing is ever pruned"* is false — a thirty-day retention sweep over runner leases and runner events now ships, with a comptime proof it cannot reach live work; it is preserved verbatim and this milestone adds no retention anywhere else. (b) The slot count is forty-five, of which fourteen — not eleven — are patch-only. (c) `schema/043_runner_lifetime_counters.sql` independently reached §2's conclusion for one table and recorded the *correctness* reason this spec had only argued on cost: a second unique key breaks concurrent first-touch upserts, because `ON CONFLICT` can arbitrate only one constraint. §2 now generalises an upstream decision instead of overriding a convention alone.

  **Premise corrections made during authoring** (recorded so a pickup agent does not re-derive them):
  - The ledger is **not** unbounded per event. `renewal.zig` and `renewal_settle.zig` both accumulate into one row via `ON CONFLICT (event_id, charge_type) DO UPDATE … +=`, capping it at three rows per event. An earlier proposal to make it append-only was withdrawn — it would have turned three rows into thousands.
  - Partitioning must key on the **event's** creation time, not write time. A renewal firing hours after the receive row would otherwise land in a different partition, miss the conflict target, and silently duplicate ledger rows.
  - Oversized-attribute storage already keeps wide event bodies out of the row. The list read is slow because it **selects** them, not because they exist — which is why §7 is a read-path fix, and payload offload is deferred rather than urgent.
