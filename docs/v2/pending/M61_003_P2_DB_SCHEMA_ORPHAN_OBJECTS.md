# M61_003: Schema orphan-object removal — prompt_lifecycle_events, ops_ro audit, dead column

**Prototype:** v2.0.0
**Milestone:** M61
**Workstream:** 003
**Date:** May 04, 2026
**Status:** PENDING
**Priority:** P2 — pre-v2.0.0 schema hygiene. Two unused tables + an audit-logging surface + a dead boolean column. None hold data anyone reads, but they all show up in every `\dt`/`\d+`/`pg_dump` and lie about what the schema enforces. Pre-v2.0, the Schema Removal Guard says the right cleanup is to edit/remove the migration file directly (no down-migration ceremony).
**Categories:** API
**Batch:** B1
**Branch:** feat/m61-schema-orphan-objects
**Depends on:** none — orthogonal to M61_001/002/004; can land in parallel.

**Canonical architecture:** `docs/architecture/` — N/A; no flow change. The schema gets smaller; consumers are unaffected because no production code reads or writes these objects today.

---

## Implementing agent — read these first

1. `AGENTS.md` — Schema Table Removal Guard (read the body in `docs/gates/schema-removal.md`); RULE NLG; Verification Gate.
2. `docs/SCHEMA_CONVENTIONS.md` — file-per-bounded-context migrations, ≤100 lines/file, single-concern.
3. `schema/embed.zig` and `src/cmd/common.zig` migration array — read the order; the audit's deletions mostly trim individual statements inside existing files and do NOT remove a whole migration file (so neither `embed.zig` nor the migration array changes shape).
4. `schema/003_rls_tenant_isolation.sql`, `schema/005_agent_failure_analysis_and_context_injection.sql`, `schema/004_workspace_entitlements.sql` — these are the three migrations being trimmed.
5. `src/db/pool_test.zig` — the only file that references `ops_ro.workspace_overview` (in a permission-RLS test). The view goes; the test goes with it.

---

## Applicable Rules

- `AGENTS.md` — Schema Table Removal Guard, RULE NLG, Milestone-ID Gate, Verification Gate.
- `docs/SCHEMA_CONVENTIONS.md` — full file.
- `docs/greptile-learnings/RULES.md` — RULE ORP, RULE FLL.

---

## Overview

**Goal (testable):** After this workstream, the strings `prompt_lifecycle_events`, `ops_ro_access_events`, `log_ops_ro_access`, `ops_ro.workspace_overview`, and `enable_score_context_injection` appear in zero `*.sql`, `*.zig`, `*.ts`, `*.tsx`, `*.js` files in the repo. `make test-integration` (Tier 3 — clean state, `make down && make up`) passes. Migration apply is fast and clean; no dead pg artifacts in the catalog.

**Problem:** The May 04, 2026 SQL audit cross-referenced every `CREATE TABLE`/`CREATE VIEW`/`CREATE FUNCTION`/`CREATE TRIGGER`/column in `schema/*.sql` against `rg` of the rest of the tree. Three families have zero live consumers:

1. **`core.prompt_lifecycle_events`** (schema/003_rls_tenant_isolation.sql:3-42) — append-only event log table + 2 indexes + 2 immutability triggers + the trigger function. Zero `INSERT`, `SELECT`, `UPDATE` callsites in `src/`. The closest thing live is `core.zombie_events` (migration 018), which is the actually-used event log. `prompt_lifecycle_events` is M14-era infrastructure that didn't ship.
2. **`audit.ops_ro_access_events` + `audit.log_ops_ro_access()` + `ops_ro.workspace_overview` view** (schema/005_agent_failure_analysis_and_context_injection.sql:4-46, line 48 view). The view exists for ops-readonly RLS tests in `src/db/pool_test.zig`; the access-event table and its logging function are referenced only by the view's `SELECT audit.log_ops_ro_access(...)` body, which is itself never executed because no production code SELECTs from the view. Test-only support surface for an ops-readonly path that isn't used in prod.
3. **`billing.workspace_entitlements.enable_score_context_injection`** (schema/004_workspace_entitlements.sql:12) — `BOOLEAN DEFAULT TRUE`. Zero refs in `src/`. M5-era scoring/context-injection plumbing that was pivoted away from. Column is harmless but lies about what the entitlement gates.

**Solution summary:** Edit migrations 003, 004, 005 in place to remove the dead DDL. Delete the matching `pool_test.zig` test for `workspace_overview` (and any sibling line referencing the audit pieces). Re-run `make down && make up && make test-integration` (Tier 3) to confirm the schema applies cleanly and no live test exercises the removed objects. Pre-v2.0 (`cat VERSION` reads `0.33.0`), Schema Removal Guard authorizes this without a down-migration: edit the up-migration source.

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `schema/003_rls_tenant_isolation.sql` | EDIT | Remove lines 3-42: `CREATE TABLE core.prompt_lifecycle_events`, its 2 indexes, the 2 append-only triggers, and the `reject_prompt_lifecycle_event_mutation()` function. RLS sections of the file remain. |
| `schema/004_workspace_entitlements.sql` | EDIT | Remove line 12 (`enable_score_context_injection BOOLEAN DEFAULT TRUE,`) from the `CREATE TABLE billing.workspace_entitlements` body. |
| `schema/005_agent_failure_analysis_and_context_injection.sql` | EDIT | Remove lines 4-12 (`CREATE TABLE audit.ops_ro_access_events`), lines 14-46 (`CREATE FUNCTION audit.log_ops_ro_access` + `CREATE OR REPLACE VIEW ops_ro.workspace_overview`). The remaining content of this file (agent_failure_analysis live tables) stays. If the file becomes empty after the cut, delete it AND remove its row from `schema/embed.zig` AND `src/cmd/common.zig` migration array. PLAN must read the file first to decide. |
| `src/db/pool_test.zig` | EDIT | Delete the `test "..."` block that SELECTs from `ops_ro.workspace_overview`. If that's the only block in the file, delete the file and drop its `_ = @import(...)` line from `src/main.zig`'s test bridge. |
| `schema/embed.zig` | EDIT (conditional) | Only if 005 becomes empty and gets deleted. |
| `src/cmd/common.zig` | EDIT (conditional) | Only if 005 becomes empty and gets deleted; remove the migration from the embedded array. |

No HTTP, CLI, UI, or interface changes. No live data is touched (the column has data — `TRUE` for every row — but no consumer reads it; dropping it is information-preserving relative to live behavior).

---

## Sections (implementation slices)

### §1 — Print Schema Removal Guard output

Per AGENTS.md `Schema Table Removal Guard`, before any edit: `cat VERSION` (expect `0.33.0`), then print the guard's pre-v2.0.0 teardown procedure block. Implementation default: pre-v2.0 = edit the up-migration source directly; no `DROP TABLE` shim in a new migration file.

### §2 — Trim migration 003 (prompt_lifecycle_events)

Edit `schema/003_rls_tenant_isolation.sql`. Remove the table, its 2 indexes (`idx_prompt_lifecycle_events_workspace_time`, `idx_prompt_lifecycle_events_tenant_time` per the audit), the 2 triggers (`trg_prompt_lifecycle_events_no_update`, `trg_prompt_lifecycle_events_no_delete`), and the trigger function (`reject_prompt_lifecycle_event_mutation()`). Keep all RLS infrastructure that the rest of the file establishes. Verify with `rg -nw 'prompt_lifecycle_events|reject_prompt_lifecycle_event_mutation' .` after edit — expect zero hits.

### §3 — Trim migration 004 (entitlement column)

Edit `schema/004_workspace_entitlements.sql`. Remove the `enable_score_context_injection` column line and any trailing comma/whitespace cleanup. The `idx_workspace_entitlements_tier` index (line 24+) is unaffected. Verify with `rg -nw 'enable_score_context_injection' .` — expect zero hits.

### §4 — Trim migration 005 (ops_ro audit infrastructure)

Edit `schema/005_agent_failure_analysis_and_context_injection.sql`. Remove `audit.ops_ro_access_events` table, the `audit.log_ops_ro_access()` function, and the `ops_ro.workspace_overview` view. Read the file first; if `agent_failure_analysis` and `context_injection` content remains, the file stays (just smaller). If the file would become empty/comment-only, delete it AND the matching rows in `schema/embed.zig` + `src/cmd/common.zig` migration array. Verify with `rg -nw 'ops_ro_access_events|log_ops_ro_access|workspace_overview' .` — expect zero hits.

### §5 — Trim `src/db/pool_test.zig`

Delete the `test "..."` block(s) that SELECT from `ops_ro.workspace_overview`. If the file's only purpose was that test, delete the file and drop its `_ = @import("db/pool_test.zig");` line from the test bridge in `src/main.zig`. Implementation default: read the file first; if it has unrelated tests, keep them.

### §6 — Tier-3 verify on clean state

Per AGENTS.md `Verification Gate` Tier 3 (mandatory when schema changes pre-v2.0): `make down && make up && make test-integration`. Schema must apply cleanly from empty. Record outcome in PR Session Notes. Then full `make lint`, `make test`, cross-compile (no Zig changes here outside §5, but cross-compile is cheap), `make memleak` not required (no allocator changes), `make bench` not required.

---

## Interfaces

N/A. The deleted objects have no live consumers. The kept-on `workspace_entitlements` table loses one column but its existing INSERT/SELECT/UPDATE callsites do not name the column, so they are unaffected.

---

## Failure Modes

| Mode | Cause | Handling |
|------|-------|----------|
| `make up` fails after edit | Hidden FK or trigger reference into a deleted object that the audit grep missed | Read the failure, restore via `git restore schema/<file>`, file the dependency in `Discovery`, decide trim-vs-keep. |
| `make test` count drops more than the expected `pool_test.zig` block | A wired test was passing with a side effect on a deleted object | Investigate before COMMIT; we should know exactly which block we removed. |
| Audit missed a string-formatted reference | Some Zig file builds the table name from string concatenation (very unusual; the audit grep would still hit substring "prompt_lifecycle_events" or "ops_ro_access_events") | The two `rg -nw` checks in §2-§4 should catch it. If something ships a name via formatString, fix in same commit. |
| `004_workspace_entitlements.sql` row-level data drifts | The column had `DEFAULT TRUE`; existing rows set the value but no consumer reads it | Verify `rg -nF 'enable_score_context_injection' .` returns zero. Pre-v2.0 source-edit semantics: deleting the column from the up-migration means a fresh `make up` (Tier 3 is fresh state) doesn't have the column; existing dev/prod databases need a `ALTER TABLE ... DROP COLUMN` only if anyone runs against them after this lands. Per Schema Removal Guard pre-v2.0.0: dev environments are torn down on demand; we don't owe a migration. Document in Session Notes. |
| Migration 005 becomes empty, embed.zig drift | Forgetting to drop the row in `schema/embed.zig` + `src/cmd/common.zig` migration array | §4 checklist requires a read-first decision; CHORE(close) does not pass if `make up` works on a fresh DB but the array references a deleted file. |

---

## Invariants

1. **Zero references in repo** to `prompt_lifecycle_events`, `ops_ro_access_events`, `log_ops_ro_access`, `ops_ro.workspace_overview`, `enable_score_context_injection` after this spec — enforced by `rg -nw` greps in §2/§3/§4 and again at HARNESS VERIFY.
2. **`make up` (clean state) succeeds** — Tier 3 verify; CHORE(close) blocks otherwise.
3. **`make test-integration` (clean state) passes** — Tier 3.
4. **`schema/embed.zig` and the `src/cmd/common.zig` migration array agree with `ls schema/*.sql`** — `make check-pg-drain` and the existing schema-coverage tests catch drift.
5. **No `*.sql` file under `schema/` exceeds 100 lines after edits** — `docs/SCHEMA_CONVENTIONS.md` cap; we're shrinking, not growing, so should hold trivially.

---

## Test Specification

| Test | Asserts | Where |
|------|---------|-------|
| Tier 3 integration | Schema applies clean from empty + suite passes | `make down && make up && make test-integration` |
| Existing entitlement tests | `workspace_entitlements` row writes/reads work without the dropped column | `src/state/workspace_entitlements_store_test.zig` (live) |
| Existing zombie events tests | The live event log (`core.zombie_events`) — not the deleted `prompt_lifecycle_events` — still works | `src/state/zombie_events_store_test.zig` (live) |
| Existing RLS tests | Tenant-isolation guarantees still pass on the trimmed migration 003 | `src/db/rls_test.zig` |

No new tests. The deleted `pool_test.zig` block is the only test loss; it was testing dead infrastructure.

---

## Eval Commands

```bash
# E1 — repo-wide name sweep, must be empty after the spec lands
for s in prompt_lifecycle_events ops_ro_access_events log_ops_ro_access \
         ops_ro.workspace_overview reject_prompt_lifecycle_event_mutation \
         enable_score_context_injection; do
  echo "=== $s ==="
  rg -nF "$s" . -g '!.git' -g '!node_modules' -g '!.zig-cache' -g '!dist' -g '!coverage'
done
# Expected: zero output under each header.

# E2 — schema apply on clean state
make down && make up

# E3 — Tier 3 verify
make test-integration

# E4 — schema-file size cap
wc -l schema/*.sql | awk '$1 > 100 && $2 != "total"'
# Expected: zero rows.
```

---

## Discovery (filled during EXECUTE)

Migration 005 status (file empty after trim?): ____
`pool_test.zig` other test blocks (kept/deleted): ____
Any `enable_score_context_injection` reference outside `schema/`: ____

---

## Out of Scope

- Other schema files (006-020): the audit found no orphans there. Don't tour them.
- Index optimization (none flagged as redundant by the audit).
- Down-migrations / `DROP COLUMN` shims for production databases — Schema Removal Guard pre-v2.0.0 says no.
- Re-introducing `prompt_lifecycle_events` later — that's a fresh milestone with table + producer + consumer + tests landed together.
- Renaming kept columns (RULE NLR is touch-it-fix-it for files we edit; the kept lines aren't legacy framing).
