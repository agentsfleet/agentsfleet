# Ripley's Log — Apr 12, 2026: 09:00 PM

**Branch:** feat/m14-001-persistent-memory
**Workstream:** M14_001 Persistent Zombie Memory

---

## What was shipped

Steps 1-4 were already done when this session started. This session shipped:

- **memory_http.zig + memory_http_helpers.zig**: 4 external-agent HTTP endpoints (store/recall/list/forget) at `/v1/memory/*`. Split into two files to satisfy RULE FLL (350-line gate).
- **Error codes** `UZ-MEM-001/002/003`: scope denial, zombie not found, backend unavailable. Registry count bumped 97 → 100; test assertion updated.
- **`GRANT memory_runtime TO api_runtime`** in schema/026: allows the HTTP handler to `SET ROLE memory_runtime` without needing a superuser. Pre-existing test (zombie_memory_role_test.zig) worked in dev because `usezombie` is a superuser; production `api_runtime` needed explicit membership.
- **OpenAPI**: Memory tag + 4 paths added to `public/openapi.json`.
- **M14_005 spec created** for deferred items.

## Key decisions

**SET ROLE pattern over a second pool.** The alternative was adding `memory_pool: ?*pg.Pool` to `Context` and initializing it with `MEMORY_RUNTIME_URL`. Rejected because: (1) it requires a separate DSN in every deploy environment, (2) the `GRANT memory_runtime TO api_runtime` approach is the canonical Postgres pattern for role escalation, (3) it avoids changing the Context struct and serve.zig startup. The security property is the same: memory queries run under `memory_runtime` grants only.

**Scope check before SET ROLE.** The zombie ownership check (`SELECT workspace_id FROM core.zombies WHERE id = $1`) must run before `SET ROLE memory_runtime`, because `memory_runtime` has no access to `core.*`. The sequence is: verify ownership (api_runtime) → SET ROLE → memory op → RESET ROLE (via defer).

**`conn.exec()` returns `?i64`.** Native `zig build test` silently drops the value; cross-compile (`-Dtarget=x86_64-linux`) flags it as error. Added `_ =` prefix to all `conn.exec()` calls in the new files. This was the only cross-compile divergence.

**`catch break` in `while` condition is invalid in Zig.** The pattern `while (q.next() catch break) |row|` looks plausible but fails compilation — `break` in the catch expression has no target. Fixed with labeled blocks: `collect: while (true) { const row = q.next() catch break :collect; ... }`. Native tests happened to not exercise this path, so it only surfaced at compile time.

## Descoped items

**Retention/pruning (§4, Step 5):** User said "keep it simple, observe how we collect first." Wrote and immediately deleted `memory_prune.zig`. Lesson: don't implement retention before you have data about what gets stored. Deferred to M14_005 with no timeline.

**Export/import and CLI (§3, Step 6/8):** Deferred to M14_005. The HTTP API is the operator-accessible path right now. CLI export/import is a quality-of-life feature, not a blocker for agents using memory.

## Pre-existing issues (not blocking)

- `gitleaks` reports 2 findings in `src/zombie/firewall/content_scanner.zig` — test fixtures with dummy key strings (`sk-proj-abc123`, `sk_live_abc123`). Pre-existing, not from this branch.
- `make lint` exits 127 on `eslint: command not found` — website ESLint binary missing in this environment. Pre-existing.
- Dims 2.2/2.3/2.4 (live-DB isolation, category routing, crash recovery) require a running Postgres instance. Not exercised in unit tests.

## Follow-ups for M14_005

- `schema/023_core_zombie_sessions.sql` comment fix (dim 1.4) — one-line change, kept out of this commit to avoid noise.
- `src/memory/export_import.zig` + `zombiectl memory` CLI.
- Integration tests for zombie isolation (dim 2.2) and external agent scope enforcement (dim 3.4) — need live DB in CI.
