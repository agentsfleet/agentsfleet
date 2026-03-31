# Zig Rules

Date: Mar 17, 2026
Status: Canonical Zig source of truth for agents and commits

## Must

- Run `make lint`, `make test`, and `gitleaks detect` before any commit that includes Zig changes.
- Run `TEST_DATABASE_URL=postgres://usezombie:usezombie@localhost:5432/usezombiedb make test-integration-db` when touching DB-backed handlers, proposal flows, or temp-table-based Zig tests.
- Read this file before creating any new `*.zig` file.
- Use `conn.exec()` for INSERT / UPDATE / DDL whenever possible.
- Drain early-exit `conn.query()` results before `deinit()`.
- Copy row-backed slices before `q.drain()` or `q.deinit()`.
- Materialize rows into owned memory before issuing writes on the same `pg.Conn`.
- Keep temp-table fixtures aligned with the real production write contract.

## Must Not

- Do not write on a `pg.Conn` while a read result is still open.
- Do not keep borrowed row data after drain/deinit.
- Do not add extra drain logic after `q.next() == null`; that path is already naturally drained.
- Do not use `ON COMMIT DROP` in temp-table setup driven by `conn.exec()`.
- Do not create ad-hoc DB pool helpers that free parsed URL storage before the pool lifetime ends.
- Do not add a new `.zig` file when an existing module can be extended cleanly.

## Allowed Exceptions

- `q.drain() catch {}` is allowed only for intentional DB cleanup paths and should stay adjacent to the drain/deinit sequence.
- `catch {}` outside DB cleanup must be explicitly best-effort and easy to justify in review.
- `undefined` in low-level initialization paths must be deliberate and, when non-obvious, documented with a short safety comment.

## ZLint Policy

- This repo uses `zlint` as part of `make lint`.
- Pinned version: `v0.7.9`.
- `suppressed-errors` stays off because this repo intentionally uses narrow `pg` cleanup patterns that a generic rule cannot classify correctly.
- `unsafe-undefined` is a good future tightening target once current low-level uses are cleaned up or annotated.
- A disabled ZLint is not useful; prefer a scoped ruleset that passes today and tightens over time.

## Memory Safety Rules

- When returning slices from a function that uses `defer resource.deinit()`, always `alloc.dupe()` the slices before the return statement. The defer fires after return evaluation but before the caller receives the value — returning a borrowed slice is a use-after-free.
- For child process timeout enforcement, use a timer thread + `child.kill()`, not a poll loop around `child.wait()`. `child.wait()` blocks the calling thread — the timeout check after it is dead code.
- Always free heap-allocated return values (`formatX`, `buildX`, `getToken`) with `defer alloc.free(result)` immediately after the call. Do not rely on arena allocators to mask leaks — arena-freed code may later be called outside an arena.
- Test allocation-heavy functions with `std.testing.allocator` (not an arena) so the leak detector fires on missed frees.

## New File Rules

- Prefer extending an existing Zig module unless a new file clearly reduces coupling or keeps module size reviewable.
- Decide ownership before writing helpers: allocator, free/deinit path, and whether data is owned or borrowed.
- If the file touches `pg`, apply the query lifecycle rules above before writing the first helper.

## No Hardcoded Roles

- Never use `ROLE_ECHO`, `ROLE_SCOUT`, or `ROLE_WARDEN` string constants in production code. These constants were removed in M20_001.
- Never string-compare against `"echo"`, `"scout"`, or `"warden"` to identify roles or skills in dispatch logic. Roles and skills are loaded from the active pipeline profile at runtime.
- The active profile's `skill_ids` are the source of truth for what skills are valid. Use `topology.defaultProfile()` to load the default skill set for entitlement and policy checks.
- The `SkillKind` enum has a single variant `.custom` — all skills are equal from the registry's perspective. The execution backend is determined by which runner was registered for a skill_id.
- Lint gate: `make lint-zig` runs `_hardcoded_role_check` to enforce this rule on every commit.

## Commands

- `make lint`
- `make test`
- `TEST_DATABASE_URL=postgres://usezombie:usezombie@localhost:5432/usezombiedb make test-integration-db`
- `gitleaks detect`
- `make check-pg-drain` — static check: every `conn.query()` must have `.drain()` in the same function. Run this when touching any file that calls `conn.query()`. See `lint-zig.py`.
