# Zig Rules

Date: Mar 17, 2026
Status: Canonical Zig source of truth for agents and commits

**Also read:** `docs/greptile-learnings/RULES.md` for cross-language rules including Zig-specific patterns learned from reviews.

## Must

- Run `make lint`, `make test`, and `gitleaks detect` before any commit that includes Zig changes.
- Run `TEST_DATABASE_URL=postgres://usezombie:usezombie@localhost:5432/usezombiedb make test-integration-db` when touching DB-backed handlers, proposal flows, or temp-table-based Zig tests.
- Read this file before creating any new `*.zig` file.
- Use `conn.exec()` for INSERT / UPDATE / DDL whenever possible.
- Drain early-exit `conn.query()` results before `deinit()`.
- Copy row-backed slices before `q.drain()` or `q.deinit()`.
- Materialize rows into owned memory before issuing writes on the same `pg.Conn`.
- Keep temp-table fixtures aligned with the real production write contract.
- Use `var rows: std.ArrayList(T) = .{};` for ArrayList init (Zig 0.15). Pass alloc per-operation: `append(alloc, ...)`, `toOwnedSlice(alloc)`, `deinit(alloc)`.
- Use `q.*.next()` and `q.*.drain()` when the query result is passed through `anytype` as a pointer (`&q`). Direct local vars use `q.next()`.
- Reference nested struct types with the full path: `Module.Struct.NestedType`, not `Module.NestedType`.
- Add `_ = @import("path/to/new_file.zig");` to `main.zig` test discovery block for every new file with tests.

## Must Not

- Do not write on a `pg.Conn` while a read result is still open.
- Do not keep borrowed row data after drain/deinit.
- Do not add extra drain logic after `q.next() == null`; that path is already naturally drained.
- Do not use `ON COMMIT DROP` in temp-table setup driven by `conn.exec()`.
- Do not create ad-hoc DB pool helpers that free parsed URL storage before the pool lifetime ends.
- Do not add a new `.zig` file when an existing module can be extended cleanly.
- Do not use `ArrayList.init(alloc)` — it does not exist in Zig 0.15. Use `= .{}`.
- Do not use `q.next()` on a query result passed via `anytype` pointer — use `q.*.next()`.
- Do not create test files without adding them to `main.zig` test discovery — tests won't run.
- Do not store credentials in plaintext tables — use `crypto_store.store()/load()` with `vault.secrets`.

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

## Type Design Rules

- Use tagged unions (`union(enum)`) when a type has mutually-exclusive variants. Do not use structs with optional fields to represent variant data. The compiler enforces exhaustive switches on tagged unions, catching missing cases at compile time.
- Use `[]const u8` for all immutable data (DB results, parsed input, config values). Reserve `[]u8` for data the function intends to mutate. Mutable slices mislead readers about ownership intent.
- When a struct carries data from different sources (e.g. vault ref + Bearer token), consider whether a tagged union better represents the "exactly one of these" constraint.
- `deinit()` methods on tagged union types must switch on all variants and free only what that variant owns.

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

## Cross-Compile Verification (M22_001)

- Run `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` before every commit that touches Zig files. Do not rely on macOS-only compilation.
- `std.http.Client.open()` does not exist on Linux targets in Zig 0.15.2. Use `client.request()` + `response.reader()` + `readVec()` for cross-platform HTTP streaming.
- `std.Io.Reader` on Linux has `readVec()`, not `read()`. Use `readVec(&[_][]u8{&buf})` for single-buffer reads.
- Verify stdlib API existence by grepping: `grep -n "pub fn" ~/.local/share/mise/installs/zig/*/lib/std/http/Client.zig`

## TLS Transport (M22_001)

- After `tls_writer.flush()`, call `stream_writer.interface.flush()` to actually send encrypted bytes to the socket. The TLS flush only encrypts into the stream writer buffer — it does not send.
- `SO_RCVTIMEO` on a socket fires `WouldBlock` at the socket level, but `Io.Reader` converts it to `ReadFailed` on both plain and TLS transports. Handle `ReadFailed` as timeout.
- `EndOfStream` means clean disconnect — also return null (not fatal) in pub/sub readers.

## SSE Heartbeat Timing (M22_001)

- The heartbeat interval must be LESS than `SO_RCVTIMEO` (socket read timeout). If `SO_RCVTIMEO = 25s` and heartbeat check is at `30s`, the first wakeup at `t=25s` skips the heartbeat (25 < 30) and the proxy drops the connection at `t=30s` before the second wakeup.
- Correct invariant: `heartbeat_interval < SO_RCVTIMEO < proxy_idle_timeout`.
