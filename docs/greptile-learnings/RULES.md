# Code Rules — Learned from Review

Generic principles derived from greptile reviews, PR feedback, and production incidents.
Each rule is a universal principle. Incident references show where it bit us.

**Read this:** at EXECUTE start, during `/review`, when fixing review feedback.
**Ignore a rule:** only when the user explicitly overrides it with a stated reason.

---

## 1. No dead code

Remove unused variables, imports, parameters, and unreachable branches immediately.
Don't leave them for later. Linters and reviewers will flag them, and they mislead readers
about what the code actually depends on.

> M1_001: unused destructured deps in zombie.js; dead `currentObj` branch in simpleYamlParse.
> M30_002: unused variables across CLI commands.

---

## 2. Use standard parsers — never hand-roll

Use the language's built-in or battle-tested parser for structured data (JSON, YAML, TOML, XML).
Never use `indexOf`, regex, or line-by-line string scanning on structured formats.

> M22_001: `extractCreatedAt` used `indexOf` on JSON — injection-prone.
> M1_001: custom `simpleYamlParse` silently dropped all arrays.

---

## 3. One owner per resource — no double cleanup

Every allocation must have exactly one cleanup path. If `errdefer` owns it, don't also
free manually. If a `defer` owns it, don't free in a catch block.

> M1_001: manual `alloc.free()` + `errdefer` on the same pointer = double-free on error path.

---

## 4. Constant-time comparison for secrets

Never use short-circuit equality (`==`, `eql`, `===`) to compare tokens, keys, or passwords.
Use XOR accumulation or a constant-time library function. Length mismatch can short-circuit
(length is not secret), but byte comparison must not.

> M1_001: webhook Bearer token used `std.mem.eql` — timing side-channel.

---

## 5. Distinguish error classes — timeout ≠ fatal ≠ retryable

Never collapse all errors into one return value. Timeouts are expected (return null/retry).
Fatal errors must propagate. Retryable HTTP 5xx must retry; permanent 4xx must not.

> M22_001: `readMessage()` returned null for ConnectionResetByPeer → busy-loop.
> M22_001: `streamRunWatch` treated 503 and 404 identically.

---

## 6. Named constants, schema-qualified SQL

No magic numbers. Timeouts, retry counts, thresholds → named constants.
If used across modules → shared constants file.
SQL in handlers → always schema-qualified (`core.table`, not `table`).

> M31_001: unqualified `platform_llm_keys` in handler query.

---

## 7. Composite keyset cursors for pagination

Cursor-based pagination must encode `(sort_column, id)` — never a bare timestamp.
Multiple rows can share the same millisecond; a scalar cursor silently skips them.

> M1_001: activity_stream cursor was timestamp-only, dropping events at ms boundaries.

---

## 8. Files under 500 lines

Split before proceeding. Extract a cohesive function group into a new module.
Enforced in CI (Zig) and VERIFY gate (all languages).

---

## 9. Cross-compile before commit (Zig)

`zig build -Dtarget=x86_64-linux-musl && zig build -Dtarget=aarch64-linux-musl`.
Always explicit ABI (`-musl`). Never assume a macOS-compiling API exists on Linux.
Never use bare `-Dtarget=x86_64-linux` in CI — LLVM can't parse host kernel versions.

> M22_001: `client.open()` compiled on macOS, missing on Linux. 3 rounds to fix.
> v0.4.0: bare `-Dtarget=x86_64-linux` failed in CI; cache hid the bug in dev.

---

## 10. Flush all layers — drain all results

**TLS:** After `tls_writer.flush()`, also call `stream_writer.interface.flush()`.
TLS flush encrypts into the buffer; socket flush actually sends the bytes.

**Postgres:** `conn.exec()` for writes. `q.drain()` before `q.deinit()` on reads.
Copy row data before drain — slices become dangling. Enforced by `make check-pg-drain`.

**Postgres UUID/JSONB reads:** Cast UUID and JSONB columns to `::text` in SELECT queries.
The `pg` library has no native UUID type — it returns raw binary bytes on Linux but text on
macOS. `::text` forces text format regardless of wire protocol. Fix opportunistically when
touching any query that reads UUID or JSONB columns with `row.get([]const u8, ...)`.

> M22_001: missing socket flush → Redis commands encrypted but never sent → infinite hang.
> M1_001: `claimZombie` read `workspace_id` UUID as binary on Linux CI, text on macOS dev.

---

## 11. Timing invariants must be explicit

When multiple timeouts interact (heartbeat, socket, proxy), the ordering invariant
must be documented and enforced: `heartbeat < socket_timeout < proxy_idle_timeout`.

> M22_001: heartbeat 30s > socket timeout 25s → first heartbeat at t=50s, proxy dropped at t=30s.

---

## 12. Streaming must verify transport, not just parser

If the goal is real-time delivery, test that bytes arrive incrementally at the transport layer.
Unit-testing a parser with `feedBytes()` doesn't prove the HTTP client isn't buffering.

> M22_001: Zig CLI buffered entire SSE response, printing all events at once.

---

## 13. Primitives are pass-by-value in JS

Never pass a mutable `boolean`/`number` to a function expecting to observe later changes.
Use an object, closure, or `AbortController`.

> M22_001: `abortedRef` boolean was frozen at `false` inside the called function.

---

## 14. Lock-free CAS: never read after failure

When a CAS fails, the winning thread may still be writing. Don't read the slot's fields.
Use a two-phase init: `occupied` (CAS claim) + `ready` flag (fields written).

> M28_001: `resolveSlot` read partially-written fields after losing CAS.

---

## 15. Test only reachable values

Integration tests must not insert values that violate real schema CHECK constraints.
Drift-detection tests must compare against an independent schema spec, not inline literals.
Comptime guards must protect narrowing casts (`u64` → `i64`).

> M31_002: tested `0` for a column with `CHECK >= 512`; tautological drift tests.

---

## 16. CLI JSON contract discipline

- Error codes must belong to the stable set — no ad-hoc codes.
- `UNKNOWN_COMMAND` messages must name the unrecognized token, not print usage.
- Dual-branch `jsonMode` guards need a comment explaining why.

> M30_002: undocumented `AGENT_ERROR`/`IO_ERROR` codes; usage text as error message.

---

## 17. Migration index assertions track position

When migration files are inserted or split, update every index-based assertion.
Stale indices silently point at the wrong SQL file.

> M31_001: `migrations[7]` pointed at wrong file after split; should have been `[6]`.

---

## 18. No semicolons in SQL comments

The migration statement splitter splits on `;` but doesn't track `-- line comments`.
A `;` inside a comment (e.g. `-- reads at claim; upserts after`) splits the comment
into two "statements" — the second half is invalid SQL.

> M1_001: `022_core_zombies.sql` and `023_core_zombie_sessions.sql` had `;` in comments,
> breaking the migration runner with `UnexpectedDBMessage`.

---

## 19. Gate dispatcher must not glob itself

`00_gate.sh` glob pattern must exclude `00_*`. Use `0[1-9]_*.sh` + `[1-9][0-9]_*.sh`.

> PR #162: glob matched itself → fork bomb in CI.

---

## 20. No prompt injection from user input

Never concatenate raw user input into agent prompts or tool calls.
Validate, type-check, length-bound all external input. Use parameterized templates.
