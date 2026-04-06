# Code Rules — Learned from Review

Rules derived from greptile reviews, PR feedback, and production incidents.
Each rule has: the rule, why it exists, and what to do instead.

**When to read this file:**
- At the start of EXECUTE phase (before writing code)
- When `/review` skill is invoked (before reviewing code)
- When fixing greptile/review feedback

**When to ignore a rule:** Only when the user explicitly overrides it for a specific case with a stated reason. Never silently skip.

---

## Parsing & Data Extraction

**RULE: Use the language's standard JSON/XML/YAML parser. Never scan raw bytes for structured data.**

Why: String scanning (`indexOf`, `grep`, regex) on structured data is injection-prone. A value containing the search pattern causes misparsing. Escape-checking hacks just move the bug.

Do:
```zig
const parsed = std.json.parseFromSlice(std.json.Value, alloc, data, .{}) catch return fallback;
defer parsed.deinit();
const val = parsed.value.object.get("field") orelse return fallback;
```

Don't:
```zig
const pos = std.mem.indexOf(u8, data, "\"field\":") orelse return fallback;
```

Incident: M22_001 greptile P2 — `extractCreatedAt` used `std.mem.indexOf` to find `"created_at":` in JSON. A crafted `gate_name` containing that string could cause misparsing.

---

## JavaScript — Reference vs Value

**RULE: Never pass a mutable boolean/number to a function expecting to observe later changes. Use an object, closure, or the AbortController pattern.**

Why: JavaScript passes primitives by value. The called function gets a frozen snapshot, not a live reference. If the outer scope changes the value after the call, the function never sees the update.

Do:
```javascript
// Use AbortController — its .signal is an object reference
const ac = new AbortController();
process.on("SIGINT", () => ac.abort());
// In the called function, check: err.name === "AbortError"
```

Don't:
```javascript
let aborted = false;
process.on("SIGINT", () => { aborted = true; });
readStream(response, aborted); // aborted is frozen at false inside readStream
```

Incident: M22_001 greptile P2 — `abortedRef` parameter was stale; only `AbortError` name check saved correctness.

---

## HTTP Error Handling

**RULE: Distinguish retryable from non-retryable HTTP errors. Never treat all non-2xx the same.**

Why: A blanket `return` on any HTTP error means transient 5xx (server overload, deploy in progress) kills the operation permanently. A retry would have succeeded.

Do:
```javascript
if (!response.ok) {
  if (response.status >= 500 && attempt < maxRetries) { retry; continue; }
  return; // 4xx: permanent, don't retry
}
```

Don't:
```javascript
if (!response.ok) { return; } // 503 and 404 treated identically
```

Incident: M22_001 greptile P2 — `streamRunWatch` returned immediately on any non-2xx, including 503.

---

## Zig — Query Results

**RULE: Call `conn.exec()` for writes. Call `q.drain()` before `q.deinit()` on reads. Copy row data before drain.**

Why: `pg.Conn` cannot issue new commands while a result set is open. Forgetting `drain()` causes silent hangs. Row slices point into the result buffer — they become dangling after drain/deinit.

Do:
```zig
var q = try conn.query("SELECT ...", .{});
defer q.deinit();
const val = try alloc.dupe(u8, row.get([]u8, 0) catch "");
q.drain() catch {};
```

Don't:
```zig
var q = try conn.query("SELECT ...", .{});
defer q.deinit();
const val = row.get([]u8, 0) catch ""; // dangling after deinit
// missing drain before deinit
```

Source: ZIG_RULES.md, enforced by `make check-pg-drain`.

---

## Constants & Magic Values

**RULE: If a timeout, retry count, or threshold appears in code, it must be a named constant. If it's used across modules, put it in a shared constants file.**

Why: Magic numbers hide intent and make tuning impossible without reading the implementation. Named constants document the contract.

Do:
```zig
pub const PUBSUB_READ_TIMEOUT_MS: u32 = 25_000;
```

Don't:
```zig
std.posix.setsockopt(fd, ..., 25000);
```

---

## File Size

**RULE: Keep code files under 500 lines. If a file exceeds 500 lines after your changes, split it before proceeding.**

Why: Large files are hard to review, test, and reason about. The 500L limit is enforced in CI for Zig and as a VERIFY gate for all languages.

Split strategy: Extract a cohesive function group into a new module (e.g., `streamRunWatch` → `run_watch.js`).

---

## Security — Input to Agents/LLMs

**RULE: Never concatenate raw user input into agent prompts, tool calls, or structured data that flows to an agent. Use structured message formats.**

Why: Prompt injection via user-controlled fields (gate names, run IDs, error messages) can manipulate agent behavior. Even if the current code doesn't send data to an LLM, data flows change — defensive coding prevents future injection.

Do: Validate, type-check, length-bound all external input. Use parameterized templates.
Don't: `prompt = "Fix this error: " + user_error_message`

---

## Error Handling — Timeout vs Fatal

**RULE: When a function handles both timeout and fatal errors, return null for timeouts and propagate fatal errors. Never swallow all errors into the same return value.**

Why: If both timeout (expected) and connection-reset (fatal) return null, the caller has no way to distinguish them. A tight loop that expects null-on-timeout will busy-loop on fatal errors, hammering downstream systems with no backoff.

Do:
```zig
if (err == error.WouldBlock or err == error.ConnectionTimedOut) return null;
return err; // fatal — propagate so caller can break/fallback
```

Don't:
```zig
// All errors return null — caller's catch-break is dead code
log.warn("read_error err={s}", .{@errorName(err)});
return null;
```

Incident: M22_001 greptile P1 — `readMessage()` returned null for ConnectionResetByPeer, causing a busy-loop in the stream handler.

---

## Zig TLS Transport — Flush Socket Writer After TLS Writer Flush

**RULE: After calling `tls_writer.flush()`, always call `stream_writer.interface.flush()` to actually send the encrypted bytes to the socket.**

Why: Zig 0.15.2's `std.crypto.tls.Client.writer.flush()` encrypts the plaintext and writes ciphertext into the `stream_writer`'s internal buffer — it does NOT flush that buffer to the socket. Without the second flush, Redis never receives the command and `readRespValue` blocks forever with no timeout.

Do:
```zig
try writer.flush();  // TLS encrypt → into stream_writer buffer
if (self.transport == .tls) try self.transport.tls.stream_writer.interface.flush();  // send to socket
```

Don't:
```zig
try writer.flush();  // TLS flush only — data sits in socket writer buffer, never sent
```

Incident: M22_001 — `sendCommand` in `redis_pubsub.zig` was missing the socket writer flush. AUTH and SUBSCRIBE commands were encrypted but never sent, causing `readRespValue` to block indefinitely with no error or timeout.

---

## SSE Heartbeat — Interval Must Be Less Than Socket Timeout

**RULE: Set the SSE heartbeat interval below `SO_RCVTIMEO` so the first socket wakeup fires the heartbeat before the proxy idle timeout.**

Why: With `SO_RCVTIMEO = 25s` and `heartbeat_interval = 30s`, the first wakeup at t=25s has elapsed=25s < 30s → no heartbeat. The second wakeup is at t=50s — after a 30s proxy has already killed the connection. The invariant `heartbeat_interval < SO_RCVTIMEO < proxy_idle_timeout` ensures the first wakeup always emits a heartbeat.

Do:
```zig
// SO_RCVTIMEO = 25s, proxy_idle_timeout = 30s
// heartbeat at t=25s: elapsed=25s ≥ 20s → fires within the proxy window
const heartbeat_interval_ns: u64 = 20 * std.time.ns_per_s;
```

Don't:
```zig
// heartbeat_interval > SO_RCVTIMEO — first heartbeat at t=50s, proxy drops at t=30s
const heartbeat_interval_ns: u64 = 30 * std.time.ns_per_s;
```

Incident: M22_001 greptile P1 — `heartbeat_interval_ns = 30s` with `SO_RCVTIMEO = 25s` meant the first heartbeat fired at ~50s, after the 30s proxy had already dropped the stream.

---

## Streaming — Never Buffer Then Parse

**RULE: If the goal is real-time output, verify the transport delivers bytes incrementally. A buffered HTTP client defeats streaming regardless of how the parser works.**

Why: `client.fetch()` + `response_writer` may buffer the entire response before calling the writer. `client.open()` + `req.reader().read()` delivers bytes as they arrive. Testing the parser with `feedBytes()` only validates parsing, not transport — the bug is invisible to unit tests.

Do: Use `client.open()` + `req.reader().read()` loop for SSE/streaming endpoints.
Don't: Use `client.fetch()` with a response_writer and assume incremental delivery.

Incident: M22_001 greptile P1 — Zig CLI buffered entire SSE response, printing all events at once after run completion.

---

## Lock-Free Hash Maps — Never Read After CAS Failure

**RULE: When a CAS (compare-and-swap) fails in a lock-free data structure, do NOT read the slot's fields. The winning thread may still be writing them. Continue probing or spin on a ready flag.**

Why: TOCTOU race — between losing the CAS and reading the field, the winner thread is still initializing. You read partially-written data (wrong hash, truncated ID, zero-length string).

Do: Use a two-phase init: `occupied` (CAS claim) + `ready` (fields written). Losers continue probing. Readers check `ready.load(.acquire) == 1` before accessing fields.
Don't: Read `slot.hash` or `slot.ws_id` immediately after losing a CAS on `slot.occupied`.

Incident: M28_001 greptile P1 — `resolveSlot` in `metrics_workspace.zig` read slot fields after CAS failure, causing potential duplicate workspace slots and corrupted metric labels.

---

## Migrations — Assert by Embedded Symbol, Not Stale Index Assumption

**RULE: When migration files are inserted or split, update every index-based migration assertion in tests to the new canonical position.**

Why: Index-coupled assertions silently point at the wrong SQL file after a reorder/split. Tests then pass or fail for the wrong reason, hiding schema regressions.

Do:
```zig
// 008_harness_control_plane.sql moved to migrations[6] after 001/002/003 split
try std.testing.expect(std.mem.containsAtLeast(u8, migrations[6].sql, 1, "trust_level   TEXT NOT NULL"));
```

Don't:
```zig
// stale index after migration ordering changed
try std.testing.expect(std.mem.containsAtLeast(u8, migrations[7].sql, 1, "trust_level   TEXT NOT NULL"));
```

Incident: M31_001 greptile P1 — `src/cmd/common.zig` checked `migrations[7]` for symbols that live in `008_harness_control_plane.sql` (`migrations[6]` after the split).

---

## SQL Qualification — Prefer Explicit Schema in Handler Queries

**RULE: New or touched SQL in application handlers must use schema-qualified table names (`core.*`, `billing.*`, etc.) instead of relying on `search_path`.**

Why: Unqualified names depend on session defaults and can resolve differently across tools, tests, or future role configuration changes.

Do:
```sql
SELECT provider FROM core.platform_llm_keys ORDER BY provider;
```

Don't:
```sql
SELECT provider FROM platform_llm_keys ORDER BY provider;
```

Incident: M31_001 greptile P2 — `admin_platform_keys_http.zig` used unqualified `platform_llm_keys`; fixed to `core.platform_llm_keys` and documented in `SCHEMA_CONVENTIONS.md`.

---

## Atomics — Use cmpxchgStrong When Spurious Failure Is Not Acceptable

**RULE: Use `cmpxchgStrong` (not `cmpxchgWeak`) when a failed CAS causes the code to skip a critical side effect. Reserve `cmpxchgWeak` for retry loops and best-effort paths where a spurious miss is harmless.**

Why: `cmpxchgWeak` is permitted to fail spuriously — it may return non-null even when the current value matches the expected value. If the code path after CAS failure skips a critical action (e.g., killing a child process, delivering a message), a spurious failure silently drops that action.

Do:
```zig
// Interrupt delivery — must not be dropped.
if (exit_reason.cmpxchgStrong(running, interrupted, .acq_rel, .acquire) == null) {
    killWithEscalation(child);
}
```

Don't:
```zig
// Spurious failure → interrupt silently dropped, gate keeps running.
if (exit_reason.cmpxchgWeak(running, interrupted, .acq_rel, .acquire) == null) {
    killWithEscalation(child);
}
return; // Falls through on spurious failure.
```

Incident: M21_002 greptile P1 — `cmpxchgWeak` on interrupt detection path could spuriously fail, causing the timer thread to exit without killing the gate child. Interrupt message consumed by GETDEL but never injected.

---

## CLI JSON Contract — Error Codes Must Belong to the Stable Set

**RULE: Every error code emitted in `--json` mode must appear in the stable error code table (§5.4 of the spec). Never introduce ad-hoc codes like `AGENT_ERROR` or `IO_ERROR` that are not in the contract.**

Why: Automation consumers branch on `error.code`. Undocumented codes create silent contract breaks — a consumer that handles the documented 5 codes will silently fail to handle an unknown code, causing brittle or incorrect error handling.

Do:
```js
// Map internal failure categories to stable contract codes
writeError(ctx, "API_ERROR", "agent returned no content", deps);
writeError(ctx, "API_ERROR", `failed to write spec: ${err.message}`, deps);
```

Don't:
```js
writeError(ctx, "AGENT_ERROR", "agent returned no content", deps);  // not in stable table
writeError(ctx, "IO_ERROR", `failed to write spec: ${err.message}`, deps);  // not in stable table
```

Incident: M30_002 greptile P1 — `spec_init.js` emitted `AGENT_ERROR` and `IO_ERROR` which were not in the 5-code stable table. Fixed to `API_ERROR` and covered by contract tests.

---

## CLI JSON Contract — UNKNOWN_COMMAND Messages Must Identify the Token

**RULE: `UNKNOWN_COMMAND` error messages must name the actual unrecognized value, not print usage text.**

Why: Automation consumers need to extract what was unknown to provide actionable error reporting. Usage text as a message is not machine-parseable.

Do:
```js
writeError(ctx, "UNKNOWN_COMMAND", `unknown skill-secret action: ${action ?? "(none)"}`, deps);
writeError(ctx, "UNKNOWN_COMMAND", `unknown harness command: ${group ?? "(none)"}`, deps);
```

Don't:
```js
writeError(ctx, "UNKNOWN_COMMAND", "usage: skill-secret put|delete ...", deps);
writeError(ctx, "UNKNOWN_COMMAND", "usage: harness source put|compile|activate|active", deps);
```

Incident: M30_002 greptile P2 — `core-ops.js`, `admin.js`, `agent_harness.js`, `harness.js` used usage text as the UNKNOWN_COMMAND message instead of identifying the unknown token.

---

## CLI JSON Contract — Dual-Branch jsonMode Guard Must Have a Comment

**RULE: When a command uses `if (ctx.jsonMode) { writeError(...) } else { multi-line prose }`, add a comment explaining why the else branch cannot use `writeError` directly.**

Why: `writeError` already handles jsonMode internally, so the outer guard looks redundant to future readers. Without a comment, they may refactor it away, losing the multi-line human output.

Do:
```js
// non-JSON: preserve multi-line usage text not expressible as a single message
if (ctx.jsonMode) {
  writeError(ctx, "UNKNOWN_COMMAND", `unknown agent subcommand: ${action}`, deps);
} else {
  writeLine(ctx.stderr, ui.err("usage: agent scores ..."));
  writeLine(ctx.stderr, ui.err("       agent profile ..."));
}
```

Don't:
```js
if (ctx.jsonMode) {
  writeError(ctx, "UNKNOWN_COMMAND", `unknown agent subcommand: ${action}`, deps);
} else {
  writeLine(ctx.stderr, ui.err("usage: agent scores ..."));  // looks like redundant guard
}
```

Incident: M30_002 greptile P2 — `agent.js` and `agent_proposals.js` had dual-branch guards without explanation.

---

## Drift-Detection Tests — Use an Independent Schema Snapshot

**RULE: Tests that verify Zig constants match SQL schema defaults must compare against a separate `SchemaSpec` struct, not against inline literals that mirror the constant definition.**

Why: A test like `expectEqual(@as(u32, 3), DEFAULT_RUN_MAX_REPAIR_LOOPS)` is tautological — it asserts `3 == 3` and cannot detect drift. An independent `SchemaSpec` struct holds the raw SQL DEFAULT values; if either the constant or the spec diverges, the test fails.

Do:
```zig
const SchemaSpec = struct { run_max_repair_loops: u32 = 3 };
const schema_spec = SchemaSpec{};
try std.testing.expectEqual(schema_spec.run_max_repair_loops, DEFAULT_RUN_MAX_REPAIR_LOOPS);
```

Don't:
```zig
try std.testing.expectEqual(@as(u32, 3), DEFAULT_RUN_MAX_REPAIR_LOOPS);
```

Incident: M31_002 greptile P2 — `src/types/defaults.zig` had tautological drift tests; replaced with `SchemaSpec` struct as independent source of truth.

---

## Integration Tests — Only Test DB-Reachable Code Paths

**RULE: Integration tests must not insert values that violate real table CHECK constraints via temp tables without constraints. Test only values reachable from the actual schema.**

Why: The real `billing.workspace_entitlements` table has `CHECK (scoring_context_max_tokens >= 512 AND <= 8192)`. Inserting `0` via an unconstrained temp table tests a dead-code branch (`<= 0` fallback) that can never be triggered from real DB data.

Do:
```zig
// Insert 512 — the minimum valid value under the CHECK constraint
// Tests that the clamp boundary is honoured correctly.
VALUES (..., 512, ...);
try std.testing.expectEqual(@as(u32, 512), cfg.scoring_context_max_tokens);
```

Don't:
```zig
// 0 is impossible in production — CHECK constraint prevents it
VALUES (..., 0, ...);
```

Incident: M31_002 greptile P2 — `src/pipeline/scoring_defaults_test.zig` tested the non-positive fallback branch using a value the real schema makes impossible.

---

## Comptime Guards for u64→i64 Casts

**RULE: Whenever a `u64` constant from `src/types/defaults.zig` is cast to `i64` or `i32`, add a `comptime std.debug.assert` at the top of the file to catch overflow if the constant is ever raised.**

Why: The cast `@as(i64, @intCast(DEFAULT_RUN_MAX_TOKENS))` is safe for current values but would panic at runtime if the constant ever exceeded `std.math.maxInt(i64)`. A comptime assert converts that latent runtime panic into a compile-time error.

Do:
```zig
comptime {
    std.debug.assert(defaults.DEFAULT_RUN_MAX_TOKENS <= std.math.maxInt(i64));
}
```

Don't:
```zig
// Silent @intCast with no guard — panics at runtime if constant grows
return used + @as(i64, @intCast(defaults.DEFAULT_RUN_MAX_TOKENS)) > budget;
```

Incident: M31_002 greptile P2 — `src/http/handlers/runs/start_budget.zig` and `src/pipeline/worker_claim.zig` cast `u64` defaults to `i64`/`i32` without comptime guards.

### Gate dispatcher must not glob itself

The `00_gate.sh` dispatcher runs section scripts via glob. The glob pattern must exclude `00_*` to prevent infinite recursion. Use `0[1-9]_*.sh` + `[1-9][0-9]_*.sh` instead of `[0-9][0-9]_*.sh`.

**Do:**
```bash
for script in "$SCRIPT_DIR"/0[1-9]_*.sh "$SCRIPT_DIR"/[1-9][0-9]_*.sh; do
```

**Don't:**
```bash
for script in "$SCRIPT_DIR"/[0-9][0-9]_*.sh; do
```

Incident: PR #162 greptile P1 — `00_gate.sh` glob matched itself, causing fork bomb in CI.
