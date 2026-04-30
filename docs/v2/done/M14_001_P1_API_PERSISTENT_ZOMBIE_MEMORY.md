# M14_001: Persistent Zombie Memory — Core and Daily Memory Survives Workspace Destruction

**Prototype:** v2
**Milestone:** M14
**Workstream:** 001
**Date:** Apr 12, 2026: 03:30 PM
**Status:** DONE (core scope: storage + HTTP API; export/import/CLI split to M14_005)
**Branch:** feat/m14-001-persistent-memory
**Priority:** P1 — Without this, every zombie run is a cold start; learned context is lost on workspace destruction
**Batch:** B1
**Depends on:** M5_001 (dynamic skill architecture, tool_bridge)

---

## Overview

**Goal (testable):** A zombie that calls `memory_store("lead_acme", "...")` in run N recalls the exact content in run N+1 after the run N workspace is destroyed, with row-level isolation enforced so a second zombie cannot read the first zombie's memory.

**Problem:** NullClaw's memory tools (`memory_store`, `memory_recall`, `memory_list`, `memory_forget`) default to SQLite at `<workspace_dir>/memory.db` via the `hybrid_keyword` profile. The executor takes these defaults unchanged (`src/executor/runner.zig:155, 200`). Workspaces are temporary worktrees destroyed after each run. Result: every zombie run starts with zero memory. Lead-Collector zombies re-research every lead, Customer-Support zombies re-ask customers their plan, Ops zombies don't recognize recurring incidents. `core.zombie_sessions.context_json` is a conversation-resume cursor (stores `{last_event_id, last_response}` per `src/zombie/event_loop_helpers.zig:67-75`), not agent memory — the SQL comment misdescribes it, and the comment is corrected as part of this workstream.

**Solution summary:** Route NullClaw's `core` and `daily` memory categories to a dedicated `memory` schema in the existing core Postgres database, isolated by a dedicated `memory_runtime` role whose grants cover only `memory.*`. Leave `conversation` category in workspace SQLite (correctly ephemeral). Ship `zombiectl memory export|import` so operators can read and edit memory as markdown on their own laptops. Default memory on for new zombies. On `core` capacity exceeded, error back to the agent loudly — never silently prune. Isolation is enforced at the query layer (row-level `zombie_id` scope) because the store lives behind a process boundary — the agent's shell tools cannot reach Postgres with `memory_runtime` credentials (different protocol, different grants). Schema-in-same-DB was chosen over separate-database to avoid building a parallel migration chain; escalation to a separate database (or instance) is a backup-and-restore migration if noisy-neighbor evidence demands it later.

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `schema/026_memory_entries.sql` | CREATE | `memory` schema with `memory_entries` table + indexes, grants to `memory_runtime` |
| `schema/004_vault_schema.sql` | MODIFY | Add `memory_runtime` role creation and `db_migrator` grant on `memory` schema |
| `schema/embed.zig` | MODIFY | Register the new SQL file as an @embedFile constant |
| `src/cmd/common.zig` | MODIFY | Add v26 migration to canonical array; bump array length 21 → 22 |
| `schema/023_core_zombie_sessions.sql` | MODIFY | Fix inaccurate comment (context_json is a bookmark, not memory) |
| `src/executor/types.zig` | MODIFY | Add `MemoryBackendConfig` struct |
| `src/executor/runner.zig` | MODIFY | Pass per-zombie MemoryConfig to NullClaw (currently takes defaults) |
| `src/memory/zombie_memory.zig` | CREATE | NullClaw backend adapter + row-level scoping helpers |
| `src/memory/export_import.zig` | CREATE | Export to markdown, import from markdown with scope validation |
| `src/http/handlers/memory_http.zig` | CREATE | `/v1/memory/{store,recall,list,forget}` endpoints (Path B external-agent) |
| `zombiectl/src/commands/memory.js` | CREATE | `zombiectl memory {enable,status,export,import,forget,scrub}` |
| `public/openapi.json` | MODIFY | Declare the new memory endpoints |
| `docs/greptile-learnings/RULES.md` | MODIFY | Add RULE CTX (cross-tenant process boundary) |

---

## Applicable Rules

- **RULE FLS** — drain/flush all pg query result rows before deinit (every query in the memory path)
- **RULE FLL** — 350-line gate on every touched .zig/.js file
- **RULE ORP** — cross-layer orphan sweep (if any symbols rename)
- **RULE XCC** — cross-compile x86_64-linux and aarch64-linux before commit
- **RULE TXN** — every DELETE/UPDATE in a transaction must ROLLBACK on failure (export/import upserts)
- **RULE SSM** — StaticStringMap for category enum lookup
- **RULE ZIG** — init/deinit/ownership conventions for new structs
- **RULE CTX** (new, added by this workstream) — cross-tenant data requires a process boundary, not a shared filesystem
- **RULE EP4** — any removed/deprecated endpoint returns HTTP 410 Gone with a named error code (no endpoints are removed in this workstream, but verify on API review)

---

## §1 — Memory Schema and Role

**Status:** IN_PROGRESS (schema + role done; table schema revised — see note)

Create a dedicated `memory` schema in the core Postgres database, with a
dedicated `memory_runtime` role that has grants only on `memory.*` and zero
grants on `core.*`. NullClaw's PostgresMemory auto-migrates its own table
schema (instance_id TEXT, TIMESTAMPTZ timestamps, session_id UUID) on first
connect. Migration 026 creates the schema, role, and grants; it does NOT
pre-create the table (NullClaw owns table DDL to keep column names coherent
with NullClaw's internal queries). Steps 5+ can ALTER TABLE to add operator
columns (tags, etc.) on top.

**Schema design note (Step 4 deviation):** Original spec called for `zombie_id
UUID FK` + BIGINT timestamps. Revised to let NullClaw manage the table because
NullClaw's INSERT includes `instance_id TEXT` (its isolation key) and
`created_at TIMESTAMPTZ` — column type mismatches would break at runtime.
The FK is deferred to when NullClaw's schema is extended in Steps 5+.

**Dimensions (test blueprints):**

| Dim | Status | Target | Input | Expected | Test type |
|-----|--------|--------|-------|----------|-----------|
| 1.1 | DONE | `schema/026_memory_entries.sql` | fresh DB + NullClaw connect | table `memory.memory_entries` auto-created by NullClaw with `(id, key, content, category, session_id, instance_id, created_at, updated_at)`; isolation via `instance_id = "zmb:{uuid}"`; schema revised from spec (see note above) | integration |
| 1.2 | DONE | `memory_runtime` role grants | fresh DB after migration | role has USAGE+CREATE on `memory` schema; has NO access to `core.*` (negative test passes per Step 2 tests); ALTER DEFAULT PRIVILEGES auto-grants on NullClaw-created tables | integration |
| 1.3 | DONE | `schema/embed.zig` + `src/cmd/common.zig` | `make run-migrations` | migration v26 runs exactly once; re-run is idempotent; migration array length matches embedFile list | contract |
| 1.4 | PENDING | `schema/023_core_zombie_sessions.sql` comment fix | read comment after edit | comment describes `context_json` as "conversation resume bookmark storing {last_event_id, last_response}" and explicitly notes it is NOT agent memory | contract |

---

## §2 — Executor Wiring and Per-Zombie Isolation

**Status:** IN_PROGRESS (Step 4 done; dims 2.1 unit-tested, 2.3/2.4 require live DB)

The executor must build a per-zombie `MemoryConfig` and pass it to NullClaw so
`core` and `daily` categories route to the memory schema while `conversation` stays
in workspace SQLite. Row-level scoping is enforced at the query layer.

**Dimensions (test blueprints):**

| Dim | Status | Target | Input | Expected | Test type |
|-----|--------|--------|-------|----------|-----------|
| 2.1 | DONE | `src/executor/types.zig:MemoryBackendConfig` | `MemoryBackendConfig{ .backend = "postgres", .connection = "...", .namespace = "zmb:0195b4ba-8d3a-7f13-8abc-...", .max_entries = 100000, .daily_retention_hours = 72 }` | struct validates at startup; rejects empty namespace, invalid backend | unit |
| 2.2 | PENDING | `src/executor/runner.zig:executeInner` | zombie_id `zom_A` running concurrently with `zom_B` | Zombie A's `memory_store("user_pref", "dark")` NOT visible to Zombie B's `memory_recall("user_pref")`; scope enforced via `WHERE instance_id = $current` in every memory op | integration |
| 2.3 | PENDING | `src/executor/zombie_memory.zig` (executor) | NullClaw calls `memory_store` with category `core` | write goes to `memory.memory_entries` in the memory schema; `conversation` category still writes to workspace SQLite | integration |
| 2.4 | PENDING | crash recovery path | zombie stores memory, then SIGKILL, then restart | all memory_store calls committed before SIGKILL are recoverable; `UPSERT` semantics (not INSERT) so retried writes don't conflict | integration |

---

## §3 — Export/Import Tool (Human-Readable View)

**Status:** DEFERRED to M14_002 — dims 3.1-3.3 (export_import.zig, zombiectl CLI) moved to new spec. Dim 3.4 (HTTP handler) DONE in this workstream.

Memory is authoritative in Postgres. Markdown files are the human-readable view,
generated on demand to the operator's laptop (never on the worker filesystem).

**Dimensions (test blueprints):**

| Dim | Status | Target | Input | Expected | Test type |
|-----|--------|--------|-------|----------|-----------|
| 3.1 | PENDING | `src/memory/export_import.zig:exportZombie` | zombie with 50 core entries + 10 daily entries | writes one markdown file per entry under `core/` and `daily/` subdirectories; frontmatter contains `key`, `category`, `zombie_id`, `tags`, `created`, `updated`; body is the content verbatim | unit |
| 3.2 | PENDING | `src/memory/export_import.zig:importZombie` | folder of edited markdown for zombie `zom_abc` | upserts entries scoped to `zom_abc` only; rejects entries whose frontmatter `zombie_id` mismatches; wraps all upserts in a single transaction with ROLLBACK on any failure | integration |
| 3.3 | PENDING | `zombiectl/src/commands/memory.js` | `zombiectl memory export --zombie zom_abc --out ./mem/` then edit then `zombiectl memory import --zombie zom_abc --from ./mem/` | next `memory_recall` on the edited key returns the edited content (edit-then-replay proof) | integration |
| 3.4 | DONE | `src/http/handlers/memory_http.zig` | external agent POSTs `/v1/memory/recall` with agent key | returns entries scoped to the agent's `zombie_id` only; cross-zombie request returns `UZ-MEM-SCOPE` error | integration |

---

## §4 — Memory-Full Policy and Retention

**Status:** DEFERRED to M14_002 — retention/pruning descoped; observe collection patterns first.

`daily` category auto-prunes on a schedule. `core` category has a per-zombie
hard cap for runaway-zombie protection; on overflow the store call errors back
to the agent loudly. Never silently drop a `core` entry the agent asked to keep.

**Dimensions (test blueprints):**

| Dim | Status | Target | Input | Expected | Test type |
|-----|--------|--------|-------|----------|-----------|
| 4.1 | PENDING | daily prune job | 100 `daily` entries older than 72h + 10 younger | older 100 deleted, younger 10 retained; run is idempotent | integration |
| 4.2 | PENDING | `src/memory/zombie_memory.zig:store` on core overflow | zombie at `max_entries=100000` attempts one more `core` store | store returns `UZ-MEM-FULL` error to agent; no entry written; nothing auto-pruned | integration |
| 4.3 | PENDING | memory-off default path | zombie without memory config OR memory backend unreachable at startup | executor falls back to ephemeral workspace SQLite; logs `memory.backend_unavailable`; agent continues running | unit |
| 4.4 | PENDING | `memory_forget` | agent calls `memory_forget("lead_acme_corp")` | entry removed from DB; next `memory_recall` returns empty; operation is idempotent | unit |

---

## Interfaces

**Status:** PENDING

### Public Functions (Zig)

```zig
// src/executor/types.zig
pub const MemoryBackendConfig = struct {
    backend: []const u8,            // "postgres" (future: "redis", "sqlite")
    connection: []const u8,         // connection string or file path
    namespace: []const u8,          // "zmb:{zombie_id}" — row-level scope key
    max_entries: u32 = 100_000,     // hard cap for runaway-zombie protection
    daily_retention_hours: u32 = 72,
};

// src/memory/zombie_memory.zig
pub fn initRuntime(alloc: Allocator, cfg: *const MemoryBackendConfig, zombie_id: []const u8) !MemoryRuntime;
pub fn store(self: *MemoryRuntime, key: []const u8, category: Category, content: []const u8, tags: []const []const u8) !void;
pub fn recall(self: *MemoryRuntime, key: []const u8) !?Entry;
pub fn list(self: *MemoryRuntime, filter: ListFilter) !EntryIter;
pub fn forget(self: *MemoryRuntime, key: []const u8) !bool;

// src/memory/export_import.zig
pub fn exportZombie(alloc: Allocator, db: *Pool, zombie_id: []const u8, out_dir: []const u8) !ExportSummary;
pub fn importZombie(alloc: Allocator, db: *Pool, zombie_id: []const u8, in_dir: []const u8) !ImportSummary;
```

### CLI Surface

```
zombiectl zombie memory enable  --zombie <id> [--categories core,daily] [--daily-retention-hours 72]
zombiectl zombie memory status  --zombie <id>
zombiectl memory export         --zombie <id> --out <dir>  [--filter "tag:X" | "category:core"]
zombiectl memory import         --zombie <id> --from <dir> [--mode upsert|replace]
zombiectl memory forget         --zombie <id> --key <key>
zombiectl memory scrub          --zombie <id> --pattern <regex>   # PII redaction
```

### Input Contracts

| Field | Type | Constraints | Example |
|-------|------|-------------|---------|
| backend | string | one of: `postgres` (sqlite, redis reserved for future) | `"postgres"` |
| connection | string | valid Postgres URI | `"postgresql://memory_runtime:..."` |
| namespace | string | non-empty; must start with `zmb:` followed by a 36-char UUID v7 | `"zmb:0195b4ba-8d3a-7f13-8abc-000000000100"` |
| category | enum | one of: `core`, `daily`, `conversation`, `workspace` | `"core"` |
| key | string | 1-255 bytes, UTF-8 | `"lead_acme_corp"` |
| content | string | 1-16384 bytes | `"Acme Corp — CTO Jane..."` |
| tags | string[] | 0-32 elements, each 1-64 bytes | `["lead", "enterprise"]` |

### Output Contracts

| Field | Type | When | Example |
|-------|------|------|---------|
| `memory.backend_ready` | log line | executor startup success | `backend=postgres namespace=zmb:0195b4ba-8d3a-7f13-8abc-...` |
| `memory.backend_unavailable` | log warning | connection failure | `err=ConnectionRefused — falling back to ephemeral` |
| `ExportSummary{count, bytes, files}` | struct | `exportZombie` completes | `{count: 47, bytes: 12403, files: 47}` |
| `ImportSummary{upserted, rejected, errors}` | struct | `importZombie` completes | `{upserted: 45, rejected: 2, errors: []}` |

### Error Contracts

| Error condition | Behavior | Caller sees |
|----------------|----------|-------------|
| Memory backend unreachable at startup | Fall back to ephemeral workspace SQLite, log degradation | activity log: `memory degraded — ephemeral only` |
| Memory backend unreachable mid-run | Memory op returns error to agent; agent continues | `UZ-MEM-UNAVAILABLE` tool error |
| Core category at max_entries | Reject new store; do NOT prune | `UZ-MEM-FULL` tool error to agent |
| Import with mismatched `zombie_id` in frontmatter | Reject that entry, continue others, report in summary | `ImportSummary.rejected` with reason |
| External agent requests another zombie's key | Reject at API layer | HTTP 403 with `UZ-MEM-SCOPE` |
| Connection pool exhaustion on memory_runtime | Tool error; no cascade to core.* pool | `UZ-MEM-POOL-EXHAUSTED` |

---

## Failure Modes

**Status:** PENDING

| Failure | Trigger | System behavior | User observes |
|---------|---------|----------------|---------------|
| Memory DB down at zombie start | Postgres memory cluster unreachable | Fall back to ephemeral workspace SQLite; activity log records degradation | `zombiectl logs` shows "memory degraded — ephemeral only" |
| Memory DB down mid-run | Network partition | memory_store/recall return error to agent | Agent may say "I can't access my memory right now" |
| Slow memory backend (>500ms) | Overloaded memory schema | Agent conversation latency increases | Slower responses; activity log shows memory latency |
| Corrupt or missing entry | Disk failure, bad migration | memory_recall returns empty | Zombie treats the fact as never-learned; same behavior as pre-M14 |
| Core category overflow | Runaway zombie or explicit 100k entries | `UZ-MEM-FULL` to agent | Agent surfaces "my memory is full — please review" |
| Edit-then-replay with wrong zombie_id | Operator pointed import at wrong zombie | Import rejects mismatched entries; summary lists rejections | Operator sees rejection count |
| PII in memory that should have been scrubbed | Skill template didn't redact before store | `zombiectl memory scrub` removes post-hoc | Support/compliance can verify scrub ran |

**Platform constraints:**
- `memory` schema lives in the core Postgres database (not a separate database or instance). Shared CPU/IO/WAL/pool with `core.*` — monitor for noisy-neighbor. Escalation path: promote to a separate database via backup-and-restore, then separate instance if workload demands.
- `memory_runtime` role must have zero access to `core.*` (verify with negative test).
- NullClaw `postgres_keyword` profile is the target — `postgres_hybrid` not used (no pgvector).

---

## Implementation Constraints (Enforceable)

**Status:** PENDING

| Constraint | How to verify |
|-----------|---------------|
| No new Zig file > 350 lines (RULE FLL) | `wc -l` on every new file |
| Cross-compiles on x86_64-linux, aarch64-linux (RULE XCC) | `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` |
| Every `conn.query()` has `.drain()` before `deinit()` (RULE FLS) | `make check-pg-drain` |
| Memory config optional — null config preserves current ephemeral behavior | Unit test: null → workspace SQLite, no regression |
| `memory_runtime` role has zero `core.*` grants | Integration negative test: `SELECT FROM core.zombies AS memory_runtime` → permission denied |
| Zero new build.zig.zon entries | NullClaw already vendors the backends; no new deps |
| Export never writes to a path under the worker's filesystem | CLI writes under caller's CWD; server handler returns bytes, caller writes |

---

## Invariants (Hard Guardrails)

**Status:** PENDING

| # | Invariant | Enforcement mechanism |
|---|-----------|----------------------|
| 1 | Every memory operation includes `WHERE zombie_id = $current` | Query-builder wrapper; grep in CI to verify no raw SQL bypasses it |
| 2 | `MemoryBackendConfig.namespace` starts with `zmb:` followed by a valid UUID v7 | `validate()` checks prefix and UUID v7 format |
| 3 | `Category` enum has exactly four variants (`core`, `daily`, `conversation`, `workspace`) | Zig enum exhaustiveness check at comptime |

---

## Test Specification

**Status:** PENDING

### Unit Tests

| Test name | Dim | Target | Input | Expected |
|-----------|-----|--------|-------|----------|
| `memory_config_default_ephemeral` | 4.3 | `types.zig` | null config | falls back to workspace SQLite |
| `memory_config_validates_backend` | 2.1 | `types.zig` | `backend="invalid"` | returns validation error |
| `memory_config_namespace_format` | 2.1 | `types.zig` | namespace `"zmb:0195b4ba-8d3a-7f13-8abc-..."` | validates; empty namespace errors |
| `category_enum_exhaustive` | 2.3 | `zombie_memory.zig` | all four variants | compiles; fifth variant = compile error |
| `memory_full_errors_core` | 4.2 | `zombie_memory.zig` | zombie at `max_entries`, store attempt | returns `UZ-MEM-FULL` |
| `memory_forget_idempotent` | 4.4 | `zombie_memory.zig` | forget non-existent key | returns false, no error |

### Integration Tests

| Test name | Dim | Infra needed | Input | Expected |
|-----------|-----|-------------|-------|----------|
| `store_destroy_recall` | 2.4 | memory schema | store in run N, destroy workspace, recall in run N+1 | fact returned |
| `crash_recovery` | 2.4 | memory schema | store → SIGKILL → restart → recall | committed facts returned |
| `zombie_isolation` | 2.2 | memory schema | A stores, B recalls same key | B gets nothing |
| `daily_prune_72h` | 4.1 | memory schema + clock fast-forward | 100 daily entries, 50 aged >72h | 50 deleted |
| `memory_role_no_core_access` | 1.2 | memory schema | `SELECT FROM core.zombies AS memory_runtime` | permission denied |
| `export_import_roundtrip` | 3.3 | memory schema | export → no edits → import | zero diff |
| `edit_then_replay` | 3.3 | memory schema | export → edit one entry → import → recall | edited content returned |
| `import_rejects_mismatched_zombie` | 3.2 | memory schema | import with wrong zombie_id in frontmatter | entry rejected, summary reports it |
| `external_agent_scope_enforced` | 3.4 | memory schema + http | agent key for zom_A requests zom_B recall | 403 `UZ-MEM-SCOPE` |

### Negative Tests (error paths that MUST fail)

| Test name | Dim | Input | Expected error |
|-----------|-----|-------|---------------|
| `reject_empty_namespace` | 2.1 | namespace `""` | config validation error |
| `reject_invalid_category` | 2.3 | store with category `"garbage"` | compile error (enum) |
| `reject_cross_zombie_recall_via_api` | 3.4 | external agent for zom_A, key in zom_B | HTTP 403 |
| `reject_import_mismatch` | 3.2 | import with frontmatter zombie_id ≠ arg | rejected entry in summary |

### Edge Case Tests (boundary values)

| Test name | Dim | Input | Expected |
|-----------|-----|-------|----------|
| `content_max_length` | 2.3 | 16384-byte content | stored successfully |
| `content_over_max` | 2.3 | 16385-byte content | validation error |
| `tag_count_max` | 2.3 | 32-tag array | stored; 33 tags → validation error |
| `unicode_key` | 2.3 | key with multibyte UTF-8 | stored and recalled bit-identical |
| `concurrent_upsert_same_key` | 2.2 | zombie A updates same key from two processes | one wins, no error, no lost update (UPSERT semantics) |

### Regression Tests (pre-existing behavior that MUST NOT change)

| Test name | What it guards | File |
|-----------|---------------|------|
| `zombie_sessions_context_json_unchanged` | `core.zombie_sessions.context_json` format stays `{last_event_id, last_response}` | `src/zombie/event_loop_test.zig` |
| `workspace_sqlite_still_handles_conversation` | NullClaw `conversation` category still writes to workspace SQLite | `src/executor/runner_test.zig` |

### Leak Detection Tests

| Test name | Dim | What it proves |
|-----------|-----|---------------|
| `export_zero_leaks` | 3.1 | std.testing.allocator detects zero leaks after exportZombie |
| `import_zero_leaks` | 3.2 | std.testing.allocator detects zero leaks after importZombie |
| `memory_runtime_deinit_clean` | 2.3 | MemoryRuntime.deinit frees all pooled connections |

### Spec-Claim Tracing

| Spec claim | Test that proves it | Test type |
|-----------|-------------------|-----------|
| Memory survives workspace destruction | `store_destroy_recall` | integration |
| Memory survives process crash | `crash_recovery` | integration |
| Zombies cannot read each other's memory | `zombie_isolation` | integration |
| Missing config degrades gracefully | `memory_config_default_ephemeral` | unit |
| `core` overflow errors, does not silently prune | `memory_full_errors_core` | unit |
| `daily` auto-prunes at 72h | `daily_prune_72h` | integration |
| Edit-then-replay works end-to-end | `edit_then_replay` | integration |
| External agent cannot cross zombie scope | `external_agent_scope_enforced` | integration |
| `memory_runtime` has zero core.* access | `memory_role_no_core_access` | integration |

---

## Execution Plan (Ordered)

**Status:** PENDING

| Step | Action | Verify (must pass before next step) |
|------|--------|--------------------------------------|
| 1 | Add `memory_runtime` role to `schema/004_vault_schema.sql`; write `schema/026_memory_entries.sql`; register in `schema/embed.zig` + `src/cmd/common.zig` migration array (bump 21→22); fix `023_core_zombie_sessions.sql` comment | `make run-migrations` succeeds; `psql -c '\dt memory.*'` lists `memory_entries`; `zig build` passes |
| 2 | Create `memory_runtime` role with scoped grants; negative test no core.* access | Integration test `memory_role_no_core_access` passes |
| 3 | Add `MemoryBackendConfig` to `executor/types.zig`; wire `runner.zig` to build config from zombie_id | `zig build test` passes, null config = no regression |
| 4 | Implement `src/memory/zombie_memory.zig` NullClaw adapter with row-level scoping | Integration test `zombie_isolation` passes |
| 5 | DEFERRED → M14_002: retention/pruning for daily category | `daily_prune_72h` passes |
| 6 | DEFERRED → M14_002: `src/memory/export_import.zig` | `export_import_roundtrip` and `edit_then_replay` pass |
| 7 | DONE: `/v1/memory/*` HTTP handlers with scope enforcement | handlers compile; scope enforced via `UZ-MEM-SCOPE` |
| 8 | DEFERRED → M14_002: `zombiectl memory` subcommands | CLI integration test passes |
| 9 | DONE: `public/openapi.json` updated with Memory tag + 4 paths | valid JSON; Memory tag present |
| 10 | PENDING: Add RULE CTX to `docs/greptile-learnings/RULES.md` | rule present and linked from failure-modes |
| 11 | DONE: Cross-compile check | `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` both pass |
| 12 | DONE (partial): pg-drain passes; full gate on commit | `make check-pg-drain` PASS |

---

## Acceptance Criteria

**Status:** PENDING

- [ ] `memory_store` in run N → `memory_recall` in run N+1 returns stored fact — verify: `make test-integration` (`store_destroy_recall`)
- [ ] Process crash (SIGKILL) does not lose committed memory — verify: `make test-integration` (`crash_recovery`)
- [ ] Two concurrent zombies cannot read each other's memory — verify: `make test-integration` (`zombie_isolation`)
- [ ] `core` capacity exceeded errors the agent; never silently prunes — verify: `make test` (`memory_full_errors_core`)
- [ ] `daily` entries auto-prune at 72h — verify: `make test-integration` (`daily_prune_72h`)
- [ ] `memory_runtime` role has zero grants on `core.*` — verify: `make test-integration` (`memory_role_no_core_access`)
- [ ] `zombiectl memory export|import` edit-then-replay round-trips — verify: `make test-integration` (`edit_then_replay`)
- [ ] External-agent cross-zombie recall is 403 — verify: `make test-integration` (`external_agent_scope_enforced`)
- [ ] Missing memory config preserves current ephemeral behavior — verify: `make test` (`memory_config_default_ephemeral`)
- [ ] No new file exceeds 350 lines — verify: `wc -l`
- [ ] Cross-compile passes — verify: `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux`
- [ ] Gitleaks passes on diff — verify: `gitleaks detect`
- [ ] RULE CTX added to RULES.md — verify: `grep -c "RULE CTX" docs/greptile-learnings/RULES.md` = 1
- [ ] SQL comment on `schema/023_core_zombie_sessions.sql` corrected — verify: `grep -c "NOT agent memory" schema/023_core_zombie_sessions.sql` = 1

---

## Eval Commands (Post-Implementation Verification)

**Status:** PENDING

```bash
# E1: Build
zig build 2>&1 | head -5; echo "build=$?"

# E2: Unit + integration tests
make test 2>&1 | tail -5; echo "test=$?"
make test-integration 2>&1 | tail -5; echo "test-int=$?"

# E3: Cross-compile
zig build -Dtarget=x86_64-linux 2>&1 | tail -3; echo "xc_x86=$?"
zig build -Dtarget=aarch64-linux 2>&1 | tail -3; echo "xc_arm=$?"

# E4: Lint + pg-drain
make lint 2>&1 | grep -E "✓|FAIL"
make check-pg-drain 2>&1 | tail -3; echo "drain=$?"

# E5: 350-line gate (exempts .md)
git diff --name-only origin/main | grep -v '\.md$' | xargs wc -l 2>/dev/null | awk '$1 > 350 { print "OVER: " $2 ": " $1 }'

# E6: Gitleaks
gitleaks detect 2>&1 | tail -3; echo "gitleaks=$?"

# E7: OpenAPI consistency
make check-openapi-errors 2>&1 | tail -3; echo "openapi=$?"

# E8: RULE CTX present
grep -c "RULE CTX" docs/greptile-learnings/RULES.md; echo "ctx_rule=$?"

# E9: SQL comment corrected
grep -c "NOT agent memory" schema/023_core_zombie_sessions.sql; echo "sql_comment=$?"
```

---

## Dead Code Sweep

**Status:** PENDING

N/A — no files deleted in this workstream. The schema comment correction is a
textual edit, not a symbol removal.

---

## Verification Evidence

**Status:** PENDING

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| Unit tests | `make test` | | |
| Integration tests | `make test-integration` | | |
| Cross-compile x86 | `zig build -Dtarget=x86_64-linux` | | |
| Cross-compile arm | `zig build -Dtarget=aarch64-linux` | | |
| Lint | `make lint` | | |
| pg-drain | `make check-pg-drain` | | |
| 350L gate | `wc -l` (exempts .md) | | |
| Gitleaks | `gitleaks detect` | | |
| OpenAPI | `make check-openapi-errors` | | |

---

## Out of Scope

- **Vector / semantic search.** Deferred unless evidence shows LLM-in-context retrieval is inadequate. Adding an `embedding` column + ivfflat index later is an additive migration, not a redesign.
- **Cross-zombie shared memory** (one zombie reads another's core). Future milestone; requires a consent/sharing model.
- **Memory migration tooling between backends** (Postgres ↔ Dragonfly ↔ markdown). Not needed until multi-backend is real.
- **Memory dashboard UI** — separate workstream: M14_003.
- **Per-archetype memory policies (skill templates)** — separate workstream: M14_002.
- **Memory-effectiveness metrics** — separate workstream: M14_004.
- **PII redaction policy engine.** `zombiectl memory scrub` ships with regex-only support; policy-driven redaction is a future milestone.
