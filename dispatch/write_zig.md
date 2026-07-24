# write_zig.md — Zig latent façade

This is the prose the AGENT reads before writing any `*.zig` file. It pairs with the deterministic façade `dispatch/write_zig.sh` — the machine half that runs the mechanically-checkable subset and emits one verdict block. This document consolidates the former Zig rules merged with the Zig-relevant dissolved gate-card deltas: every original rule line is preserved verbatim, each `## ` section now carries exactly one enforcement tag, and the dissolved gate-card prose is appended under "Merged from dissolved gate cards." Mechanical thresholds live once in the `.sh`; this file references rule codes, never restates the numbers.

**Signal legend** (printed by `write_zig.sh`):

- 🟢 pass — deterministic check passed.
- 🔴 fail — deterministic check failed (or helper absent); STOP, fix, rerun.
- 🔵 DECIDE — judgment-only; no script can decide, the agent reads the section and makes the call (blocks the TURN, not the script).
- ⚪ delegated — the checker runs only in the product repo, not in dotfiles.

**Tag legend** — each section heading below carries one of:

- `> [DETERMINISTIC → <CODE>]` — a machine can pass/fail it; the `.sh` row for `<CODE>` (e.g. `UFS`, `DEINIT`, `DRAIN`, `XCOMPILE`, `PUB`) enforces it. `TODO-CHECK` marks a mechanizable rule with no helper wired yet (build-the-check). `NEW:<name>` marks a proposed-but-not-yet-existing code.
- `> [JUDGMENT → <CODE>]` — no script can decide; the agent decides at write time against the prose.
- `> [container]` — a non-enforcement wrapper heading (e.g. "Merged from dissolved gate cards"); its tagged subsections carry the real codes, and the coherence audit (§6.3) skips it.

The tag grammar and semantic-anchor model are defined by the tagged sections and
deterministic façade in this snapshot.

---

# Zig Rules

Date: Mar 17, 2026
Status: Canonical Zig source of truth for agents and commits

**Also read:** `docs/greptile-learnings/RULES.md` for cross-language rules including Zig-specific patterns learned from reviews.

## Verification workflow

> [JUDGMENT → ARCH]

For every commit that touches `*.zig`, the agent runs the workflow below — no human approvals required mid-loop. This façade is the human-discipline complement to `make lint`; rules already enforced by lint are not duplicated here. The agent owns the rules that lint cannot mechanically catch.

1. **Before write (trigger: about to edit / create `*.zig`):** scan this façade's section headers (`grep -n "^## " dispatch/write_zig.md`). Re-read any section whose topic the diff touches: concurrency for atomics / threads, allocator-ownership for new structs, doc-comments for new `pub` types, comptime assertions for new invariants, single-type-module pattern for new file structure, etc.

2. **During write:** for each surfaced uncertainty (atomic ordering choice, allocator pattern, naming, structural choice), state the rule and the choice in chat — don't decide silently.

3. **Before commit (post-`make lint`):** run a self-audit grep against the staged diff for the rules `make lint` does not enforce. Don't ask for approval; do the audit and either comply or state explicitly why an exception applies.
    - Weak atomic orderings (`\.\(acquire\|release\|monotonic\|unordered\)\b`) — every match needs a `// safe because: ...` comment within 3 lines.
    - New `pub` symbols — confirm at least one external import via `grep -rn "<symbol>"`. Remove `pub` if unreferenced.
    - New structs that own heap memory — confirm `alloc:` field OR doc-comment naming the caller-owned-allocator pattern.
    - Mutex `\.lock\(\)` calls — confirm immediately followed by `defer .*\.unlock\(\)`.
    - New `pub` types/functions — confirm `///` doc-comment present.
    - New hot-path loops, buffers, threads, queues, or per-request work — confirm a `RESOURCE BUDGET:` line was printed before the edit.
    - New allocator choice (`ArenaAllocator`, fixed buffer, page allocator, caller allocator, stored allocator) — confirm an `ALLOCATOR CHOICE:` line was printed before the edit.
    - New long-running worker, child process, socket/read loop, or blocking wait — confirm timeout, cancellation, and join/cleanup paths are named in the diff or adjacent comments.

4. **After `dispatch/write_zig.md` edits land:** every active branch with uncommitted Zig work must rerun steps 1–3 against the updated rules. Do not assume yesterday's audit covers today's rules. The agent is responsible for re-checking; the user is not the gate.

5. **Test discovery hygiene:** when extracting tests to a new file, verify discovery by adding an import to `main.zig` (or a `test {}` façade block), per the "New File Rules" section below.

## Must

> [DETERMINISTIC → DRAIN]

- Run `make lint`, `make test`, and `gitleaks detect` before any commit that includes Zig changes.
- Run `TEST_DATABASE_URL=postgres://agentsfleet:agentsfleet@localhost:5432/agentsfleetdb make test-integration-db` when touching DB-backed handlers, proposal flows, or temp-table-based Zig tests.
- Read this file before creating any new `*.zig` file.
- Use `conn.exec()` for INSERT / UPDATE / DDL whenever possible.
- Drain early-exit `conn.query()` results before `deinit()`.
- Copy row-backed slices before `q.drain()` or `q.deinit()`.
- Materialize rows into owned memory before issuing writes on the same `pg.Conn`.
- Keep temp-table fixtures aligned with the real production write contract.
- Use `var rows: std.ArrayList(T) = .empty;` for ArrayList init (Zig 0.16; the `= .{}` form was 0.15). Pass alloc per-operation: `append(alloc, ...)`, `toOwnedSlice(alloc)`, `deinit(alloc)`.
- Use `q.*.next()` and `q.*.drain()` when the query result is passed through `anytype` as a pointer (`&q`). Direct local vars use `q.next()`.
- Reference nested struct types with the full path: `Module.Struct.NestedType`, not `Module.NestedType`.
- Add new test files to test discovery. Either `_ = @import("path/to/new_file.zig");` in `main.zig`'s test block, or — preferred when a façade already exists — in a `test {}` block inside the façade (`test { _ = @import("foo_test.zig"); }`). Zig strips `test {}` blocks in release builds, so this adds zero bytes to the production binary. The façade pattern keeps `main.zig` at a flat one-line-per-module list and lets each module own its own test discovery.
- **New `*.zig` file with exactly one primary type → use file-as-struct layout** (`const Foo = @This();` with fields immediately after, methods next, imports at file end). Multi-type modules (tagged unions + helpers, protocols with multiple shapes) keep conventional `pub const Foo = struct { ... };` layout. See "Single-Type-Module Pattern" below for the canonical shape.
- **Surface a Pub Surface & Struct-Shape Gate block** (defined in `AGENTS.md`) before saving a new `*.zig` file or any edit that adds a new `pub` symbol. The gate forces choosing the layout and justifying every new `pub` against an external consumer.

## Must Not

> [DETERMINISTIC → DRAIN]

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

> [JUDGMENT → TGU]

- `q.drain() catch {}` is allowed only for intentional DB cleanup paths and should stay adjacent to the drain/deinit sequence.
- `catch {}` outside DB cleanup must be explicitly best-effort and easy to justify in review.
- `undefined` in low-level initialization paths must be deliberate and, when non-obvious, documented with a short safety comment.

## ZLint Policy

> [DETERMINISTIC → PUB]

- This repo uses `zlint` as part of `make lint`.
- Pinned version: `v0.9.0`.
- **`unused-decls: error` is load-bearing.** PUB GATE (the pub-surface section of this façade) delegates mechanical consumer-grep to this rule — a `pub` without an in-tree consumer fails `make lint`. Disabling or downgrading it silently bypasses half the gate; if you must, amend PUB GATE in the same diff so the design call is captured elsewhere.
- `suppressed-errors` stays off because this repo intentionally uses narrow `pg` cleanup patterns that a generic rule cannot classify correctly.
- `unsafe-undefined` is a good future tightening target once current low-level uses are cleaned up or annotated.
- A disabled ZLint is not useful; prefer a scoped ruleset that passes today and tightens over time.

## Memory Safety Rules

> [DETERMINISTIC → DEINIT]

- When returning slices from a function that uses `defer resource.deinit()`, always `alloc.dupe()` the slices before the return statement. The defer fires after return evaluation but before the caller receives the value — returning a borrowed slice is a use-after-free.
- For child process timeout enforcement, use a timer thread + `child.kill()`, not a poll loop around `child.wait()`. `child.wait()` blocks the calling thread — the timeout check after it is dead code.
- Always free heap-allocated return values (`formatX`, `buildX`, `getToken`) with `defer alloc.free(result)` immediately after the call. Do not rely on arena allocators to mask leaks — arena-freed code may later be called outside an arena.
- Test allocation-heavy functions with `std.testing.allocator` (not an arena) so the leak detector fires on missed frees.

## Deterministic Resource Budget Gate

> [DETERMINISTIC → TODO-CHECK]

Every hot path needs a visible budget before code is written. This turns "optimize memory and Central Processing Unit (CPU)" into a reviewable shape instead of taste.

**Triggers** — every Edit/Write to a `*.zig` file that net-adds any of these:

- A request/message/row-processing loop.
- A byte buffer, response builder, serializer, parser, or formatter whose retained size depends on external input or runs in a request/worker loop.
- A thread, worker, child process, queue, mutex, atomic coordination flag, or blocking wait.
- Per-request or per-event heap allocation.
- Whole-body reads of network, file, or database data.

**Required output before the edit:**

```text
RESOURCE BUDGET: <function/type>
  Heap allocations: <0 | N | per item | per request>
  Max retained bytes: <bound or reason unbounded is impossible>
  Max loop cardinality: <bound or streaming>
  Concurrency: <single-thread | bounded N | thread-pool N | none>
  Timeout/cancel path: <how it stops and who joins/cleans it>
```

Rules:

- No unbounded thread spawn, queue growth, retry loop, body read, or result accumulation. If the input is unbounded, stream it or reject it at a named limit.
- No per-item allocation in hot loops unless the returned ownership requires it. Prefer stack/fixed buffers, caller-provided scratch, or one outer allocation.
- Long-running work must name a stop path: timeout, close signal, cancellation flag, queue close, child kill, and final `join`/`wait` cleanup as applicable.
- CPU-heavy work must not block an Input/Output (I/O) coordination thread. Move it to bounded workers or make the caller pay an explicit synchronous cost.
- Tests for hot-path helpers should assert the behavior and the ownership path; allocation-heavy helpers use `std.testing.allocator` so leaks fail deterministically.

**Self-audit at end of turn:** run `git diff -U0 HEAD -- '*.zig' | grep -E '^\+.*(while |for \(|Thread|spawn|Mutex|atomic|ArrayList|alloc\.|readAll|readToEnd|wait\(|sleep\()' | head`. Non-empty means the edit likely touched a budgeted surface; verify a `RESOURCE BUDGET:` line was printed before the edit or document why the grep hit is a false positive.

## Allocator Choice Gate

> [DETERMINISTIC → TODO-CHECK]

Allocator choice is part of the API shape. Pick it before writing the allocation.

**Required output before net-adding allocator-sensitive code:**

```text
ALLOCATOR CHOICE: <function/type> uses <caller | stored | arena | fixed-buffer | page | testing>
Reason: <lifetime, ownership, failure mode>
```

Rules:

- Default production functions accept a caller allocator. Do not hide allocation behind a global allocator.
- Structs that own heap memory store `alloc: std.mem.Allocator` unless they are short-lived values with a documented caller-owned `deinit(self, alloc)` pattern.
- Use arenas only when every allocation dies together and the arena lifetime is shorter than the request/job. Arena use must not mask missing frees in reusable helpers.
- Prefer stack or fixed buffers for bounded scratch data. If bounded scratch can overflow to heap, name the threshold and test both paths.
- Use `std.testing.allocator` in tests that are meant to prove leak freedom. Do not use an arena in a leak-safety test.
- Use the page allocator only for process-lifetime allocations or low-level primitives where page granularity is the point; document that lifetime.

## Out of Memory and Partial-Init Rules

> [DETERMINISTIC → DEINIT]

Out of Memory (OOM) is a normal failure path, not a cleanup surprise.

- Every init/build function that allocates more than one resource uses the lifecycle-approved partial-init pattern: either attach an adjacent `errdefer` per owned resource, or attach one adjacent wrapper `errdefer` that frees all fields already transferred into the wrapper.
- Do not put multiple `try alloc.*` or `try dupe*` calls inside one struct literal, array literal, function argument list, or `return` expression. If a later allocation fails, earlier ownership is invisible and easy to leak.
- A function returning owned memory must have an obvious caller free path in the same test or call site. Tests should cover success cleanup and at least one allocation-failure or parse-failure branch when practical.
- Fallible cleanup should not hide OOM. Cleanup may ignore best-effort drain errors only where this file explicitly allows it; allocator frees are not fallible and must run.

## Panic, Hang, and Shutdown Policy

> [JUDGMENT → NEW:HANG]

- Library, handler, worker, parser, and service code returns errors for user data, network failures, environment failures, and OOM. It does not `@panic` for recoverable failures.
- `unreachable` is allowed only after a compiler-exhaustive switch or with an adjacent invariant comment explaining why runtime input cannot reach it.
- `std.debug.assert` is for programmer bugs and invariants that should disappear in release builds. Never use it to validate external input.
- Thread entrypoints and worker loops catch/report errors through their owner-visible channel. They do not silently die, spin forever, or leave ownership half-cleaned.
- Every blocking wait introduced by a diff names its shutdown path. A wait without timeout is allowed only when another owner-controlled signal can always wake it and the owning code joins it.

## Type Design Rules

> [JUDGMENT → TGU]

- Use tagged unions (`union(enum)`) when a type has mutually-exclusive variants. Do not use structs with optional fields to represent variant data. The compiler enforces exhaustive switches on tagged unions, catching missing cases at compile time.
- Use `[]const u8` for all immutable data (DB results, parsed input, config values). Reserve `[]u8` for data the function intends to mutate. Mutable slices mislead readers about ownership intent.
- When a struct carries data from different sources (e.g. vault ref + Bearer token), consider whether a tagged union better represents the "exactly one of these" constraint.
- `deinit()` methods on tagged union types must switch on all variants and free only what that variant owns.

## Buffer Type Selection (BUFFER GATE)

> [DETERMINISTIC → TODO-CHECK]

Three byte-accumulation tools are available; picking the wrong one creates realloc churn or unnecessary materialize-and-copy steps. Pick deliberately.

**Triggers** — every Edit/Write to a `*.zig` file that net-adds byte-accumulation code (request bodies, JSON serialization, log construction, byte recorders, response builders, anything assembling many writes into one slice):

- A new `std.ArrayList(u8)` declaration intended as a byte buffer.
- A new `StringBuilder.init` / `initCapacity` call (`src/util/strings/string_builder.zig`).
- A new `StringJoiner.init` call (`src/util/strings/string_joiner.zig`).
- Any new function whose body is a write/append loop accumulating into a buffer.

**Decision table:**

| Tool                | Use when                                                  | Why                                                                                                                                        |
| ------------------- | --------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------ |
| `StringBuilder`     | Total size known up front                                 | Two-phase `count → allocate → append`. Single allocation; slices borrow from one backing buffer. No realloc churn.                          |
| `StringJoiner`      | Many small slices, total size unknown                     | Rope of nodes; `pushStatic` (borrow) / `pushCloned` (own); `done(alloc)` materializes once. Lower realloc cost than `ArrayList` on heavy writes. |
| `std.ArrayList(u8)` | Append-as-you-go AND need random-access reads of `.items` | One contiguous slice always available — direct `std.mem.indexOf` / grep. Realloc churn on growth, but fine when read access dominates write throughput. |

**Required output (before the edit):** `BUFFER GATE: <ArrayList|StringBuilder|StringJoiner> for <field/var name> — <one-line reason matching the table>.`

**Self-audit at end of turn (before declaring done):** run `git diff -U0 HEAD -- '*.zig' | grep -E '^\+.*(std\.ArrayList\(u8\)|StringBuilder\.init|StringJoiner\.init)\b' | head`. Non-empty = new buffer accumulation introduced this turn; verify a `BUFFER GATE:` line was printed before each corresponding Edit/Write. Missing gate lines are a violation, caught here.

False positives: `std.ArrayList(u8)` used as a non-buffer list of bytes (rare). The fix is still to print a `BUFFER GATE: ArrayList for <var> — list of bytes, not append-loop accumulator` so the choice is visible.

**Override syntax:** `BUFFER GATE: SKIPPED per user override (reason: ...)` immediately preceding the edit.

## Progressive Cleanup (apply on file touch)

> [DETERMINISTIC → PUB]

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

> [DETERMINISTIC → PUB]

- Prefer extending an existing Zig module unless a new file clearly reduces coupling or keeps module size reviewable.
- Decide ownership before writing helpers: allocator, free/deinit path, and whether data is owned or borrowed.
- If the file touches `pg`, apply the query lifecycle rules above before writing the first helper.

## HTTP Integration Tests — Use TestHarness

> [JUDGMENT → ARCH]

Canonical source: `src/http/test_harness.zig`. Every `*_http_integration_test.zig` under `src/http/` MUST consume it.

- **Do not** define a local `TestServer` / `RunningServer` struct, local `startTestServer()` / `startServer()`, local `sendReq()` / `sendRequest()`, or local `waitForServer()`. These are provided by `TestHarness` with a fluent Request/Response API.
- **Do** wire middleware via `Config.configureRegistry: fn(*MiddlewareRegistry, *TestHarness) anyerror!void`. The harness is policy-agnostic; each suite supplies only the middleware it exercises.
- **Integration tests run against the LIVE test DB** — never `CREATE TEMP TABLE` in an integration test. Fixtures go through the real schema so the code under test sees production-shaped rows. The `make test-integration` gate resets schemas via `_reset-test-db`; individual tests clean up their own rows explicitly in the test body (not via `defer`) because deferred cleanup leaks pool connections at `pool.deinit()`.
- **Test fixtures live in a sibling `*_test_fixtures.zig`** beside the integration file, or in a shared fixture module — not inlined. See `src/http/webhook_test_fixtures.zig` for the pattern.
- **Skip gracefully when DB is absent** — `TestHarness.start` returns `error.SkipZigTest` when `TEST_DATABASE_URL` is unset; tests just propagate it.
- New file registration: add `_ = @import("..._test.zig")` to the `test {}` block at the bottom of `src/http/server.zig` (not `src/main.zig`). Integration tests discover from there.

## Commands

> [DETERMINISTIC → DRAIN]

- `make lint`
- `make test`
- `TEST_DATABASE_URL=postgres://agentsfleet:agentsfleet@localhost:5432/agentsfleetdb make test-integration-db`
- `gitleaks detect`
- `make check-pg-drain` — static check: every `conn.query()` must have `.drain()` in the same function. Run this when touching any file that calls `conn.query()`. See `lint-zig.py`.

## Zig 0.15.2 API Gotchas (M3_001)

> [DETERMINISTIC → XCOMPILE]

- `std.ArrayList(T).init(alloc)` does not compile. Use `var list: std.ArrayList(T) = .{};` and pass `alloc` per-method: `.append(alloc, item)`, `.deinit(alloc)`, `.toOwnedSlice(alloc)`.
- `std.fmt.parseHex` does not exist. Use `std.fmt.hexToBytes(&out, hex_str)` to decode hex to bytes.
- `std.fmt.fmtSliceHexLower` does not exist. Use `std.fmt.bytesToHex(bytes, .lower)` to encode bytes to hex.
- When unsure about an API, check the codebase first: `grep -rn "ArrayList\|hexToBytes" src/ --include="*.zig"` to see how existing code uses it.

## Cross-Compile Verification (M22_001)

> [DETERMINISTIC → XCOMPILE]

- Run `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` before every commit that touches Zig files. Do not rely on macOS-only compilation.
- **Production-binary targets alone do NOT catch Linux-gated drift.** Code inside `if (builtin.os.tag == .linux)` branches and test-only helpers is comptime-dead on a macOS target and never analysed in the production graph (M82 shipped green locally; three Linux-gated `std.fs.accessAbsolute` sites failed CI). For cross-platform changes, also compile the linux **test graphs**: `zig build test -Dtarget=x86_64-linux` + `zig build test-lib -Dtarget=x86_64-linux` + `zig build --build-file build_runner.zig test -Dtarget=x86_64-linux`. A clean compile ending only in `unable to execute binaries from the target` is the PASS signal.
- **Linux-gated tests compile but never RUN on macOS** (`SkipZigTest` off-linux) — a wrong runtime assertion slips through to CI (M84_007). To execute locally: cross-compile the test graph `-Dtarget=aarch64-linux`, take the static ELF from `.zig-cache/o/*/`, and run it in a **native arm64** container: `docker run --rm --platform linux/arm64 -v "$PWD:/w" -w /w debian:stable-slim /w/<binary>`. qemu-emulated x86_64 is a false oracle — its fork/clone emulation breaks tests that pass on real Linux. Note: prod children run under bwrap (closes non-passed fds), so bwrap-less tests must assert the *relative* fd property (child fds ⊆ parent), never absolute fd counts.
- `std.http.Client.open()` does not exist on Linux targets in Zig 0.15.2. Use `client.request()` + `response.reader()` + `readVec()` for cross-platform HTTP streaming.
- `std.Io.Reader` on Linux has `readVec()`, not `read()`. Use `readVec(&[_][]u8{&buf})` for single-buffer reads.
- Verify stdlib API existence by grepping: `grep -n "pub fn" ~/.local/share/mise/installs/zig/*/lib/std/http/Client.zig`

## TLS Transport (M22_001)

> [DETERMINISTIC → XCOMPILE]

- After `tls_writer.flush()`, call `stream_writer.interface.flush()` to actually send encrypted bytes to the socket. The TLS flush only encrypts into the stream writer buffer — it does not send.
- `SO_RCVTIMEO` on a socket fires `WouldBlock` at the socket level, but `Io.Reader` converts it to `ReadFailed` on both plain and TLS transports. Handle `ReadFailed` as timeout.
- `EndOfStream` means clean disconnect — also return null (not fatal) in pub/sub readers.

## SSE Heartbeat Timing (M22_001)

> [JUDGMENT → NEW:HANG]

- The heartbeat interval must be LESS than `SO_RCVTIMEO` (socket read timeout). If `SO_RCVTIMEO = 25s` and heartbeat check is at `30s`, the first wakeup at `t=25s` skips the heartbeat (25 < 30) and the proxy drops the connection at `t=30s` before the second wakeup.
- Correct invariant: `heartbeat_interval < SO_RCVTIMEO < proxy_idle_timeout`.

## Listener Shutdown Must Wake accept() — Linux (0.16)

> [JUDGMENT → NEW:HANG]

- `listener.deinit(io)` from another thread does NOT unblock a thread inside `std.Io.net.Server.accept(io)` on **Linux** (it does on macOS/BSD) — so `acceptor.join()` hangs forever, and deinit-during-accept races the acceptor. Passes locally, dies only in Linux CI (M88_002: 10-minute timeout, zero test output).
- Shutdown pattern: (1) `stop.store(true)`; (2) one throwaway loopback connect to the listen port to wake the blocked `accept()`; (3) acceptor accepts it, sees `stop`, closes, returns; (4) `acceptor.join()`; (5) only THEN `listener.deinit(io)` — no thread may still sit in `accept` at deinit.
- Verify any Linux-gated hang fix in a native arm64 container (Cross-Compile Verification above), not on macOS.

## Tagged Unions for Result Types (M4_001)

> [JUDGMENT → TGU]

- When a function returns a decision or outcome with distinct failure modes, use `union(enum)` with payloads — not a bare `enum`. Callers need the *reason*, not just the verdict.
- Bare enums are fine for input classification (e.g., `GateDecision = enum { auto_approve, requires_approval, auto_kill }`) where every variant is handled identically. But for return values where callers branch on failure details, carry the context in the union.
- Example: `GateCheckResult = union(enum) { passed: void, blocked: BlockReason, auto_killed: AutoKillTrigger }` — callers can produce specific error messages without re-deriving context.

## Removed Endpoint Stubs (M10_001)

> [JUDGMENT → ARCH]

- When a handler's backing table is dropped, replace it with a 410 stub — do not delete the function unless the route is also removed from the router.
- Use `common.errorResponse(res, .gone, error_codes.ERR_*, "...", req_id)` as the entire body. No arena, no auth, no DB.
- If the route is removed from the router at the same time (pre-production), delete the handler file too and remove all re-exports from `handler.zig`.
- When removing a handler file, also remove it from `m5_handler_changes_test.zig` (or equivalent import-resolution test) — stale `@import` will break the build.

## Comptime Eval Quota + Package Boundary (M11_001)

> [DETERMINISTIC → XCOMPILE]

- Comptime loops over large tables (e.g. 130 codes × 131 TABLE entries × `std.mem.eql`) need `@setEvalBranchQuota(N)` as the first line. Default is 1000. Formula: `N ≈ code_count × table_size × avg_string_len`. Round to next power-of-ten; comment the math.
- `@embedFile` is sandboxed to `src/`. Any path escaping it (`../../public/openapi.json`) is a compile error. For files outside `src/`, write a Python/shell validator invoked via a `make` target wired into `lint-zig`.

## Sentinel Values Must Not Collide With Real Registry Codes (M11_001)

> [DETERMINISTIC → TODO-CHECK]

- In any code registry with a fallback sentinel (e.g. `UNKNOWN_ENTRY` in `error_table.zig`), the sentinel's `.code` must NOT match any real registered entry. Use a visually distinct value like `"UZ-UNKNOWN"`. Collision causes tests to pass with wrong semantics and the comptime coverage gate to fail. Add a test that verifies the sentinel is absent from TABLE.

## Module Split Pattern (M4_001)

> [JUDGMENT → ARCH]

- When a module hits the line limit, split by concern — not arbitrarily. Preferred extraction order:
  1. Types + parsing → `foo_types.zig` or `foo_config.zig` (re-exported by `foo.zig`)
  2. Tests → `foo_test.zig` (imported via `test { _ = @import("foo_test.zig"); }`)
  3. Integration with other modules → `foo_integration.zig` (thin adapter)
- The original module remains the public API. Extracted modules are implementation details imported only by the parent.
- Do not split into `foo_part1.zig` / `foo_part2.zig` — names must describe the concern, not the split order.

## Struct Init Partial Leak (M6_001)

> [DETERMINISTIC → DEINIT]

- Never build a struct literal with multiple `try dupeJsonStr()` calls in a single expression. If a later field's dupe fails, the already-duped fields leak.
- Build field-by-field with `errdefer alloc.free(field)` after each dupe. Only assemble the struct after all fields are successfully allocated.
- The `errdefer` chain unwinds in reverse order, freeing exactly what was allocated.

## Stack Buffer Return Safety (M6_001)

> [DETERMINISTIC → DEINIT]

- Never return a `[]const u8` slice that points into a stack-allocated buffer (`var buf: [N]u8`). The stack frame is deallocated when the function returns. The caller reads garbage.
- If you need to return a substring from a stack buffer: either `alloc.dupe()` it, or remove the field from the return type.
- This applies to any function-local array used as a normalization/scratch buffer.

## Multi-Step Init: errdefer Chain Pattern (bvisor)

> [DETERMINISTIC → DEINIT]

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

> [JUDGMENT → TGU]

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

> [DETERMINISTIC → DRAIN]

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

## SQL Statement Modules

> [DETERMINISTIC → SQLMOD]

New production Zig modules must not define SQL statement text inline. Put query
text in a domain-local `sql.zig` and import it from the state/handler/service
module. This mirrors `src/agentsfleetd/fleet_bundle/sql.zig`: table names and
query statements stay grepable in one place, while the public module owns row
mapping, allocator ownership, and error translation.

Rules:
- `sql.zig` owns SQL statement constants (`SELECT_*`, `INSERT_*`, `UPDATE_*`,
  `DELETE_*`, `*_SQL`, `*_QUERY`) and multiline SQL bodies.
- The parent module imports the SQL module and calls `conn.query(sql.NAME, ...)`
  or `conn.exec(sql.NAME, ...)`.
- Tests may keep setup/teardown SQL inline; fixture readability matters there.
- Existing production modules with inline SQL are legacy shape until touched for
  extraction, but any new production module with inline SQL fails the dispatch.

## Build Verification: `make`, Not `zig build`

> [DETERMINISTIC → XCOMPILE]

- Verification commands are defined in `make` targets and CI runs `make`. Use `make test` (tier 1) and `make test-integration` (tier 2) for verification — **not** `zig build test` standalone. `zig build test` runs only the Zig unit set and silently skips the website / app / agentsfleet unit tests + cross-language gates that the verification gate is defined against. Use `zig build` directly only for compilation, never for "did my change pass tests".
- `make memleak` is required when the diff touches server lifecycle, allocator wiring, or cross-thread heap ownership. The macOS `leaks` tool prints a "not debuggable" line under System Integrity Protection — that is expected; the authoritative signal is the allocator-leak phase across `std.testing.allocator`-wrapped tests.

## Doc-Comments and Inline Comments

> [JUDGMENT → NEW:DOC]

- Use `//!` for module-level documentation at the top of a file. Reserve `///` for `pub` type, function, and field doc-comments. Reserve `//` for inline rationale.
- Comments above fields explain *why* the field exists, not what its type is. Group related fields with `// === Section ===` separators.
- Skip comments when a well-named identifier already explains intent. Add a comment only when removing it would confuse a future reader (hidden invariant, surprising workaround, non-obvious lifetime contract).
- **Production `.zig` stays comment-sparse**: at most one short line per `pub` symbol (often none), no multi-line rationale blocks, struct fields uncommented unless they encode a non-obvious invariant. The 350-line FLL cap is hard, and comment bloat is the first thing that pushes a file over it. Move rationale and usage narrative to the spec or the sibling `_test.zig`.
- `_test.zig` files are the opposite — FLL-exempt, so commentary, setup explanation, and section markers are welcome there. When extracting tests, let the test file carry the longer explanations the source file can't afford.

## Concurrency

> [JUDGMENT → NEW:CONC]

- Protect critical sections with `mutex.lock(); defer mutex.unlock();` in a bare block. Never wrap in `try` or error handling — `.lock()` blocks until acquired, it does not error. Use `.tryLock()` only on non-blocking fast paths.
- Atomic loads and stores default to `.acq_rel`. Weaker orderings (`.acquire`, `.release`, `.monotonic`, `.unordered`) require an inline `// safe because: <reason>` comment so a reviewer can audit at a glance. The acquire-release pair on a single coordination flag is a fine pattern — but the comment is mandatory.
- When two threads coordinate via a flag, document which side does the `.release` store and which does the `.acquire` load. The comment is the synchronization contract.

## Allocator Ownership in Structs

> [DETERMINISTIC → DEINIT]

Extends "Type Design Rules". Two patterns, both legitimate; pick deliberately and document the choice on the type:

- **Stored allocator (default)** — any struct that owns heap memory (`ArrayList`, `HashMap`, duped strings) stores `alloc: std.mem.Allocator` as a field. `deinit(self: *Self) void` then frees without a separate parameter. Default to this pattern.
- **Caller-owned allocator** — short-lived value types may take the allocator at `deinit(self: *Self, alloc: std.mem.Allocator)` instead of storing it. Use this for transient decode results, parser outputs, or anything passed by value through a `defer ... deinit(alloc)` pair. When used, document it on the type's doc-comment: `/// Caller passes the same allocator to deinit that <fn> received.`

## Identifier Conventions

> [DETERMINISTIC → TODO-CHECK]

- snake_case throughout: file names, fields, functions, locals, constants. agentsfleet has no JS-interop boundary — there is no carve-out for camelCase fields. (Bun mixes the two for JS-mapped fields; we do not.)

## RULE UFS — Named constants for repeated and semantic literals

> [DETERMINISTIC → UFS]

_`UFS` is a universal rule **enforced once by `write_any`** (which fires for every source file); this façade carries its prose for the author but does not re-run `audit-ufs` — no redundant full-tree scan (`DISPATCH_ARCHITECTURE.md` §16, Decision 6)._

Mirrors the write_ts_adhere_bun.md §2 clause; both runtimes follow the same rule.

- **String literals used in ≥2 sites become a named const or an enum.** Wire-format values (mode strings, charge types, posture labels) MUST live in an enum (`pub const Mode = enum { platform, self_managed }`) or a `pub const X: []const u8 = "…";` and be referenced by name everywhere else — including test fixtures and `std.mem.eql` arguments at parse boundaries.
- **Numeric literals carrying semantic meaning become a named const** even at the first use site. Conversion factors (`NANOS_PER_USD`, `NANOS_PER_SECOND`, `BYTES_PER_MIB`), thresholds (`LOW_BALANCE_THRESHOLD_NANOS`, `MAX_RETRY_COUNT`), sub-cent rates, and time scaling factors all need names. `1_000_000_000` in a `wall_seconds * 1_000_000_000` expression is a smell — `wall_seconds * NANOS_PER_SECOND` is the rule. Carve-out: **pin tests** where the literal IS the contract keep the literal with a `// pin test: literal is the contract` comment.
- **Cross-runtime constants are named identically.** `NANOS_PER_USD` in Zig matches `NANOS_PER_USD` in TS and `NANOS_PER_USD` in JS — never `NANOS_PER_DOLLAR` in one and `NANOS_PER_USD` in another. When a constant exists in a sibling runtime, the Zig version reuses the name verbatim (snake_case → SCREAMING_SNAKE is fine; semantic name stays identical).
- **CONFORM self-audit.** Before declaring done, grep the diff for repeated string literals and bare-numeric divisors/multipliers; every hit either becomes a named const, joins an enum, or earns the `// pin test` carve-out comment. Spec dimensions that introduce a new wire-format value, conversion factor, or threshold must define the constant in PLAN, not EXECUTE.

## Comptime-Gated Assertions

> [DETERMINISTIC → TODO-CHECK]

- For runtime invariants that should be free in release builds, use `std.debug.assert(condition)` — debug-on, release-off automatically. Use this for capacity bounds, monotonic counter checks, and "this can't happen unless we have a bug" assertions.
- For comptime-evaluable invariants on type shape (struct sizes, array lengths matching enum variant counts), use `comptime { std.debug.assert(@sizeOf(T) == N); }` adjacent to the type definition — see "Progressive Cleanup" above.

## Single-Type-Module Pattern

> [DETERMINISTIC → PUB]

**Binding — applies to new files AND to any existing single-primary-type file the moment an edit touches it, regardless of touch size.** When a `*.zig` file's primary purpose is exactly one struct, use the file-as-struct layout (`const Foo = @This();`). If your edit lands on an existing conventional single-primary-type file — even a one-line bug fix — the same diff that lands the change rearchitects the file to file-as-struct shape. The Pub Surface & Struct-Shape Gate (in `AGENTS.md`) requires the layout choice and any rearchitect plan to be declared before saving. Multi-type modules (tagged unions + helpers, protocols with multiple shapes, handler-collection files) keep conventional layout.

The file-as-struct layout eliminates a level of nesting:

```zig
//! Module-level doc comment.

const HashedString = @This();   // file IS the type

ptr: [*]const u8,               // fields next
len: u32,
hash: u32,

pub const empty = HashedString{ ... };

pub fn init(buf: []const u8) HashedString { ... }
pub fn eql(self: HashedString, other: anytype) bool { ... }

const std = @import("std");      // imports at file END
```

Multi-type modules (e.g. a protocol with `MessageType` + `Envelope` + `Decoded`) keep the conventional struct-inside-file layout — file-as-struct only fits when there is exactly one primary type.

**Directory of file-as-struct types → a facade module (the `std.net` pattern).** When a feature is several cohesive `@This()` types in their own directory (`foo/Bar.zig`, `foo/Baz.zig`), add a one-line-per-type facade `foo/foo.zig` that re-exports them — `pub const Bar = @import("Bar.zig");` — and have callers import the facade, not the leaf files. Mirrors `std.net` (re-exports `Address`/`Stream`/`Server`) and `std.Build`. Stateless helper collections in the same directory stay plain function namespaces (the `std.mem`/`std.fmt` shape), **not** `@This()` structs — `@This()` is for types that own state, function namespaces for stateless builders.

**Reuse a std type before inventing one.** A field that is an IP address is `std.Io.net.IpAddress` (0.16), not a hand-rolled `[4]u8`; a socket handle is a thin wrapper that owns the fd (the `std.Io.net.Stream` shape), not a bare `fd: i32` with ad-hoc methods; a byte accumulator is the BUFFER GATE's `std.ArrayList(u8)`/`StringBuilder`, not a custom growable. Inventing a parallel type fragments the surface and forfeits std's `format`/`parse`/`eql` for free — only roll your own when no std type fits, and say why in the Pub Surface gate block.

**Compound type names follow Zig std: split the words, TitleCase each.** A file-as-struct type that is two words is `HostName`, not `Hostname` (`std/Io/net/HostName.zig` is literally `const HostName = @This();`); `AllowList`, not `Allowlist`; mirror `FixedBufferAllocator` / `HeadParser` / `ErrorBundle` — std has **zero** single-word-joined compound type. (Functions stay camelCase, fields/vars/constants snake_case, and a non-type module file — a function namespace, not a `@This()` type — keeps a snake_case name: `mem.zig`, `array_list.zig`.)

## Bun-Inspired Conventions (apply on new code, do not retrofit blindly)

> [JUDGMENT → TGU]

These are recommended patterns surfaced from the Bun Zig codebase audit. They improve readability and eliminate boilerplate but do not need to be retrofitted into existing modules in a single sweep — apply when touching the file for other reasons.

- **`inline fn` for ≤3-line getters and trivial conversions.** Field extractors, enum-to-string converters, comptime-evaluable switches — these benefit from inlining and the compiler does the right thing. Reserve uninlined `fn` for multi-line algorithms or anything with a loop body. (Bun example: `sys/Error.zig:62-68`.)
- **`@This()` in nested or self-referential contexts.** Inside a struct definition, prefer `pub fn save(self: *@This())` over `pub fn save(self: *MyStruct)` — easier to refactor (struct rename doesn't cascade) and clearer in nested types.
- **Free functions before structs that use them.** Within a file, top-level constants → free helper functions → primary struct(s) that consume them → tests. Reading top-down then matches data flow. Section comments (`// === Section ===`) demarcate when a file has more than one logical region.
- **Strategy tagged-union over a "kind + behavior" registry.** When N variants mostly share one path and a few are bespoke, model the *strategy* as a `union(enum)`: declarative-data variants for the common cases + a `custom: *const fn(Ctx) anyerror!Out` escape hatch, and let the union own its dispatch (`fn run(self, ctx)` / `fn isX(self)`) so callers never switch on an id. Collapses an `on_demand: bool` + `fn` pair into one exhaustively-checked type. (Bun: `SideEffects` in `resolver/package_json.zig`, `AllowUnresolved` in `options.zig`.) Pairs with "Tagged Unions for Result Types" below — that rule is for *outcomes*, this one is for *behaviors*.
- **Comptime registry + comptime validation.** A declarative `[]const Spec` table with a `comptime {}` block asserting no duplicate ids / full enum coverage; for hot name→entry lookups at scale, `ComptimeStringMap` (Bun `comptime_string_map.zig` — length-bucketed, benchmarked faster than a switch) over a linear scan. Adding an entry is one data line; dispatch stays data, never a new branch.
- **Don't abstract at N=1.** Add a strategy variant or generalize a registry only when ≥2 real callers validate the shape. Until then the bespoke `custom`/one-off IS the honest abstraction — a speculative generic with a single (or zero) caller is untested dead code (RULE NDC). Bun's `SideEffects` grew variants over time, not up front.
- **Pure core + injected effects.** Pass I/O (http, clock, signer, secret loader) into a decision core as a `Ctx`/deps struct instead of hardcoding it, so the core is unit-testable with no DB/network and the effects are swappable in tests. (Bun: s3 `SignOptions → SignResult`, a pure signing function.) Same instinct as the caller-allocator default — dependencies are arguments, not globals.
- **Do NOT copy Bun's `anyopaque` / manual-vtable erasure at a type-identity or security boundary.** Bun uses it for JS interop; we have a module-boundary type-identity invariant and a sandbox boundary that the compiler's type check is load-bearing for. Keep the tagged union / named module — borrow Bun's idioms, not its type erasure.

## Module Boundaries & Shared Modules (`src/lib`)

> [JUDGMENT → ARCH]

**The boundary rule (Zig-enforced):** a module's relative `@import("…")` may only reach files **within the directory subtree of that module's root source file**. Reaching across with `../..` into a sibling tree fails to compile — `error: import of file outside module path`. A binary's module root is the directory of its `root_source_file` in `build.zig` (a binary rooted at `src/main.zig` can reach all of `src/`; one rooted at `src/runner/main.zig` is confined to `src/runner/`).

**Why it exists — and why it is a feature, not friction:**

1. **One file = one module = one type identity.** A file reached by relative path belongs to the importing module. If the *same* file were reachable from two modules by path, it would compile twice into two distinct types that look identical but never compare equal (the "expected `Foo`, found `Foo`" split). Confining relative imports to a module's own tree forces a shared file to be reached as a **named module**, which the build dedups to one instance — the type is defined exactly once.
2. **`build.zig` is the single source of truth for the dependency graph.** Every cross-module edge is a declared `.imports` entry, never a smuggled `../..`. Incremental compilation, implementation-swapping (e.g. a stub dependency in tests), and dependency analysis all depend on this.
3. **Auditable, minimal surface per binary.** A binary's *entire* external dependency surface is its module's `.imports` list. "This binary cannot link X" becomes structural — read the `.imports` and you see everything it can touch. A binary cannot reach a dependency that is not declared there, no matter what sits elsewhere in `src/`.
4. **Relocatable / cacheable / sandboxed modules.** A module confined to its tree (plus declared deps) can be moved, vendored, or published; a dependency you pull cannot reach up into your files.

**`src/lib/` — the home for our own shared modules.** Code reused across **≥2 build graphs** (separate `build*.zig` files producing separate binaries) lives under `src/lib/<name>/` and is consumed as a **named module**, never by relative `../` reach-across:

- Declare it in every `build*.zig` that needs it (`b.createModule({ .root_source_file = b.path("src/lib/<name>/<name>.zig") })`), reusing the *same* module object across the `.imports` within one build graph so type identity holds.
- Import it by name everywhere: `const x = @import("<name>");` — never `@import("../../lib/<name>/…")`.
- External packages (those from `build.zig.zon`) are **not** `src/lib/` — they are already named modules; do not relocate them into `src/`.
- A wire contract shared between a server binary and a client/daemon binary is the canonical inhabitant: putting it in `src/lib/` lets both build graphs compile it without either reaching into the other's tree.

**Adding to `src/lib/` is gated (reason-then-approval).** Promoting a module into `src/lib/` widens the shared surface, so it is not an agent-unilateral move. Before creating a new `src/lib/<name>/`: (1) state the reason — which ≥2 build graphs consume it, and why it must be shared rather than duplicated or kept domain-local; (2) propose it to the owner with that reason; (3) create it only on approval. Helpers shared *within a single binary* stay under that domain's `common/` (e.g. `src/<domain>/common/`), not `src/lib/`.

## Allocator model — ghostty-derived, rules A1–A6

> [container]

Mined from ghostty (`~/Projects/oss/ghostty/src/`) in the M126 adversarial review and
directed by Indy into six citable rules. Each rule codifies a ghostty (or in-repo) exemplar —
name the exemplar when you apply the rule (RULE GRD). These are the rules an editing agent
breaks by *omission*: the daemon's liveness sweeper leaked precisely because it diverged from
the A2 ladder its two sibling sweepers followed.

### A1 — Backing allocator chosen once in `main`, threaded as a parameter

> [JUDGMENT → ARCH]

One backing allocator is selected at process start and passed down as a parameter; Zig code
paths never reach a global to allocate. Debug/test builds pick the leak-checking General
Purpose Allocator (GPA); release picks the C allocator. Global state never holds the allocator
for a Zig path.
**Exemplar:** ghostty `global.zig:74-96` (allocator selection at `main`, Valgrind detected at
runtime). In-repo: `agentsfleetd/main.zig` and `runner/daemon/worker_pool.zig` own the GPA;
everything below takes `alloc` as an argument.

### A2 — errdefer ladder: one errdefer immediately after each acquisition

> [DETERMINISTIC → DEINIT]

Every init that acquires more than one resource places an `errdefer` directly after each
fallible acquisition; block-scoped `errdefer`s collapse to one composite `errdefer` after an
ownership handoff; `errdefer comptime unreachable` after the last fallible op makes the commit
region compiler-proven atomic. Never batch errdefers at the bottom, and never build a struct
literal with multiple `try dupe*()` calls (an earlier field leaks when a later one fails).
**Exemplar:** ghostty `PageList.MemoryPool.init:85-102` (per-acquisition ladder),
`Surface.zig:616-664` (block-scoped composite handoff). In-repo: `reclaim_sweeper` /
`approval_gate_sweeper` are the correct shape; the M126_001 fix brought `liveness_sweeper` back
onto the ladder. The `deinit-pairs` audit enforces the pairing mechanically.

### A3 — Leaf structures unmanaged; only lifecycle roots store an allocator

> [JUDGMENT → TGU]

Leaf structures take `alloc` per call and store nothing; only lifecycle roots (servers, pools,
long-lived caches) keep an `alloc` field. A leaf that stores an allocator it does not own is a
lifetime bug waiting to fire. Pairs with "Allocator Ownership in Structs" below — that rule is
the two-pattern menu; this one says a *leaf* gets the caller-owned pattern.
**Exemplar:** ghostty leaf structs (`Tabstops`, decode results) take `alloc` per call while
roots (`Surface`, `App`) hold it.

### A4 — Arena as the ownership unit

> [JUDGMENT → ARCH]

An arena is the ownership unit for a config-shaped object or a transient operation: one
`_arena` field per config/request object (deinit is a single call), a scratch arena per
transient op, and arena-in-message when a payload crosses a thread boundary. Reload is
build-new / replay / swap / deinit-old; recover the parent via `arena.child_allocator`. Do not
use an arena to mask a missing free in a reusable helper (Allocator Choice Gate).
**Exemplar:** ghostty `Config.zig:3757-3815` (`_arena` per config), `Surface.zig:1405-1418` →
`renderer/generic.zig:652,803` (arena-in-message cross-thread transfer).

### A5 — Ownership stated in fixed phrases; `self.* = undefined` in every deinit

> [DETERMINISTIC → TODO-CHECK]

Every allocating public fn states ownership in one of the fixed phrases **"caller must free"**
or **"takes ownership"** — grepable, uniform, no synonyms. Every `deinit` poisons with
`self.* = undefined` after freeing, so a use-after-deinit traps instead of reading stale
fields. Callers own their arguments; a callee clones only what it keeps. The phrase and poison
checks are mechanized by the roster-scoped repo lint (blocking inside the discipline roster,
advisory outside) in `lint-zig.py`.
**Exemplar:** ghostty ownership phrases on every allocating pub fn (`App.zig:135-137`,
caller-owns-arguments), `self.* = undefined` poisoning in every deinit.

### A6 — Multi-step init carries tripwire fail points + a loop-all-failpoints test

> [DETERMINISTIC → TODO-CHECK]

Every multi-step init in the discipline roster carries comptime-erased tripwire fail points
(zero production cost) and a test that loops `for (std.meta.tags(FailPoint))` injecting
`error.OutOfMemory` under `std.testing.allocator`, asserting the errdefer chain freed
everything **and** that state rolled back. This is the failure-injection shape ghostty uses in
place of `checkAllAllocationFailures`.
**Exemplar:** ghostty `src/tripwire.zig` (~290 lines, `enabled = builtin.is_test`, inline
call convention), `Tabstops.zig:255-271` + `PageList.zig:5503-5527` (state-rollback asserts).
In-repo: `src/lib/tripwire/tripwire.zig` (vendored in M126_001).

## Concurrency discipline — ghostty-derived, rules C1–C5

> [container]

The five concurrency rules the review found agentsfleet held only by implicit convention while
ghostty holds them structurally. Name the exemplar when you apply the rule (RULE GRD); the
durable model is `docs/architecture/concurrency.md` in the product repo.

### C1 — Cross-thread channels are Single-Producer Single-Consumer; the receiver frees

> [JUDGMENT → NEW:CONC]

A channel that crosses a thread boundary declares its single producer and single consumer
(Single-Producer Single-Consumer, SPSC). Payloads carry their own allocator; the receiver
frees, in a `defer` at the top of the handler. Pin `@sizeOf(Message)` with a test. C1 binds
*new* channels — migrating an existing channel is a separate judgment with the architecture
doc as input.
**Exemplar:** ghostty `datastruct/blocking_queue.zig` (SPSC + wakeup handle),
`datastruct/message_data.zig` + `termio/message.zig:110-113` (receiver-frees, size pinned).

### C2 — Shutdown is stop-signal → join → deinit, never free-on-timeout

> [JUDGMENT → NEW:CONC]

Shutdown signals stop, joins the worker, and only then deinits shared state — a dying consumer
keeps draining until ordered to stop, and nothing shared is freed while either thread can still
touch it. A bounded drain that *times out* must not then free state a straggler still reads
(the class of bug M126_001 fixed in the streaming teardown). Use `common.Event` (Zig 0.16
removed `std.Thread.ResetEvent`) for the deterministic stop→join handshake in tests.
**Exemplar:** ghostty `Surface.zig:772-798` (stop→join→deinit), `termio/Thread.zig:226-233`
(drain-until-stop). In-repo: `cmd/serve_shutdown.zig` (M126_001 ordering fix).

### C3 — No blocking push/write while holding a lock the consumer needs

> [JUDGMENT → NEW:CONC]

Never do a blocking push or socket write while holding a lock the consumer needs to make
progress: try instantly, else notify + unlock + block + relock. Lock state is an explicit
parameter, not an ambient assumption. A blocking Transport Layer Security (TLS) write held
under a hub mutex pins the reader thread and every subscribe until Transmission Control
Protocol keepalive kills the connection — a watchdog-bounded send under a dedicated wire lock
is the fix.
**Exemplar:** ghostty `termio/mailbox.zig:61-93`, `Termio.zig:400-410`
(instant-try-else-notify, `MutexState` enum parameter). In-repo: `events/subscription_hub.zig`
wire discipline (M126_001).

### C4 — One documented mutex per shared aggregate, stating exactly what it protects

> [JUDGMENT → NEW:CONC]

Each shared aggregate has exactly one mutex with a doc comment naming precisely what it
protects and any ordering constraint; `lock(); defer unlock();` adjacent. Copy out into an
arena inside the tightest critical section, then compute unlocked. Every mutex in the
discipline roster carries this invariant comment — the repo lint counts declarations against
documented invariants.
**Exemplar:** ghostty `renderer/State.zig:10-14` (mutex + explicit "protects" invariant).

### C5 — Thread-confined state is the default, marked as such

> [JUDGMENT → NEW:CONC]

State touched by exactly one thread needs no lock — but say so: a `// only touched by thread X`
comment on the field, and a `*Locked` suffix on any fn that must be entered with the lock held.
Silence forces the next editor to re-derive the confinement or add a redundant lock.
**Exemplar:** ghostty `Termio.zig:759-764` (thread-confined comment), `*Locked` fn-name-suffix
convention.

## Merged from dissolved gate cards

> [container]

Prose merged verbatim from the Zig-relevant `docs/gates/*.md` cards (zig, pub-surface, lifecycle). Each subsection carries its own enforcement tag.

### Test files are in scope for the memory-safety discipline

> [DETERMINISTIC → DEINIT]

Tests are in scope — drain/errdefer/ownership rules apply equally.

### File-shape tie-break: behavior-bound-to-state vs operations-over-value

> [JUDGMENT → FSD]

**Tie-break:** behavior-bound-to-state → file-as-struct; operations-over-value → conventional. **Escape clause:** "I can articulate in one sentence why this is operations-over-value." If you can't, file-as-struct wins.

### FILE SHAPE DECISION is a PLAN-time obligation, not EXECUTE

> [JUDGMENT → FSD]

The file-shape verdict (file-as-struct vs conventional) is decided at PLAN, before the first Write/Edit creating or reshaping the file — not chosen by inertia at write time. A `conventional` verdict requires a one-sentence "why not file-as-struct." Skipping the decision is a PLAN violation, not just an EXECUTE one.

### No inheritance of the shape verdict across symbols

> [JUDGMENT → FSD]

Each new pub surface needs its own shape verdict — cloning a sibling's "Public for the integration test in …" justification does NOT discharge it. The design call does not transfer between symbols; own the verdict per surface, do not inherit a sibling's justification.

### PUB GATE firing conditions, pre-edit grep, and override

> [DETERMINISTIC → PUB]

**Threshold-cross — the gate fires on an existing file when:** (a) an Edit adds the file's first `pub` type, OR (b) adds a `pub fn ... self ...` method to a file currently dominated by `pub` free fns, OR (c) removes the last pub free fn from a multi-pub-fn file. It also fires on any new `*.zig` under `src/`, and on a user message saying "rethink the layout of `<file>`". For all other `*.zig` edits the block is skipped with a one-liner `PUB GATE: skipped — <one-line reason>` — never produce a `pub` change without either the full block OR the skip warning preceding the edit.

**Pre-edit grep (new bytes only — any match fires the gate):** `^pub` (new top-level pub declaration), `^\s+pub fn` (new pub method on an existing struct), `ErrorUnion{… NewVariant,` (new variant on a pub error union), `= enum { … NewVariant,` (new variant on a pub enum). Mechanical consumer-grep is delegated to `zlint`'s `unused-decls: error`.

**Override:** `PUB GATE: SKIPPED per user override (reason: ...)` immediately preceding the edit (sub-gate override). The umbrella `FILE SHAPE DECISION` override requires the user's explicit ask **this turn** — auto-mode does NOT cover it.

### Carve-out: PUB GATE vs LIFECYCLE GATE

> [DETERMINISTIC → DEINIT]

PUB GATE and LIFECYCLE GATE answer different questions and neither defers to the other. PUB GATE asks: should this symbol be `pub`, and does the file shape justify it? LIFECYCLE GATE asks: if `init` exists, does `deinit`? Is `errdefer` placed correctly? Is allocator ownership clear? Both gates may fire on the same `pub fn init` edit — print both verdicts; the pub-surface decision never satisfies the lifecycle-pairing decision and vice versa.

### `defer` + `errdefer` on the same allocation — forbidden

> [DETERMINISTIC → DEINIT]

`defer` and `errdefer` on the *same* allocation is forbidden — pick one per `LIFECYCLE_PATTERNS.md` §6. (`errdefer` cleans up only on the error path; `defer` always cleans up. Both on one allocation double-frees on the error path, or frees a value the caller still owns on success.) The pairing audit surfaces this as `defer-mix:<ok|conflict>`.

### Lifecycle-method recognition: renames do not bypass

> [DETERMINISTIC → DEINIT]

The init/deinit pairing audit treats all of `deinit`, `close`, `release`, `destroy`, `shutdown`, `dispose`, `free` as lifecycle (cleanup) methods. Renaming `deinit` to `close` or `release` does NOT bypass the pairing requirement — a struct owning heap memory or an opaque handle still needs an `init`-paired cleanup method under any of those names.

### Empty-pair and arena-leakage informational flags

> [DETERMINISTIC → DEINIT]

Two non-blocking informational flags the pairing audit surfaces: (1) `init` body empty AND `deinit` body empty — likely a pair-for-shape that isn't actually needed; (2) an arena-allocated slice stored in a long-lived struct — the reviewer must either restructure the ownership or explicitly acknowledge the arena-lifetime mismatch. Neither blocks mechanically; both demand a reviewer decision rather than silent acceptance.

### Scope (M70): full-tree audit, staged content satisfies the same hook run

> [DETERMINISTIC → DEINIT]

`deinit-pairs.sh` walks the full `src/` working tree via `git ls-files`. The index includes staged-but-not-yet-committed content, so a fix staged in pre-commit satisfies the check on the same hook run. `--staged` is preserved as an opt-in narrowing mode for iterative dev.

### Deinit idempotency assertion (reviewer-owned)

> [JUDGMENT → DIDEM]

Every type with a cleanup contract should carry a test proving its cleanup method is idempotent (or its single-shot ownership is asserted) — the mechanical audit reports idempotency-test:<present|missing> but does not block on it; presence/absence is a reviewer responsibility, not a machine pass/fail. A struct whose `deinit` frees fields must have a test that exercises the success-cleanup path so the leak detector fires on a missed or double free.
