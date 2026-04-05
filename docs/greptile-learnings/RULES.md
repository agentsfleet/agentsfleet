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
