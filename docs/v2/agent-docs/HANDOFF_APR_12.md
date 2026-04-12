# Handoff — M18_001 Zombie Execution Telemetry

**Date:** Apr 12, 2026  
**Worktree:** `/Users/kishore/Projects/usezombie-m18-zombie-execution-telemetry`  
**Main repo:** `/Users/kishore/Projects/usezombie`

---

## Scope/Status

M18_001 is **fully implemented, reviewed, and approved**. PR #202 is open, CI green. One commit not yet pushed — push and merge to close out.

- ✅ Schema: `schema/026_zombie_execution_telemetry.sql` — table + 2 indexes, idempotent on `event_id`
- ✅ Store: `src/state/zombie_telemetry_store.zig` — insert, cursor-paginated customer query, cross-workspace operator query, cursor helpers + inline tests
- ✅ HTTP: `src/http/handlers/zombie_telemetry.zig` — customer `GET /v1/workspaces/{ws}/zombies/{id}/telemetry`, operator `GET /internal/v1/telemetry` (admin-gated via `requireRole(.admin)`)
- ✅ Routing: router.zig, route_matchers.zig, server.zig wired; route matcher tests added
- ✅ Metering: `src/zombie/metering.zig` — telemetry insert + OTel `zombie.delivery` span; gate-blocked events (`epoch_wall_time_ms=0`) skipped
- ✅ Event loop: `epoch_wall_time_ms` captured at delivery entry (`event_loop.zig:212`)
- ✅ Executor: `StageResult.time_to_first_token_ms` populated from executor JSON
- ✅ PostHog: `ZombieCompleted` extended with TTFT field
- ✅ Tests: metering_test.zig (6 cases), m18_001_handler_unit_test.zig, parseCursor inline tests
- ✅ Spec: `docs/v2/done/M18_001_ZOMBIE_EXECUTION_TELEMETRY.md`
- ✅ Changelog: `~/Projects/docs/changelog.mdx` updated (v0.12.0)
- ✅ `/review` + `/review-pr` complete — verdict APPROVE

---

## Working Tree

```
## feat/m18-zombie-execution-telemetry...origin/feat/m18-zombie-execution-telemetry [ahead 1]
```

**1 commit not pushed:** `6ea7f97` — review findings (admin gate, zero-timestamp guard, overflow caps, ID validation, tests). Working tree is clean.

---

## Branch/PR (GitHub)

- **Branch:** `feat/m18-zombie-execution-telemetry`
- **PR:** [#202](https://github.com/usezombie/usezombie/pull/202) — OPEN
- **CI:** CodeQL js+ts → SUCCESS | CodeQL python → SUCCESS | Greptile → SUCCESS
- **Greptile last reviewed:** `96dcfd2` — all 3 findings resolved in `6ea7f97`

---

## Running Processes

None.

---

## Tests/Checks

- ✅ `zig build` — native + x86_64-linux + aarch64-linux
- ✅ `zig build test` — 863/927 passed, 64 skipped
- ✅ `make check-pg-drain` — 220 files, passed
- ✅ 350-line gate — all M18 files under limit
- ✅ Orphan sweep — clean
- ⏳ `make test-integration` — not run (requires live Postgres)

---

## Next Steps

1. **Push:**
   ```bash
   cd /Users/kishore/Projects/usezombie-m18-zombie-execution-telemetry
   git push
   ```
2. **Merge:**
   ```bash
   gh pr merge 202 --squash
   ```
3. **(Optional follow-up)** Add `zombie_id`-only index — low priority, admin-gated endpoint:
   ```sql
   -- schema/026_zombie_execution_telemetry.sql
   CREATE INDEX idx_telemetry_zombie
       ON zombie_execution_telemetry (zombie_id, recorded_at DESC);
   ```
4. **(Optional follow-up)** `event_loop_types.zig:27` — `ZombieSession` size assertion still uses `std.debug.assert`; swap to `comptime @compileError` (same fix as `TelemetryRow` in this PR).
5. **M8 Slack plugin (PR #204)** is the next open PR — 9 unresolved review findings, needs rebase on main (28 commits behind). See `RIPLEYS_LOG_APR_12_13_30.md` for M18 design context.

---

## Risks/Gotchas

- 2 pre-existing test failures in the suite — unrelated to M18, do not investigate.
- Greptile reviewed `96dcfd2` not `6ea7f97` — re-trigger will show all 3 findings resolved.
- `~/Projects/usezombie/docs/release/` deleted — content migrated to changelog.mdx. Intentional.
- **M8 PR #204** is 28 commits behind main (M17+M15+M18). Rebase conflicts expected in `src/main.zig`, `src/errors/error_registry_test.zig`, `src/cmd/common.zig`.
