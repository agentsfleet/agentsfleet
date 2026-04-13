# M14_005: Zombie Memory Export/Import and CLI

**Prototype:** v2
**Milestone:** M14
**Workstream:** 005
**Date:** Apr 12, 2026: 03:30 PM
**Status:** PENDING
**Branch:** (not started)
**Priority:** P2 — Operator tooling; agents already persist memory via M14_001
**Batch:** B2
**Depends on:** M14_001 (persistent memory schema + HTTP API shipped)
**UI equivalent:** M24_001 (B7) — the dashboard import flow (`POST /v1/zombies/{id}/memory/import`) covers the same zip-upload-and-upsert operation this CLI wraps. The two paths must produce identical results; M24_001 reuses this workstream's zip format and import API. Non-CLI operators use M24_001; power users and agents use this CLI.

---

## Overview

**Goal (testable):** An operator can run `zombiectl memory export --zombie zom_abc --out ./mem/`, edit a markdown file, run `zombiectl memory import --zombie zom_abc --from ./mem/`, and on the next zombie run `memory_recall` returns the edited content.

**Context:** M14_001 shipped the core storage path (executor wiring, row-level isolation, HTTP API). This workstream adds the human-readable layer: a markdown export/import format so operators can audit, edit, and bulk-update zombie memory from their own laptops. It also adds `zombiectl memory` subcommands as the operator CLI surface.

**Deferred items from M14_001:**
- `src/memory/export_import.zig` (M14_001 §3 dims 3.1-3.2)
- `zombiectl/src/commands/memory.js` (M14_001 §3 dim 3.3)
- `schema/023_core_zombie_sessions.sql` comment fix (M14_001 dim 1.4)
- §4 retention/pruning job — observe collection patterns first; revisit when data exists

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `src/memory/export_import.zig` | CREATE | Export zombie memory to markdown; import from edited markdown with upsert |
| `zombiectl/src/commands/memory.js` | CREATE | `zombiectl memory export|import|forget|scrub` CLI surface |
| `schema/023_core_zombie_sessions.sql` | MODIFY | Fix comment: context_json is a conversation resume bookmark, not agent memory |

---

## §1 — Export/Import Library

**Status:** PENDING

`exportZombie` writes one markdown file per entry (key → filename, frontmatter, content body).
`importZombie` reads the folder, validates frontmatter `zombie_id`, upserts in a single transaction.

**Dimensions:**

| Dim | Status | Target | Input | Expected | Test type |
|-----|--------|--------|-------|----------|-----------|
| 1.1 | PENDING | `src/memory/export_import.zig:exportZombie` | zombie with 50 core + 10 daily entries | one markdown file per entry under `core/` and `daily/` subdirs; frontmatter has `key`, `category`, `zombie_id`, `updated_at`; body is content verbatim | unit |
| 1.2 | PENDING | `src/memory/export_import.zig:importZombie` | folder of edited markdown for `zom_abc` | upserts entries scoped to `zom_abc` only; rejects entries whose frontmatter `zombie_id` mismatches; wraps all upserts in a single transaction; ROLLBACK on any failure | integration |
| 1.3 | PENDING | roundtrip | export → no edits → import | zero diff; count unchanged | integration |
| 1.4 | PENDING | edit-then-replay | export → edit content → import → memory_recall | edited content returned on next recall | integration |

---

## §2 — CLI Surface

**Status:** PENDING

| Dim | Status | Target | Input | Expected | Test type |
|-----|--------|--------|-------|----------|-----------|
| 2.1 | PENDING | `zombiectl memory export` | `--zombie <id> --out ./mem/` | files written; summary printed | CLI integration |
| 2.2 | PENDING | `zombiectl memory import` | `--zombie <id> --from ./mem/` | upsert count printed; mismatches listed | CLI integration |
| 2.3 | PENDING | `zombiectl memory forget` | `--zombie <id> --key <key>` | entry deleted; idempotent | CLI integration |

---

## §3 — Schema Comment Fix

**Status:** PENDING

| Dim | Status | Target | Input | Expected | Test type |
|-----|--------|--------|-------|----------|-----------|
| 3.1 | PENDING | `schema/023_core_zombie_sessions.sql` | read comment | comment says "conversation resume bookmark storing {last_event_id, last_response}; NOT agent memory" | contract |

---

## Interfaces

```zig
// src/memory/export_import.zig
pub fn exportZombie(alloc: Allocator, conn: *pg.Conn, zombie_id: []const u8, out_dir: []const u8) !ExportSummary;
pub fn importZombie(alloc: Allocator, conn: *pg.Conn, zombie_id: []const u8, in_dir: []const u8) !ImportSummary;

pub const ExportSummary = struct { count: u32, bytes: u64 };
pub const ImportSummary = struct { upserted: u32, rejected: u32, errors: [][]const u8 };
```

```
zombiectl memory export  --zombie <id> --out <dir>  [--category core|daily]
zombiectl memory import  --zombie <id> --from <dir> [--mode upsert|replace]
zombiectl memory forget  --zombie <id> --key <key>
```

---

## Applicable Rules

- **RULE FLS** — drain all pg query results before deinit
- **RULE FLL** — 350-line gate on every .zig/.js file
- **RULE TXN** — all import upserts in a single transaction; ROLLBACK on failure
- **RULE XCC** — cross-compile before commit
- **RULE CTX** — use SET ROLE memory_runtime for all memory ops (same pattern as memory_http.zig)
- **SCHEMA GUARD** — any schema edit triggers guard (023 comment-only edit is pre-v2.0 teardown era)

---

## Execution Plan

| Step | Action | Verify |
|------|--------|--------|
| 1 | Fix `schema/023_core_zombie_sessions.sql` comment | comment updated; `zig build` passes |
| 2 | Implement `src/memory/export_import.zig` with exportZombie + importZombie | dims 1.1-1.4 pass |
| 3 | Add `zombiectl/src/commands/memory.js` CLI commands | dims 2.1-2.3 pass |
| 4 | Cross-compile + full gate | `zig build -Dtarget=x86_64-linux && aarch64-linux`; pg-drain; lint |

---

## Acceptance Criteria

- [ ] `zombiectl memory export|import` edit-then-replay roundtrip — verify: `make test-integration` (`edit_then_replay`)
- [ ] Import rejects entries with mismatched zombie_id — verify: unit test
- [ ] Import wraps all upserts in one transaction; partial failure rolls back — verify: integration test
- [ ] `zombiectl memory forget` removes entry; idempotent on second call — verify: CLI integration test
