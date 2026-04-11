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

## 20. Functions ≤ 50 lines, methods ≤ 70 lines

Keep functions short enough to read without scrolling. If a function exceeds
50 lines (70 for methods with setup/teardown), split it into named helpers.
Each helper should do one thing and be independently testable.

> M1_001: `handleReceiveWebhook` was 120+ lines — 8 steps inlined into one function.

---

## 21. All user-facing strings are constants

Error messages, status values, HTTP header prefixes, Redis key patterns, TTLs —
if a string appears in a response or is compared against input, it must be a named
constant in a shared constants file. No inline string literals for values that cross
a module boundary.

Group by domain: `webhook_constants.zig`, `error_messages.zig`, or add to the
existing `src/errors/codes.zig` for error codes.

> M1_001: `handleReceiveWebhook` had inline `"Bearer "`, `"active"`, `"duplicate"`,
> `"accepted"`, `"webhook_received"`, `86400`, `"UZ-AUTH-001"`.

---

## 22. Error messages follow a standard structure

Every error response uses `error_codes.ERR_*` for the code and a constant for the
human message. Error messages must be:
- Actionable: tell the developer what to do
- Consistent: same structure everywhere
- Constant: defined once, not duplicated as inline strings

Pattern: `common.errorResponse(res, status, error_codes.ERR_*, error_messages.MSG_*, req_id)`

> M1_001: webhook handler mixed `error_codes.ERR_*` constants with inline message strings.

---

## 23. No prompt injection from user input

Never concatenate raw user input into agent prompts or tool calls.
Validate, type-check, length-bound all external input. Use parameterized templates.

---

## 24. Tagged unions over optional-field structs

When a type has mutually-exclusive variants (e.g. trigger types, auth methods),
use a Zig tagged union, not a struct with optional fields. The compiler enforces
exhaustive switches, making invalid states unrepresentable.

> M2_002: ZombieTrigger was a struct with `source: ?[]const u8`, `schedule: ?[]const u8`.
> Webhook-without-source was representable but semantically invalid. Refactored to
> `union(ZombieTriggerType)` with per-variant fields.

---

## 25. Secrets belong in vault, not in entity tables

Never store plaintext secrets (tokens, API keys, webhook secrets) in core entity tables.
Store a vault reference (key_name) in the entity table. Resolve via `crypto_store.load()`
at runtime. This keeps secrets encrypted at rest and out of query results, backups, and logs.

> M2_002: webhook_secret was initially a TEXT column in core.zombies. Refactored to
> webhook_secret_ref (vault key_name). Resolved via crypto_store.load() in the webhook handler.

---

## 26. No static strings in SQL schema

Do not use DEFAULT or CHECK constraints with hardcoded string values in SQL.
Enforce value constraints in application code via named constants (e.g. `ZOMBIE_STATUS_ACTIVE`).
SQL cannot reference Zig/JS constants, so hardcoded strings in schema drift from code.

> M2_002: `DEFAULT 'active'` and `CHECK (status IN ('active', 'paused', 'stopped'))` removed
> from core.zombies. Status enforcement moved to application-level constants.

---

## 27. Escape control characters in JSON string emission

When writing a JSON string encoder, escape all ASCII control characters (0x00-0x1F)
per RFC 8259 section 7. Missing escapes for `\n`, `\r`, `\t`, or null bytes produce
malformed JSON and enable injection if the input contains attacker-influenced content.

> M2_002: writeJsonString in yaml_frontmatter.zig initially only escaped `"` and `\`.
> Review caught that `\n` in a YAML value could inject keys into the JSON output.

---

## 28. Constant-time comparison must not short-circuit on length

When comparing secrets (tokens, webhook secrets), always run the XOR loop over
`@min(a.len, b.len)` bytes, then fold the length mismatch into the result after
the loop. Short-circuiting on `a.len == b.len` leaks the expected secret's length.

> M2_002: constantTimeEq used `a.len == b.len and ct: { ... }` which skipped the
> XOR loop entirely on length mismatch. Fixed to always run the loop.

---

## 29. Use `[]const u8` for immutable data, not `[]u8`

When a struct holds data read from a database or parsed from input that will not be
modified, declare fields as `[]const u8`. Mutable `[]u8` signals the data can be changed,
which misleads readers and prevents the compiler from catching unintended mutation.

> M2_002: ZombieRow used `[]u8` for workspace_id, status, token — all immutable DB data.
> Refactored to `[]const u8` to match semantic ownership.

---

## 30. Cross-layer orphan sweep on every rename, delete, or format change

When you rename a column, delete a function, change a struct, or migrate a file format,
grep for the OLD name/pattern across ALL layers before committing. The layers are:

1. **Schema** (SQL): column names, DEFAULT values, CHECK constraints
2. **Server** (Zig): struct fields, query strings, constants, error messages
3. **CLI** (JS): function calls, imports, template references, output strings
4. **Tests** (Zig + JS): assertions, fixtures, mock data, test helper functions
5. **Docs** (MD): comments, spec references, AGENTS.md, RULES.md, release notes

The sweep command for any renamed symbol `OLD_NAME`:
```bash
grep -rn 'OLD_NAME' src/ schema/ zombiectl/ docs/ AGENTS*.md --include='*.zig' --include='*.js' --include='*.sql' --include='*.md' | grep -v '.zig-cache' | grep -v node_modules
```

A rename is not done until this grep returns zero hits in non-historical files
(completed specs in `docs/v*/done/` and learning docs are exempt — they document history).

> M2_002: webhook_secret renamed to webhook_secret_ref but stale comments still said
> "webhook_secret column." ZombieTrigger changed from struct to union but integration
> test still accessed `.trigger_type`. simpleYamlParse deleted but stale config.zig
> comments still described the old client-parsed flow. Each required a separate fix commit.

---

## 31. CHORE(close) must include orphan verification gate

Before opening a PR, run a mandatory orphan sweep for every symbol that was renamed,
deleted, or changed format in the branch. This is part of CHORE(close), not a separate step.

The verification is:
```bash
# For each deleted/renamed symbol, confirm zero non-historical references:
git diff origin/main --name-only | xargs grep -l 'OLD_PATTERN' 2>/dev/null
# Must return empty for production code. Historical docs are exempt.
```

If the sweep finds hits, fix them before opening the PR. Do not defer orphan cleanup
to a follow-up — the PR that changes the symbol owns the full cleanup.

---

## 32. Test discovery requires explicit import in main.zig

Inline `test` blocks in Zig files are only compiled and run if the file is
reachable from the test root (`main.zig`'s `comptime` test block). A file
can have tests for years that never execute if nobody imports it.

When creating a new file with tests, add `_ = @import("path/to/file.zig");`
to `main.zig`'s test discovery block.

> M2_001: Router tests existed inline since M16 but were never discovered.
> When imported in `main.zig`, two pre-existing test bugs surfaced.

---

## 33. Pointer dereference for anytype query params

When passing a `pg` query result to a function via `anytype` as `&q` (pointer),
the function must use `q.*.next()` and `q.*.drain()`, not `q.next()`.
Direct local variables use `q.next()` without dereference.

> M2_001: `collectActivityPage` received `&q` but called `q.next()`.
> Adding a new caller forced instantiation and exposed the error.

---

## 34. Zig 0.15 ArrayList API

`ArrayList.init(alloc)` does not exist in Zig 0.15. Use `var rows: std.ArrayList(T) = .{};`
and pass the allocator per-operation: `rows.append(alloc, item)`,
`rows.toOwnedSlice(alloc)`, `rows.deinit(alloc)`.

---

## 35. No dead struct fields

If a struct field always holds the same value across all construction sites, remove it.
A field that is never varied is not configuration — it is dead weight that misleads readers
into thinking it can change. Inline the constant at the usage site instead.

> M4_001: `AnomalyRule.behavior` was always `.auto_kill` at every construction site.
> Removed the field; anomaly rules always auto-kill by definition.

---

## 36. Narrow types at parse boundaries

When external input (JSON, env vars, CLI flags) can only hold a known finite set of values,
parse into an enum immediately — never store the raw string. This catches invalid values
at the boundary instead of deep in business logic.

Applies to: config parsing, HTTP payload deserialization, Redis value decoding.

> M4_001: `AnomalyRule.pattern` was `[]const u8` but only `"same_action"` was valid.
> `ApprovalPayload.decision` was `[]const u8` validated later by a separate function.
> Both replaced with enums; invalid values now fail at parse time.

---

## 37. Config-driven over enum-driven for multi-provider patterns

When supporting multiple providers (webhook signatures, API clients, auth schemes),
use a configuration struct with data fields, not an enum with per-variant switch arms.
Adding a new provider should be one new const, not new functions or switch cases.

> M3_001: Built `slack_verify.zig` with Provider enum + per-provider verify functions,
> then rewrote as `webhook_verify.zig` with `VerifyConfig` struct. Adding GitHub/Linear
> was one const each, zero new functions.

---

## 38. Test fixtures must use the same constants as production code

Never use inline string literals in test fixtures for values that have named constants.
If `codes.zig` defines `SKILL_DOMAINS_AGENTMAIL = "api.agentmail.to"`, tests must use
that constant, not a hardcoded `"api.agentmail.dev"`.

> M3_001: agentmail domain was `api.agentmail.to` in production, `api.agentmail.dev`
> in test fixtures, and `api.agentmail.com` in spec docs. Three different values.

---

## 39. Every ERR_* code must have a hint() entry

When adding a new error code to `codes.zig`, always add a corresponding entry in the
`hint()` function. The hint must be actionable: tell the operator what to check and
what command to run. Omitting the hint means the error code is useless to operators.

> M3_001: Added `ERR_TOOL_API_FAILED`, `ERR_TOOL_GIT_FAILED`, `ERR_TOOL_TIMEOUT`
> without hints. Caught in review.

---

## 40. Don't derive values by slicing related fields

When one value is logically independent of another, give it its own field. Don't
compute it by string-slicing a related field. This creates invisible coupling that
breaks when either field changes independently.

> M3_001: Derived Slack HMAC version `"v0"` by slicing prefix `"v0="[0..len-1]`.
> Fixed by adding explicit `hmac_version` field to `VerifyConfig`.

---

<!-- New rules use compact format: Rule / Why / Tags / Ref (4 lines each) -->

## 41. Comptime table scans need explicit eval quota

**Rule:** Add `@setEvalBranchQuota(N)` as the first line of any `comptime {}` block that iterates over a registry table with string comparison. Formula: `N ≈ code_count × table_size × avg_string_len`, round to next power-of-ten. Add a comment with the math.
**Why:** Default quota is 1000. 130 codes × 131 entries × char-by-char `std.mem.eql` = ~2.2M comparisons — blows the quota silently with "evaluation exceeded 1000 backwards branches".
**Tags:** zig, comptime, testing
**Ref:** M11_001 `m11_001_coverage_test.zig` — comptime exhaustive coverage for error code registry

---

## 42. `@embedFile` is sandboxed to the package root (`src/`)

**Rule:** Never use `@embedFile` to reach files outside `src/`. For external files (OpenAPI specs, config fixtures), write a Python/shell validator and wire it into a `make` target under `lint-zig`.
**Why:** Zig's embed security model restricts `@embedFile` to the package directory. `@embedFile("../../public/openapi.json")` is a hard compile error, not a runtime failure. There is no workaround except an external script.
**Tags:** zig, comptime, testing
**Ref:** M11_001 §3.1 — OpenAPI ErrorBody validation moved to `scripts/check_openapi_errors.py` + `make check-openapi-errors`

---

## 43. Fallback sentinels must not share a code with real registry entries

**Rule:** In any code registry with a fallback sentinel (e.g. `UNKNOWN_ENTRY`), the sentinel's key field must NOT match any real registered entry. Use a distinct value that cannot appear in the real table. Add a test that verifies the sentinel is absent from the table.
**Why:** A sentinel whose code matches a real entry causes tests to silently pass with wrong semantics (comparing the sentinel to "real" entries succeeds), and breaks comptime coverage gates that assume the sentinel is outside the table.
**Tags:** zig, error-handling, design
**Ref:** M11_001 `error_table.zig` — `UNKNOWN_ENTRY.code` was `"UZ-INTERNAL-001"` (real 503 entry), renamed to `"UZ-UNKNOWN"` (distinct sentinel)
