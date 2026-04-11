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
- Do not use `pub` on constants, types, or functions unless an external file imports them. Default to private; add `pub` only when a consumer exists outside the file.
- When touching a file, audit its `pub` exports: `grep -n "^pub " src/path/file.zig`, then for each symbol `grep -rn "symbol_name" src/ --include="*.zig"` to check if any other file uses it. Remove `pub` from symbols with zero external references. This is progressive cleanup — no need to sweep the whole repo at once.

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

## Progressive Cleanup (apply on file touch)

When you touch a file during any workstream and see one of these patterns,
fix it in the same commit. No spec needed — these are incremental improvements.

- **Comptime struct size assertion.** When modifying a struct used for DB rows,
  metrics snapshots, or wire formats, add `comptime { std.debug.assert(@sizeOf(T) == N); }`
  after the struct definition. This catches silent field drift on the next edit.
  Applies to: `Snapshot`, `EntitlementPolicy`, `ActivityEventRow`, any `*Row` struct.

- **Stack buffer for bounded data.** When you see `alloc.dupe(u8, ...)` or
  `Stringify.valueAlloc(...)` and the output is ≤256 bytes with a known upper
  bound, replace with a stack `var buf: [N]u8 = undefined;` + `std.fmt.bufPrint`
  or `std.io.fixedBufferStream`. Eliminates allocator pressure on hot paths.
  Applies to: UUID formatting, small JSON payloads, log message interpolation.

- **Remove unused `pub`.** When touching a file, run:
  `grep -n "^pub " src/path/file.zig` and for each symbol
  `grep -rn "symbol_name" src/ --include="*.zig"` to check if any other file
  uses it. Remove `pub` from symbols with zero external references.

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

## Zig 0.15.2 API Gotchas (M3_001)

- `std.ArrayList(T).init(alloc)` does not compile. Use `var list: std.ArrayList(T) = .{};` and pass `alloc` per-method: `.append(alloc, item)`, `.deinit(alloc)`, `.toOwnedSlice(alloc)`.
- `std.fmt.parseHex` does not exist. Use `std.fmt.hexToBytes(&out, hex_str)` to decode hex to bytes.
- `std.fmt.fmtSliceHexLower` does not exist. Use `std.fmt.bytesToHex(bytes, .lower)` to encode bytes to hex.
- When unsure about an API, check the codebase first: `grep -rn "ArrayList\|hexToBytes" src/ --include="*.zig"` to see how existing code uses it.

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

## Tagged Unions for Result Types (M4_001)

- When a function returns a decision or outcome with distinct failure modes, use `union(enum)` with payloads — not a bare `enum`. Callers need the *reason*, not just the verdict.
- Bare enums are fine for input classification (e.g., `GateDecision = enum { auto_approve, requires_approval, auto_kill }`) where every variant is handled identically. But for return values where callers branch on failure details, carry the context in the union.
- Example: `GateCheckResult = union(enum) { passed: void, blocked: BlockReason, auto_killed: AutoKillTrigger }` — callers can produce specific error messages without re-deriving context.

## Removed Endpoint Stubs (M10_001)

- When a handler's backing table is dropped, replace it with a 410 stub — do not delete the function unless the route is also removed from the router.
- Use `common.errorResponse(res, .gone, error_codes.ERR_*, "...", req_id)` as the entire body. No arena, no auth, no DB.
- If the route is removed from the router at the same time (pre-production), delete the handler file too and remove all re-exports from `handler.zig`.
- When removing a handler file, also remove it from `m5_handler_changes_test.zig` (or equivalent import-resolution test) — stale `@import` will break the build.

## Comptime Eval Quota + Package Boundary (M11_001)

- Comptime loops over large tables (e.g. 130 codes × 131 TABLE entries × `std.mem.eql`) need `@setEvalBranchQuota(N)` as the first line. Default is 1000. Formula: `N ≈ code_count × table_size × avg_string_len`. Round to next power-of-ten; comment the math.
- `@embedFile` is sandboxed to `src/`. Any path escaping it (`../../public/openapi.json`) is a compile error. For files outside `src/`, write a Python/shell validator invoked via a `make` target wired into `lint-zig`.

## Sentinel Values Must Not Collide With Real Registry Codes (M11_001)

- In any code registry with a fallback sentinel (e.g. `UNKNOWN_ENTRY` in `error_table.zig`), the sentinel's `.code` must NOT match any real registered entry. Use a visually distinct value like `"UZ-UNKNOWN"`. Collision causes tests to pass with wrong semantics and the comptime coverage gate to fail. Add a test that verifies the sentinel is absent from TABLE.

## Module Split Pattern (M4_001)

- When a module hits the line limit, split by concern — not arbitrarily. Preferred extraction order:
  1. Types + parsing → `foo_types.zig` or `foo_config.zig` (re-exported by `foo.zig`)
  2. Tests → `foo_test.zig` (imported via `test { _ = @import("foo_test.zig"); }`)
  3. Integration with other modules → `foo_integration.zig` (thin adapter)
- The original module remains the public API. Extracted modules are implementation details imported only by the parent.
- Do not split into `foo_part1.zig` / `foo_part2.zig` — names must describe the concern, not the split order.

## Struct Init Partial Leak (M6_001)

- Never build a struct literal with multiple `try dupeJsonStr()` calls in a single expression. If a later field's dupe fails, the already-duped fields leak.
- Build field-by-field with `errdefer alloc.free(field)` after each dupe. Only assemble the struct after all fields are successfully allocated.
- The `errdefer` chain unwinds in reverse order, freeing exactly what was allocated.

## Stack Buffer Return Safety (M6_001)

- Never return a `[]const u8` slice that points into a stack-allocated buffer (`var buf: [N]u8`). The stack frame is deallocated when the function returns. The caller reads garbage.
- If you need to return a substring from a stack buffer: either `alloc.dupe()` it, or remove the field from the return type.
- This applies to any function-local array used as a normalization/scratch buffer.

## Multi-Step Init: errdefer Chain Pattern (bvisor)

Every init function that allocates more than one resource must use a sequential errdefer chain — one errdefer immediately after each allocation:

```zig
pub fn init(alloc: Allocator, shared: ?*SharedThing) !*Self {
    // Step 1: resolve or allocate shared dependency
    const thing = shared orelse try SharedThing.init(alloc);
    errdefer if (shared == null) thing.unref(); // only free if WE allocated it

    // Step 2: allocate self
    const self = try alloc.create(Self);
    errdefer alloc.destroy(self);

    // Step 3: allocate inner resources
    const buf = try alloc.alloc(u8, 256);
    errdefer alloc.free(buf);

    self.* = .{ .alloc = alloc, .thing = thing, .buf = buf };
    return self;
}
```

Rules:
- Return `!*Self` for heap-allocated; `!Self` for stack-returned with fallible setup.
- `deinit()` always `pub fn deinit(self: *Self) void` — pointer receiver, void return.
- Conditional ownership: if the caller may have pre-allocated a dependency, the `?*T` + `errdefer if (param == null)` pattern encodes "we own it only when we created it."
- If deinit owns nothing (all fields borrowed), it's still defined — it just calls nothing. Presence of deinit signals "this type has a cleanup contract."

## Ownership Encoding: raw pointer = borrowed, ref/unref = owned (bvisor)

Zig has no borrow checker. Encode ownership in the type, not in documentation:

```zig
// Borrowed — caller owns the lifetime, self never frees it
parent: *ThreadGroup,

// Owned — self manages lifetime via refcount
ref_count: std.atomic.Value(usize),

pub fn ref(self: *Self) *Self {
    _ = self.ref_count.fetchAdd(1, .monotonic);
    return self;
}

pub fn unref(self: *Self) void {
    const prev = self.ref_count.fetchSub(1, .acq_rel);
    if (prev == 1) {
        if (self.parent) |p| p.unref(); // cascade before self-destroy
        self.children.deinit(self.alloc);
        self.alloc.destroy(self);
    }
}
```

When a raw pointer field DOES need a comment (rare — borrowed from a peer, not an ancestor), write: `// borrowed from X, freed by X.deinit()`. Silence implies the pointer is an ancestor/caller lifetime.

## Pg Query Wrapper: use PgQuery, not anytype (M10_004)

**Do not pass `pg.Result` via `anytype` to helper functions.** The `anytype` pattern requires callers to remember `q.*.next()` vs `q.next()` depending on how the value was passed — a compile-silent footgun.

Instead, use the `PgQuery` wrapper in `src/db/pg_query.zig`:

```zig
// caller
var q = PgQuery.from(try conn.query(sql, args));
defer q.deinit(); // auto-drains, then deinits

return someHelper(alloc, &q);

// helper — takes *PgQuery, never anytype
fn someHelper(alloc: Allocator, q: *PgQuery) !Result {
    while (try q.next()) |row| { ... }
}
```

Rules:
- Always `PgQuery.from(conn.query(...))` — never store `pg.Result` directly.
- Use `defer q.deinit()` in the owner. `deinit()` auto-drains idempotently.
- Helpers take `*PgQuery`, never `anytype` — `q.next()` always works, no `q.*.next()`.
- On early exit (parse failure, missing row), just `return` — the `defer` handles drain + deinit.
- `check-pg-drain` lint still runs but now targets only `PgQuery.from()` call sites.
