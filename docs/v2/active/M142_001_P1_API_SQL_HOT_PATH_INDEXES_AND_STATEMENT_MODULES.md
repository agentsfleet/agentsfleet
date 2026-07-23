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

# M142_001: Every recurring control-plane read is served by an index

**Prototype:** v2.0.0
**Milestone:** M142
**Workstream:** 001
**Date:** Jul 23, 2026
**Status:** IN_PROGRESS
**Priority:** P1 — background sweeps re-scan whole tables on every cycle, so idle database load grows with fleet and runner count while no user is waiting on it
**Categories:** API
**Batch:** B1 — single workstream; nothing else in this milestone
**Branch:** feat/m142-sql-hot-path-indexes
**Test Baseline:** unit=2827 integration=378
**Depends on:** none — disjoint from M141_001_P0_API_DOCS_OBS_BOUNDED_RUNNER_LEASE_FANOUT; no file appears in both Files Changed tables, and M141 declares no schema change, so every index here stays unowned by it
**Provenance:** Large Language Model (LLM)-drafted (Claude Opus 4.8, Jul 23, 2026) from an exhaustive read of all 32 `schema/*.sql` files, the 11 existing `sql.zig` modules, and all 68 inline-Structured Query Language (SQL) production modules at `main` `5f3649947`
**Canonical architecture:** `docs/architecture/scaling.md` §Per-request volume; `docs/architecture/memory.md` §Storage

---

## Overview

**Goal (testable):** No recurring control-plane read performs a sequential scan or sorts an unbounded row set, and at least 80% of SQL-carrying data-access modules route their statement text through a sibling `sql.zig`.

**Problem:** Operators pay database load that no user requested. Background sweeps re-read whole tables every cycle, list endpoints sort result sets the database could have returned pre-ordered, and one credential list issues a query per stored credential. Separately, an event filter ending in a backslash returns a 500 instead of an empty page. None of it is visible as a feature regression — it surfaces as connection pressure and latency that grow with the size of the account rather than with traffic.

**Solution summary:** Nine indexes land in one additive migration, each justified by a named query that today scans or sorts without one. Three list reads are restructured so the expensive per-row work happens after pagination rather than before it, and the credential list collapses to a single query. The filter defect is fixed by escaping the backslash the translator currently passes through. Alongside, inline statement text moves into per-domain `sql.zig` modules — the pattern already used by eleven domains — until at least 80% of the data-access layer follows it, with a checker wired into repository conformance so adoption cannot regress. No wire protocol, response shape, or stored data changes.

## PR Intent & comprehension handshake

- **PR title (eventual):** perf(db): index the recurring control-plane reads and centralise SQL text
- **Intent (one sentence):** An operator's database load tracks the work their account is actually doing, not the number of rows it has accumulated.
- **Handshake** — the implementing agent fills this at PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `docs/SCHEMA_CONVENTIONS.md` §Migration Model — additive migrations only; shipped slot files are frozen history. The migration here appends a slot and never edits one.
2. `src/agentsfleetd/http/handlers/fleets/sql.zig` — the reference shape for §5. Statement constants with the reasoning in doc comments, no logic, no allocation. Every module extracted in this spec mirrors it.
3. `src/agentsfleetd/fleet/liveness_sweeper.zig` — two of the worst reads live here; read `fetchDueRunners` and `expireActiveLeaseSlots` together to see why one is a scan and the other is a scan per due runner.
4. `src/agentsfleetd/state/fleet_events_filter.zig` §`globToLike` — the escape defect in §4. The translator handles `%` and `_` and passes `\` through.
5. `scripts/check_zig_discipline_test.py` — the convention for a repository checker with its own test; §5's adoption checker mirrors its structure and its wiring into conformance.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `schema/033_hot_path_indexes.sql` | CREATE | The ten indexes, each with the query that justifies it named in a comment. |
| `schema/embed.zig` | EDIT | Register the new slot. This is the ONLY registration edit — see the correction below. |
| `scripts/check_sql_statement_modules.py` | CREATE | Compute and gate statement-module adoption. |
| `scripts/check_sql_statement_modules_test.py` | CREATE | Prove the checker's denominator, exclusions, and threshold behaviour. |
| `make/lint.mk` | EDIT | Wire the adoption checker into repository conformance. |
| `src/agentsfleetd/http/handlers/fleet/runners_list.zig` | EDIT | Evaluate the lease-liveness check after pagination, not before. |
| `src/agentsfleetd/http/handlers/fleets/secret_list.zig` | EDIT | Collapse the per-credential load into one query. |
| `src/agentsfleetd/http/handlers/api_keys/list.zig` | EDIT | Order through the new index. |
| `src/agentsfleetd/state/fleet_events_filter.zig` | EDIT | Escape the backslash in the glob translator. |
| `src/agentsfleetd/state/fleet_events_filter_test.zig` | EDIT | Cover the trailing-backslash and literal-backslash cases. |
| `src/agentsfleetd/db/index_usage_integration_test.zig` | CREATE | Prove each new index is chosen by the planner under a seeded workload. |
| `src/agentsfleetd/http/handlers/fleet/runners_list_integration_test.zig` | EDIT | Prove the liveness check is bounded by page size. |
| `src/agentsfleetd/http/handlers/fleets/secret_list_integration_test.zig` | EDIT | Prove query count is independent of credential count. |
| `src/agentsfleetd/state/<domain>/sql.zig` | CREATE | Statement modules for the store layer; one per domain currently carrying inline SQL. |
| `src/agentsfleetd/fleet/sql.zig` | CREATE | Statement module for the fleet store domain. |
| `src/agentsfleetd/fleet_runtime/sql.zig` | CREATE | Statement module for the approval-gate domain. |
| `src/agentsfleetd/memory/sql.zig` | CREATE | Statement module for the fleet-memory domain. |
| `src/agentsfleetd/http/handlers/<domain>/sql.zig` | CREATE | Statement modules for handler domains carrying three or more statements. |
| `src/agentsfleetd/state/*.zig` | EDIT | Import statement text rather than carrying it inline. |
| `src/agentsfleetd/fleet/*.zig` | EDIT | Same, for the fleet store domain. |
| `src/agentsfleetd/fleet_runtime/*.zig` | EDIT | Same, for the approval-gate domain. |
| `src/agentsfleetd/memory/*.zig` | EDIT | Same, for the fleet-memory domain. |
| `src/agentsfleetd/http/handlers/**/*.zig` | EDIT | Same, for handler domains meeting the three-statement threshold. |
| `src/agentsfleetd/tests.zig` | EDIT | Register the new suites. |
| `docs/architecture/scaling.md` | EDIT | State which recurring reads are index-served and what that bounds. |

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — **NSQ** (extracted statements stay schema-qualified; no magic numbers in the new checker), **STS** (the migration adds no `DEFAULT` literal and no `CHECK` value list; partial-index predicates follow the existing precedent in slots 005, 007, and 021), **SGR** (no new table, so no new grant; index creation needs none), **MIG** (additive slot, `IF NOT EXISTS` guards, idempotent against fresh bootstrap and an already-provisioned database), **UFS** (the adoption threshold and the checker's exclusion list are named constants), **NDC** (no unreached extraction shim ships), **NLR** (a module touched for extraction gets its inline statements moved, not partially moved), **NLG** (no dual-path fallback where a module reads statements from two places), **ORP** (every moved constant swept for stale references), **FLS** (`PgQuery` with `defer q.deinit()` preserved verbatim through every extraction), **TFX** (fixtures keep their inline SQL and stay out of the denominator), **GRD** (every index claim is grounded in a named query, not in a general principle).
- **`dispatch/write_sql.md`** — read before the migration; `docs/SCHEMA_CONVENTIONS.md` is the source of truth for the slot's shape and its registration in both the embed file and the migration array.
- **`dispatch/write_zig.md`** — public-surface shape verdict for each new `sql.zig`, file and function length caps, both Linux target builds.
- **`dispatch/write_any.md`** — logging standard, error registry, source length, milestone-free test naming.
- **`dispatch/write_python.md`** — standard-library only in the adoption checker, context-managed file reads, specific exceptions.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | yes | format, lint, unit suites, and both Linux target builds before COMMIT |
| PUB / Struct-Shape | yes | declare a FILE SHAPE DECISION for each new `sql.zig`; every one is a flat constant surface with no public function |
| File & Function Length (≤350/≤50/≤70) | yes | extraction shrinks the modules it touches; each new `sql.zig` stays a constant surface and splits by domain before approaching the cap |
| UFS (repeated/semantic literals) | yes | adoption threshold, exclusion list, and index names are named constants in the modules that own them |
| User Interface (UI) Substitution / DESIGN TOKEN | no | no TypeScript or design-system surface is touched |
| LOGGING / LIFECYCLE / ERROR REGISTRY / SCHEMA | schema | SCHEMA GUARD fires on the new slot; §2 additionally requires recorded index-usage evidence and owner approval before any `DROP INDEX` |

## Prior-Art / Reference Implementations

- **Reference:** `src/agentsfleetd/http/handlers/fleets/sql.zig` and `src/agentsfleetd/fleet_library/sql.zig` — the in-repo statement-module pattern, already carrying 81 statements across eleven domains. §5 extends it rather than introducing a second convention.
- **Reference:** `schema/030_fleet_activity_counters.sql` — the precedent for a migration that exists purely to remove read-time work, including how it states the measured cost it replaces.
- **Reference:** `scripts/check_zig_discipline.py` with `scripts/check_zig_discipline_test.py` — the repository-checker pattern the adoption gate mirrors: standard library only, its own test, wired into conformance.

## Sections (implementation slices)

### §1 — Index the recurring reads

Nine indexes, each tied to a query that scans or sorts today. The two sweeper reads are the reason this is P1: `expireActiveLeaseSlots` filters `runner_affinity` on an unindexed column once per due runner per cycle, and `fetchDueRunners` orders `runners` by an unindexed column every cycle. Both are pure background cost. The remainder serve list reads and unindexed cascade paths.

**Implementation default:** one migration slot for all nine, because they share a justification and a rollback; splitting them across slots would make the evidence in §1's test harder to read than the change itself.

- **Dimension 1.1** — the slot applies cleanly to a fresh bootstrap and to an already-provisioned database, twice in a row → Test `test_migration_slot_is_idempotent` — **DONE**
- **Dimension 1.2** — the affinity-expiry sweep is planned as an index scan → Test `test_affinity_expiry_uses_index` — **DONE**
- **Dimension 1.3** — the due-runner read satisfies both its filter and its ordering from an index → Test `test_due_runner_read_is_index_ordered` — **DONE**
- **Dimension 1.4** — a bounded memory read over a fleet's entries is served pre-ordered by the composite index, with no sort node → Test `test_bounded_memory_read_is_index_ordered` — **amended at EXECUTE against measured plans; see Discovery. The original wording ("memory hydration returns rows pre-ordered") is not achievable: `fleet_memory.listAll` fetches a fleet's whole memory set with no `LIMIT`, and for an unbounded fetch PostgreSQL correctly prefers bitmap-scan-plus-sort whether or not the redundant narrow index is present.**
- **Dimension 1.5** — the workspace event keyset resolves as a single index seek rather than a seek plus filter → Test `test_event_keyset_is_index_seek` — **DONE**
- **Dimension 1.6** — the reclaim lease lookup and the fleet-list page each plan against their new index → Test `test_reclaim_and_fleet_list_use_indexes` — **DONE**

### §2 — Retire the indexes nothing reads

Three indexes cost write throughput and return nothing. `idx_api_keys_key_hash_active` duplicates the equality lookup that `api_keys_hash_uniq` already serves on the authentication path; `idx_memory_entries_fleet_id` becomes a prefix of §1's composite; `idx_memory_entries_category` filters a column never queried without `fleet_id` alongside it.

Removal is gated: the Schema Table Removal Guard requires owner approval, and no index is dropped on reasoning alone. This section carries recorded `pg_stat_user_indexes` scan counts under the seeded workload as its evidence, and it is severable — §1 ships whether or not §2 is approved.

**Implementation default:** the drops land in their own slot, separate from §1, so approval on the additive work is never blocked by a pending decision on the removals.

- **Dimension 2.1** — each index proposed for removal records zero scans across the seeded workload → Test `test_redundant_indexes_record_no_scans`
- **Dimension 2.2** — the authentication lookup still plans against the unique index after the drop → Test `test_api_key_auth_lookup_survives_drop`

### §3 — Do the expensive work after pagination, not before

Three list reads pay per-row cost across the whole result set to return one page. The runner list evaluates a lease-liveness subquery for every runner in the fleet before applying `LIMIT`. The credential list issues one decrypt-load per stored credential. The api-key list sorts on columns no index covers.

**Implementation default:** the runner list keeps its total-count semantics; only the position of the liveness evaluation moves. Changing what the endpoint reports is out of scope here.

- **Dimension 3.1** — the liveness check is evaluated only for rows on the returned page → Test `test_runner_list_liveness_bounded_by_page`
- **Dimension 3.2** — the credential list issues a fixed number of queries regardless of credential count → Test `test_secret_list_query_count_is_constant`
- **Dimension 3.3** — the api-key list satisfies its ordering from an index → Test `test_api_key_list_is_index_ordered`
- **Dimension 3.4** — all three endpoints return byte-identical payloads to the current implementation for the same fixture → Test `test_list_payloads_unchanged`

### §4 — An event filter never returns a 500 for user input

`globToLike` escapes `%` and `_` but passes `\` through unchanged. A filter ending in a backslash produces a pattern ending in an escape character, which PostgreSQL rejects with `22025`, surfacing as a 500. A filter containing an interior backslash silently escapes the following character instead of matching a literal one.

- **Dimension 4.1** — an actor filter ending in a backslash returns a normal page, never a server error → Test `test_actor_filter_trailing_backslash_is_literal`
- **Dimension 4.2** — an interior backslash matches a literal backslash rather than escaping its successor → Test `test_actor_filter_interior_backslash_is_literal`

### §5 — Statement text lives in statement modules

Eleven domains already keep their SQL in a sibling `sql.zig`; sixty-eight production modules still carry it inline, which is why an audit of the query surface has to read sixty-eight files. Extraction is pure movement: statement text moves, call sites import it, behaviour does not change.

The denominator is the store layer plus handler domains carrying three or more statements. Test fixtures, the migration bootstrap, and the two metering statements in `renewal.zig` and `renewal_settle.zig` are excluded — the first two are inline by design, and the metering statements are the most correctness-critical text in the repository with nothing to gain from moving.

**Implementation default:** one module per domain directory rather than per file, matching every existing `sql.zig`, so a domain's statements stay greppable from one place.

- **Dimension 5.1** — the checker reports adoption as a ratio and exits non-zero below the threshold → Test `test_adoption_checker_gates_on_threshold`
- **Dimension 5.2** — the checker's denominator excludes fixtures, migrations, and the named metering modules → Test `test_adoption_checker_honours_exclusions`
- **Dimension 5.3** — every statement module is a constant surface with no function and no allocation → Test `test_statement_modules_are_constant_surfaces`
- **Dimension 5.4** — adoption across the data-access layer is at or above the threshold → Test `test_adoption_meets_threshold`

## Interfaces

```text
HTTP surface — UNCHANGED.
  GET /v1/fleets/runners, GET /v1/api-keys, GET /v1/workspaces/{id}/secrets,
  and the fleet-events reads keep their request parameters, response bodies,
  and status codes. §3 moves where work happens, not what is returned.

Database surface
  schema/033 — additive index slot. No table, column, constraint, or grant.
  §2 removals, if approved, land in their own slot.

scripts/check_sql_statement_modules.py
  stdout: "<extracted>/<total> (<pct>%)"
  exit 0 at or above threshold; exit 1 below; exit 2 on unreadable input.

sql.zig modules (internal)
  Public surface is statement constants only. No function, no allocation,
  no import beyond what a constant expression needs.
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Migration applied twice | Re-run against a provisioned database | `IF NOT EXISTS` guards make every statement a no-op; the version row is unchanged and no error surfaces. |
| Index created but not chosen | Planner prefers a sequential scan on a small seeded table | The index-usage test seeds enough rows to make the index the cheaper plan and asserts the plan, so a merely-created index fails the rubric rather than passing it. |
| Index creation locks a live table | Building against a populated production table | **Corrected at EXECUTE:** the runner cannot build concurrently. `pool_migrations.zig` wraps every slot in `BEGIN`/`COMMIT`, and PostgreSQL rejects `CREATE INDEX CONCURRENTLY` inside a transaction block, so each build takes a ShareLock for its duration. On a large table the operator creates the indexes by hand outside the migration first; the `IF NOT EXISTS` guards then make the slot a no-op. |
| Write throughput regresses | Nine new indexes on tables with hot insert paths | The seeded workload measures insert cost before and after; a regression past the recorded bound returns the offending index to EXECUTE. |
| A removal candidate is live | An index §2 proposes to drop is used by an unmeasured path | Removal is gated on recorded zero scans plus owner approval; absent either, §2 is dropped and §1 ships alone. |
| Extraction changes statement text | A statement is edited while being moved | The integration suite runs unmodified across §5's commit; any behavioural difference fails it. |
| Extraction breaks result drainage | A moved statement loses its `PgQuery` drain pairing | `make check-pg-drain` fails the conformance run before COMMIT. |
| Checker miscounts | A module matches the denominator heuristic but holds no statement | The checker's own test pins the denominator against known-good and known-bad fixtures. |
| Filter escape over-corrects | Escaping the backslash breaks an existing glob | The filter's existing tests run unchanged alongside the two new cases. |

## Invariants

1. The migration adds no table, column, constraint, or grant — enforced by the SCHEMA GUARD output and asserted by a test reading the slot for prohibited statements.
2. Every index in the slot is chosen by the planner for its named query — enforced by the index-usage integration test, which asserts on the plan, not on the index's existence.
3. No index is dropped without a recorded zero scan count and owner approval — enforced by §2 being severable and its evidence being a rubric row.
4. Extraction changes no statement text — enforced by the integration suite passing with zero test-file modifications in §5's commit.
5. Every statement module is a constant surface — enforced by the adoption checker, which fails on any function or allocation in a `sql.zig`.
6. Adoption never falls below the threshold — enforced by the checker running inside repository conformance.
7. Every HTTP response body is byte-identical to the current implementation for the same fixture — enforced by the payload-comparison test in §3.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| not applicable | not applicable | no product or operator signal changes | not applicable | not applicable | not applicable |

This workstream changes no product-analytics event, no operator metric, and no funnel. Poll-cost and readiness telemetry belong to M141_001 and are not duplicated here. No analytics or funnel playbook update is required.

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | integration | `test_migration_slot_is_idempotent` | applying the slot to a fresh and to a provisioned database, twice each, leaves identical index sets and raises nothing. |
| 1.2 | integration | `test_affinity_expiry_uses_index` | with a seeded affinity population, the sweep's expiry statement plans as an index scan on the runner column. |
| 1.3 | integration | `test_due_runner_read_is_index_ordered` | the due-runner read plans without a sort node and reads no more rows than its batch bound. |
| 1.4 | integration | `test_bounded_memory_read_is_index_ordered` | a bounded read over a fleet holding 4 000 of 40 000 entries plans as an index scan on the composite, with no sort node. |
| 1.5 | integration | `test_event_keyset_is_index_seek` | a workspace keyset page plans as an index seek with no post-filter on the tiebreak column. |
| 1.6 | integration | `test_reclaim_and_fleet_list_use_indexes` | the reclaim lookup and the fleet-list page each plan against their new index. |
| 2.1 | integration | `test_redundant_indexes_record_no_scans` | after the seeded workload, each removal candidate records zero scans. |
| 2.2 | integration | `test_api_key_auth_lookup_survives_drop` | with the partial index dropped, the authentication lookup plans against the unique index and returns the same row. |
| 3.1 | integration | `test_runner_list_liveness_bounded_by_page` | with runners numbering ten times the page size, the liveness subquery is evaluated at most page-size times. |
| 3.2 | integration | `test_secret_list_query_count_is_constant` | listing one credential and listing twenty issue the same number of queries. |
| 3.3 | integration | `test_api_key_list_is_index_ordered` | each supported sort plans without a sort node. |
| 3.4 | integration | `test_list_payloads_unchanged` | all three endpoints return byte-identical bodies to the pre-change implementation for one fixture. |
| 4.1 | unit | `test_actor_filter_trailing_backslash_is_literal` | a filter ending in a backslash yields a pattern PostgreSQL accepts and an empty page, never an error. |
| 4.2 | unit | `test_actor_filter_interior_backslash_is_literal` | a filter containing a backslash matches an actor containing that literal backslash. |
| 5.1 | unit | `test_adoption_checker_gates_on_threshold` | a tree below the threshold exits non-zero; at or above exits zero; both print the ratio. |
| 5.2 | unit | `test_adoption_checker_honours_exclusions` | fixtures, migration bootstrap, and the named metering modules are absent from the denominator. |
| 5.3 | unit | `test_statement_modules_are_constant_surfaces` | a module containing a function or an allocation fails the checker. |
| 5.4 | integration | `test_adoption_meets_threshold` | the checker run against the repository reports at or above the threshold. |
| regression | integration | `test_query_surface_behaviour_unchanged` | the full integration suite passes with no test file modified by §5's extraction commit. |
| regression | integration | `test_write_throughput_within_bound` | seeded insert cost on the indexed tables stays within the recorded bound after the slot applies. |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Every new index is chosen by the planner for its query (§1) | `make test-integration-db` | all `test_*_uses_index` / `*_index_*` tests pass | P0 | |
| R2 | The migration slot is additive and idempotent (§1) | `make test-integration-db` | `test_migration_slot_is_idempotent` passes | P0 | |
| R3 | Per-row list work is bounded by page size (§3) | `make test-integration` | §3 tests pass, payload comparison included | P0 | |
| R4 | A backslash in an actor filter never returns a server error (§4) | `make test-unit-agentsfleetd` | both §4 tests pass | P0 | |
| R5 | Statement-module adoption is at or above threshold (§5) | `python3 scripts/check_sql_statement_modules.py` | exit 0, printed ratio at or above the threshold | P0 | |
| R6 | Extraction changed no behaviour (§5) | `git diff --name-only origin/main -- 'src/**/*_test.zig' 'src/**/*_integration_test.zig'` | no test file modified by the extraction commit | P0 | |
| R7 | Write throughput did not regress (§1) | `make test-integration-db` | `test_write_throughput_within_bound` passes | P1 | |
| R8 | Index removals carry evidence and approval (§2) | recorded scan counts plus the approval quote in Discovery | zero scans recorded and quote present, or §2 dropped | P1 | |
| R9 | Diff stays inside Files Changed | `git diff --name-only origin/main` | 0 paths missing from the Files Changed table | P0 | |
| S1 | Repository conformance passes | `make harness-verify` | exit 0 | P0 | |
| S2 | Repository unit suites pass | `make test-unit-all` | exit 0 | P0 | |
| S3 | Result drainage preserved through extraction | `make check-pg-drain` | exit 0 | P0 | |
| S4 | No leaks | `make memleak` | exit 0 | P0 | |
| S5 | Both Linux targets build | `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` | both exit 0 | P0 | |
| S6 | No secrets | `gitleaks detect --no-banner` | exit 0 | P0 | |
| S7 | No oversize source file | `git diff --name-only origin/main \| grep -v '\.md$' \| xargs wc -l 2>/dev/null \| awk '$1>350 && $2!="total"'` | no output | P0 | |

**Grading protocol (VERIFY):** run the Verify command verbatim; grade ONLY from its output. Graded = ✅/❌ + the one decisive output line; long evidence goes to PR Session Notes with a pointer here. **Ship gate:** every row graded, every P0 ✅ → eligible for CHORE(close); any ❌ or empty cell → return to EXECUTE; a P1 ❌ ships only with an Indy-acked deferral quote in Discovery.

## Dead Code Sweep

**1. Orphaned files — deleted from disk and git.**

N/A — no files deleted.

**2. Orphaned references — zero remaining imports/uses.**

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| inline statement text in extracted modules | `grep -rnE '\\\\ *(SELECT \|INSERT INTO \|DELETE FROM \|UPDATE )' src/agentsfleetd/state src/agentsfleetd/fleet src/agentsfleetd/memory \| grep -v _test \| grep -v /sql.zig` | 0 matches in extracted domains |
| dropped index names (§2 only) | `grep -rn "idx_api_keys_key_hash_active\|idx_memory_entries_category" src/ schema/` | 0 matches outside the removal slot |

## Out of Scope

- **Candidate discovery on the lease path.** `assign.listCandidates` and everything downstream of it belong to M141_001_P0_API_DOCS_OBS_BOUNDED_RUNNER_LEASE_FANOUT. No file in that spec's blast radius is touched here.
- **The telemetry identifier type mismatch.** `core.fleet_execution_telemetry` stores `fleet_id` and `workspace_id` as `TEXT` while every sibling table uses `UUID`, forcing casts at every join and preventing a foreign key. Correcting it is a data migration on the billing spine and needs its own milestone.
- **Session-scoped tenant context.** `common_authz` writes `app.current_tenant_id` at session scope on a pooled connection and never resets it. Inert without row-level security, but it should not stay that way once row-level security lands. Separate spec.
- **Statement caching and pagination convergence.** The driver never names its prepared statements, so every execution re-plans; enabling caching trades planning cost against losing partial-index matching on generic plans, which needs measurement rather than a default. Separately, three endpoints paginate by offset while fleets and events use keyset — unifying them changes public response shapes.

---

## Product Clarity (authoring record)

1. **Successful user moment** — an operator whose account has accumulated a year of fleets and events opens the dashboard and it responds as it did on day one; the database graph is flat while nobody is running anything.
2. **Preserved user behaviour** — every endpoint keeps its request parameters, response body, status codes, and ordering. No stored data changes shape. An unmodified client cannot tell this shipped, except that it is faster.
3. **Optimal-way check** — the direct path is to let the database answer the questions it is already being asked, using indexes it does not yet have. The unconstrained-optimal shape would also converge the pagination models and correct the telemetry identifier types; both change public surfaces or require a data migration, so they stay out and are named in Out of Scope.
4. **Rebuild-vs-iterate** — iterate. Every query here returns the right answer; it returns it by reading more rows than it needs to. Rewriting the query surface would risk behaviour that is currently correct, which is why §5 is pure movement and §3 preserves payloads byte-for-byte.
5. **What we build** — one additive index slot, three restructured list reads, one filter escape fix, per-domain statement modules, and a checker that keeps adoption from regressing.
6. **What we do NOT build** — no schema shape change, no wire change, no pagination change, no statement-cache change, and nothing on the lease candidate path.
7. **Fit with existing features** — compounds the write-time counter table from slot 030, which removed read-time aggregation; this removes the remaining read-time scans and sorts. The thing it must not destabilize is the metering path, which is why its statements are excluded from extraction.
8. **Surface order** — N/A — no user surface; this is data-access internals behind unchanged endpoints.
9. **Dashboard restraint** — N/A — no dashboard surface is added. Poll-cost telemetry belongs to M141_001.
10. **Confused-user next step** — N/A — no user-facing behaviour changes, so there is no new state for a user to be confused by. An operator investigating database load reads the corrected `docs/architecture/scaling.md` section naming which reads are index-served.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** one workstream, because the index work and the extraction touch the same modules — extracting a statement and then indexing the query it issues in two separate passes would edit most of the data-access layer twice, and the second pass would conflict with the first. §2 is severable inside the workstream so a pending removal decision never blocks the additive work.
- **Alternatives considered:** splitting performance and extraction into two workstreams was rejected because of that double-edit, and because the audit that produced both findings read the same sixty-eight files once. Adding indexes without the planner assertions was rejected because a created-but-unused index looks identical to a fix in a passing test suite. Extracting all sixty-eight modules to reach full adoption was rejected as churn without signal — single-statement handlers gain nothing from a sibling module.
- **Patch-vs-refactor verdict:** this is a **patch** on the query surface — no query changes what it returns — combined with a **mechanical refactor** of where statement text lives. Neither changes a design decision; both make existing decisions legible and cheap.

## Discovery (consult log)

- **Consults** — Architecture: `docs/architecture/scaling.md` §Per-request volume describes recurring read cost and is updated here to state which reads are index-served. M141_001 edits the same section for the idle-cost model; the two are textually adjacent but semantically disjoint, so whichever merges second rebases that section.
- **Owner decision at CHORE(open) — §2 sequencing.** Put to Indy: whether §2's index removals are approved, and against which evidence. Indy chose *"Gather evidence, then decide"* — §1 lands first, the seeded workload runs, and the recorded `pg_stat_user_indexes` scan counts come back to Indy before any `DROP INDEX` is authored. §2 is therefore **not deferred and still in scope**; it is blocked on evidence that does not yet exist. R8 stays ungraded until that decision is taken.
- **Rules conflict at CHORE(open) — which removal model governs §2.** `dispatch/write_sql.md` keys the Schema Table Removal Guard on `VERSION`: below `2.0.0` it prescribes the teardown-rebuild path (delete the slot file, drop the `@embedFile` constant, drop the migration-array entry). `VERSION` is `0.21.0`, so that branch fires. `docs/SCHEMA_CONVENTIONS.md` §Migration Model records a later owner decision (Jul 22, 2026) making additive migrations the current model, with slots `001`–`031` frozen as bootstrap history. **Additive wins** — it is the newer owner decision, and `write_sql.md` names `SCHEMA_CONVENTIONS.md` as its own source of truth. §2's drops, if approved, land as a new numbered slot; no shipped slot file is edited.
- **Source finding** — `liveness_sweeper.expireActiveLeaseSlots` filters `fleet.runner_affinity` on `last_runner_id`, which carries no index and is a foreign key with `ON DELETE SET NULL`; the statement runs once per due runner per sweep cycle, so its cost is proportional to fleets times runners times cycles.
- **Source finding** — `liveness_sweeper.fetchDueRunners` orders `fleet.runners` by `updated_at`, which carries no index; the table has none at all beyond its identity and token-hash uniqueness, so both the sweep and the operator runner list scan and sort it. `reclaim.reclaimPriorActive` likewise filters `fleet.runner_leases` by `fleet_id`, unindexed despite being a foreign key with `ON DELETE CASCADE`, so every reclaim and every fleet delete scans that table.
- **Source finding** — `memory.memory_entries` carries a single-column index on `fleet_id`, but the hydration read, the eviction pass, and the daily sweep all order by `updated_at`, so each sorts the fleet's full memory set. Separately, `idx_api_keys_key_hash_active` is redundant behind `api_keys_hash_uniq` — the authentication lookup filters on `key_hash` alone and never on `active`.
- **Source finding** — `globToLike` escapes `%` and `_` but passes `\` through, so an actor filter ending in a backslash produces a pattern PostgreSQL rejects with `22025`, surfacing as a 500 on user input.
- **Source finding** — 81 statements live in 11 `sql.zig` modules; 68 production modules still carry statement text inline, which is why auditing the query surface requires reading 68 files rather than 11.
- **Adjacent finding (not this spec's scope)** — `M139_001` names two different specs on `main`: `docs/v2/done/M139_001_P1_API_UI_FLEET_EVENT_LEGIBILITY.md` and `docs/v2/pending/M139_001_P1_API_INFRA_OWNER_SAFE_DEADLINE_SCHEDULER.md`. If the deadline-scheduler branch has not renumbered its workstream, the collision resolves on merge or needs a rename; raised here so it is not lost.
- **EXECUTE correction — the slot registers in one file, not three.** Files Changed named `cmd/common.zig` and `db/migration_versions.zig` as edits. Neither is needed. `schema/embed.zig` says so itself — *"adding a migration is ONE edit (one line here), not two files"* — because `common.canonicalMigrations()` derives the array from it at comptime, and `migration_versions.zig` holds a capacity constant (`MAX_TRACKED_MIGRATIONS = 64`), not an index-keyed assertion list. Slot 33 is well inside it. Both rows dropped from the table.
- **EXECUTE correction — ten indexes, not nine.** The runner-list sort allowlist is four options over two columns (`created_at`, `host_id`, each direction), and one btree cannot serve both columns. Every index still traces to a named query; the count was the estimate, not the requirement.
- **EXECUTE correction — the migration cannot build concurrently.** See the amended Failure Modes row. Load-bearing for anyone applying this to a populated database.
- **§1 evidence — the sweeper read, measured.** On a 20 000-row `fleet.runners` fixture, `fetchDueRunners` before the slot plans as `Seq Scan` + `Sort` over the whole table; after, as `Index Scan using idx_runners_updated_at_id` with **no sort node**, returning its 200-row batch in **6 shared buffer hits**, with the lease subplan `never executed`. That is the P1 justification, and it is why the tests assert on the plan rather than on the index existing.
- **§2 evidence (partial) — `idx_memory_entries_fleet_id` is superseded.** On a 4 000-of-40 000 fixture, dropping the narrow index moves the hydration read onto `idx_memory_entries_fleet_id_updated_at_id` with no other plan change: the composite serves the same equality filter, as its prefix. That is one of the three removals evidenced. `idx_api_keys_key_hash_active` and `idx_memory_entries_category` still need their scan counts before the decision Indy reserved.
- **Open question raised at EXECUTE — does index 4 earn its place before §2 lands?** With the narrow index still present, the unbounded `listAll` keeps choosing it, so the composite's read benefit today is limited to bounded reads while its write cost is paid on every memory write. It becomes unambiguously load-bearing the moment §2 drops the narrow index. Options: ship both together, or hold index 4 with §2. Indy's call — flagged, not decided.
- **Environment note — the integration database is shared and was being reset mid-run.** `make/test-integration.mk`'s `_reset-test-db` hardcodes `-d agentsfleetdb`, so a suite run from any checkout drops every schema out from under a concurrent run in another worktree. This workstream's plan assertions therefore run against a dedicated `agentsfleetdb_m142`. Worth a follow-up so worktrees do not collide by default.
- **Metrics review** — no product-analytics event, operator metric, or funnel changes; no analytics playbook update required.
- **Skill-chain outcomes** —
- **Deferrals** —
