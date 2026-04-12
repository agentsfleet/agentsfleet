# Code Rules — Learned from Review

Generic principles from greptile reviews, PR feedback, and production incidents.

**Read this:** at EXECUTE start, during `/review`, when fixing review feedback.
**Ignore a rule:** only when the user explicitly overrides it with a stated reason.

Reference a rule as `RULE NDC`, `RULE OWN`, etc.

---

## RULE NDC — No dead code

**Rule:** Remove unused variables, imports, parameters, and unreachable branches immediately.
**Why:** They mislead readers about real dependencies and are flagged in every review.
**Tags:** zig, js, all
**Ref:** M1_001 unused deps in zombie.js; dead currentObj branch in simpleYamlParse. M30_002 dead CLI variables.

## RULE PSR — Use standard parsers — never hand-roll

**Rule:** Use the language's built-in parser for JSON/YAML/TOML/XML; never use indexOf or regex on structured formats.
**Why:** Hand-rolled parsers silently drop data and are injection-prone.
**Tags:** zig, js, security
**Ref:** M22_001 extractCreatedAt used indexOf on JSON. M1_001 simpleYamlParse silently dropped all arrays.

## RULE OWN — One owner per resource — no double cleanup

**Rule:** Every allocation has exactly one cleanup path — errdefer OR manual free, never both on the same pointer. For multi-step init, use the sequential errdefer chain (see ZIG_RULES.md "Multi-Step Init"). For shared ownership, use `ref()`/`unref()` with an atomic refcount — `unref()` destroys when count hits zero. Raw pointer field = borrowed (caller owns); refcounted field = owned (self manages).
**Why:** Two cleanup paths = double-free on the error path. The bvisor encoding (raw ptr = borrowed, refcount = owned) makes the ownership contract readable from the type alone.
**Tags:** zig, memory
**Ref:** M1_001 manual alloc.free() + errdefer on same pointer = double-free. bvisor Thread.zig, ThreadGroup.zig for ref/unref pattern.

## RULE CTM — Constant-time comparison for secrets

**Rule:** Never use short-circuit equality (==, eql, ===) to compare tokens or passwords; use XOR accumulation.
**Why:** Short-circuit byte comparison leaks secret length via timing side-channel.
**Tags:** zig, security
**Ref:** M1_001 webhook Bearer token compared with std.mem.eql.

## RULE ECL — Distinguish error classes — timeout ≠ fatal ≠ retryable

**Rule:** Never collapse all errors into one return; timeout → retry, fatal → propagate, 4xx → don't retry.
**Why:** Identical handling of 503 and 404 creates busy-loops and misleads callers.
**Tags:** zig, js, reliability
**Ref:** M22_001 readMessage returned null for ConnectionResetByPeer → busy-loop. streamRunWatch treated 503 = 404.

## RULE NSQ — Named constants, schema-qualified SQL

**Rule:** No magic numbers; all SQL in handlers must be schema-qualified (core.table, not table).
**Why:** Unqualified table names fail when search_path differs across environments.
**Tags:** zig, sql
**Ref:** M31_001 unqualified platform_llm_keys in handler query.

## RULE KYS — Composite keyset cursors for pagination

**Rule:** Encode (sort_column, id) in cursors — never a bare timestamp.
**Why:** Multiple rows share the same millisecond; scalar cursor silently skips them.
**Tags:** sql, zig
**Ref:** M1_001 activity_stream cursor was timestamp-only, dropped events at ms boundaries.

## RULE FLL — Files ≤ 350 lines (new/touched); functions ≤ 50 lines

**Rule:** Every new or touched .zig/.js file must stay under 350 lines; every new function under 50 lines.
**Why:** Files over 350L hide coupling and slow review; functions over 50L inline multiple concerns.
**Tags:** zig, js, all
**Ref:** AGENTS_POLICY_APPENDIX.md Code Structure Policies — tightened from 400L at M15.

## RULE XCC — Cross-compile before commit (Zig)

**Rule:** Run zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux before every Zig commit.
**Why:** macOS APIs (client.open, etc.) compile locally but don't exist on Linux; CI cache hides it in dev.
**Tags:** zig, ci
**Ref:** M22_001 client.open compiled on macOS, absent on Linux — 3 rounds to fix. v0.4.0 bare -gnu in CI.

## RULE FLS — Flush all layers — drain all results

**Rule:** After TLS flush, also flush the socket layer. Cast UUID/JSONB to ::text in SELECT. For pg results: use `PgQuery` (see ZIG_RULES.md "Pg Query Wrapper") — `defer q.deinit()` auto-drains. Manual `q.drain() catch {}; q.deinit()` on early-exit paths is eliminated by the wrapper.
**Why:** TLS flush only encrypts into buffer; undrained slices dangle; ::text prevents binary/text divergence across OS.
**Tags:** zig, tls, postgres
**Ref:** M22_001 missing socket flush → infinite hang. M1_001 UUID read as binary on Linux CI, text on macOS. M10_004 PgQuery wraps drain into deinit.

## RULE TIM — Timing invariants must be explicit

**Rule:** Document and enforce heartbeat_interval < socket_timeout < proxy_idle_timeout.
**Why:** Heartbeat > socket_timeout means the first wakeup misses the window and proxy drops at t=30s.
**Tags:** zig, reliability
**Ref:** M22_001 heartbeat 30s > socket timeout 25s → proxy dropped connection.

## RULE STR — Streaming must verify transport, not just parser

**Rule:** Test byte-level incrementality at the transport layer, not just the parser.
**Why:** A correct SSE parser passing feedBytes() does not prove the HTTP layer isn't buffering.
**Tags:** zig, js, testing
**Ref:** M22_001 Zig CLI buffered entire SSE response, printed all events at once.

## RULE PJV — Primitives are pass-by-value in JS

**Rule:** Never pass a mutable boolean/number expecting to observe later changes; use object/closure/AbortController.
**Why:** Primitives are copied on pass; the called function sees a frozen snapshot.
**Tags:** js
**Ref:** M22_001 abortedRef boolean was frozen at false inside called function.

## RULE CAS — Lock-free CAS: never read after failure

**Rule:** After a CAS fails, don't read the slot's fields; use an occupied flag + separate ready flag.
**Why:** The winning thread may still be writing when the loser reads — partial write is visible.
**Tags:** zig, concurrency
**Ref:** M28_001 resolveSlot read partially-written fields after losing CAS.

## RULE TVR — Test only reachable values

**Rule:** Don't insert test values that violate schema CHECK constraints; use independent schema spec for drift tests.
**Why:** Testing invalid values passes in isolation but fails at integration; tautological drift tests catch nothing.
**Tags:** zig, testing, sql
**Ref:** M31_002 tested 0 for a column with CHECK >= 512.

## RULE JCL — CLI JSON contract discipline

**Rule:** Use only stable error codes; UNKNOWN_COMMAND must name the unrecognized token; dual jsonMode guards need a comment.
**Why:** Ad-hoc codes break CLI consumers; usage text as error message is unparseable.
**Tags:** js, cli
**Ref:** M30_002 undocumented AGENT_ERROR/IO_ERROR codes; usage text returned as error message.

## RULE MIG — Migration index assertions track position

**Rule:** When inserting, splitting, or removing migration files, update every index-based assertion in `src/cmd/common.zig`. While `cat VERSION` < 2.0.0, removed files become `SELECT 1;` (see RULE SCH) — their array slot stays but the assertion must match the new content.
**Why:** Stale index silently points at the wrong SQL file with no compile-time error. M10_001 assertions checked for CREATE TABLE in files that were now `SELECT 1;` version markers.
**Tags:** zig, sql
**Ref:** M31_001 migrations[7] pointed at wrong file after a split. M10_001 migrations[14]/[15] asserted dropped table names in version marker files.

## RULE SCM — RESOLVED — SQL comment handling

**Rule:** ~~No semicolons or apostrophes in SQL comments.~~ Fixed in M10_001: `SqlStatementSplitter` now skips `--` line comments before processing. Semicolons and apostrophes in comments are safe. `make test` validates every migration file is parseable (zero-statement files fail).
**Tags:** sql
**Ref:** M1_001 original bug. M10_001 `sql_splitter.zig` + unit test in `common.zig` replaced this rule.

## RULE GLS — Gate dispatcher must not glob itself

**Rule:** Exclude 00_* from 00_gate.sh's own glob; use 0[1-9]_*.sh + [1-9][0-9]_*.sh.
**Why:** Glob matching itself creates a fork bomb.
**Tags:** bash, ci
**Ref:** PR #162 glob matched itself → fork bomb in CI.

## RULE FNC — Functions ≤ 50 lines, methods ≤ 70 lines

**Rule:** Functions ≤ 50 lines; methods ≤ 70 lines; split into named helpers if exceeded.
**Why:** Functions over 50L inline multiple concerns and are untestable in isolation.
**Tags:** zig, js, all
**Ref:** M1_001 handleReceiveWebhook was 120+ lines — 8 steps inlined into one function.

## RULE UFS — All user-facing strings are constants

**Rule:** Every string that crosses a module boundary (response, header prefix, Redis key) must be a named constant.
**Why:** Inline literals across modules drift independently and create silent mismatches.
**Tags:** zig, js
**Ref:** M1_001 handleReceiveWebhook had 7 inline strings including "Bearer ", status values, and error codes.

## RULE EMS — Error messages follow a standard structure

**Rule:** Always use error_codes.ERR_* + a constant message string; never mix ERR_* constants with inline strings.
**Why:** Inconsistent structure breaks operator tooling and makes error codes unsearchable.
**Tags:** zig
**Ref:** M1_001 webhook handler mixed ERR_* constants with inline message strings.

## RULE PRI — No prompt injection from user input

**Rule:** Never concatenate raw user input into agent prompts; validate, type-check, and length-bound all external input.
**Why:** Unsanitized input enables prompt injection into agent decisions and tool calls.
**Tags:** security, zig, js
**Ref:** Principle — no single incident yet.

## RULE TGU — Tagged unions over optional-field structs

**Rule:** Use union(enum) for mutually-exclusive variants; never represent them with optional struct fields.
**Why:** Optional fields make invalid states representable; tagged unions make them unrepresentable.
**Tags:** zig
**Ref:** M2_002 ZombieTrigger struct with ?source/?schedule — webhook-without-source was valid but semantically wrong.

## RULE VLT — Secrets belong in vault, not in entity tables

**Rule:** Store a vault key_name in entity tables; resolve via crypto_store.load() at runtime.
**Why:** Plaintext secrets appear in query results, backups, and logs.
**Tags:** zig, sql, security
**Ref:** M2_002 webhook_secret TEXT column in core.zombies → refactored to webhook_secret_ref.

## RULE STS — No static strings in SQL schema

**Rule:** Never use DEFAULT or CHECK with hardcoded strings in SQL; enforce value constraints via application constants.
**Why:** SQL can't reference Zig/JS constants, so schema strings drift from code.
**Tags:** sql
**Ref:** M2_002 DEFAULT 'active' and CHECK status IN (...) removed from core.zombies.

## RULE ESC — Escape control characters in JSON string emission

**Rule:** Escape all 0x00-0x1F ASCII control chars per RFC 8259 §7 in any custom JSON encoder.
**Why:** Unescaped \n or null bytes produce malformed JSON and enable key injection.
**Tags:** zig, security
**Ref:** M2_002 writeJsonString only escaped " and \ — \n in YAML value could inject JSON keys.

## RULE CTC — Constant-time comparison must not short-circuit on length

**Rule:** Run XOR loop over min(a.len, b.len) bytes always; fold length mismatch into result after the loop.
**Why:** Early return on length mismatch leaks the expected secret's length.
**Tags:** zig, security
**Ref:** M2_002 constantTimeEq skipped XOR loop entirely on length mismatch.

## RULE IMS — Use []const u8 for immutable data, not []u8

**Rule:** Declare struct fields as []const u8 for DB results and parsed input; use []u8 only for data you mutate.
**Why:** Mutable slice on immutable data misleads readers and allows accidental mutation.
**Tags:** zig
**Ref:** M2_002 ZombieRow used []u8 for workspace_id, status, token — all immutable DB data.

## RULE ORP — Cross-layer orphan sweep on every rename, delete, or format change

**Rule:** After any rename/delete, grep OLD_NAME across src/, schema/, zombiectl/, docs/ before committing.
**Why:** Stale references in tests, SQL queries, and comments compile fine but fail at runtime.
**Tags:** zig, js, sql, all
**Ref:** M2_002 webhook_secret renamed but stale comments and test fixtures still used the old name.

## RULE CHR — CHORE(close) must include orphan verification gate

**Rule:** Before opening a PR, grep every renamed/deleted symbol and confirm zero non-historical hits.
**Why:** The PR that changes the symbol owns the full cleanup; deferred orphans compound across PRs.
**Tags:** process
**Ref:** M2_002 multiple follow-up fix commits after missing orphan sweep in CHORE(close).

## RULE TST — Test discovery requires explicit import in main.zig

**Rule:** Add _ = @import("path/to/file.zig"); to main.zig test block for every new Zig file with tests.
**Why:** Inline test blocks don't run unless the file is reachable from the test root.
**Tags:** zig, testing
**Ref:** M2_001 router tests existed since M16 but never ran; two pre-existing bugs surfaced on import.

## RULE PTR — ~~Pointer dereference for anytype query params~~ ELIMINATED by PgQuery wrapper

**Rule:** ~~Use q.*.next() and q.*.drain() when a pg query result is passed as &q via anytype.~~
**Status:** Eliminated in M10_004. Use `src/db/pg_query.zig` — helpers take `*PgQuery`, not `anytype`. `q.next()` always works; the `q.*.next()` footgun is structurally impossible.
**Why the old rule existed:** q.next() on a pointer type compiles but calls the wrong dispatch.
**Tags:** zig, postgres
**Ref:** M2_001 original bug. M10_004 PgQuery wrapper eliminates this class.

## RULE ZAL — Zig 0.15 ArrayList API

**Rule:** Use var list: std.ArrayList(T) = .{}; — pass alloc per-operation: append(alloc,), deinit(alloc), toOwnedSlice(alloc).
**Why:** ArrayList.init(alloc) does not compile in Zig 0.15.
**Tags:** zig
**Ref:** Zig 0.15 breaking API change from 0.13.

## RULE DFS — No dead struct fields

**Rule:** Remove struct fields that hold the same value at every construction site; inline the constant.
**Why:** Invariant fields masquerade as configuration and mislead readers.
**Tags:** zig
**Ref:** M4_001 AnomalyRule.behavior was always .auto_kill at every site — field removed.

## RULE NTP — Narrow types at parse boundaries

**Rule:** Parse external input into enums immediately at the boundary; never store raw strings for finite value sets.
**Why:** String validation deferred to business logic silently accepts garbage at the boundary.
**Tags:** zig, js
**Ref:** M4_001 AnomalyRule.pattern was []const u8 with only one valid value; ApprovalPayload.decision validated late.

## RULE CFG — Config-driven over enum-driven for multi-provider patterns

**Rule:** Use a VerifyConfig struct with data fields for multi-provider patterns; avoid enum + per-variant switch arms.
**Why:** Adding a provider requires one new const, not new functions or switch cases.
**Tags:** zig
**Ref:** M3_001 slack_verify.zig Provider enum rewrote as webhook_verify.zig VerifyConfig struct.

## RULE TFX — Test fixtures must use the same constants as production code

**Rule:** Never hardcode string literals in test fixtures for values that have named constants in production code.
**Why:** Three copies of the same value drift independently; only one matches production.
**Tags:** zig, js, testing
**Ref:** M3_001 agentmail domain was .to in prod, .dev in tests, .com in spec docs.

## RULE ERH — ~~Every ERR_* code must have a hint() entry~~ ELIMINATED (M16_001)

**Status:** ELIMINATED by M16_001 — Entry struct requires `.hint` field; comptime asserts `hint.len > 0`.
**Tags:** zig

## RULE DRV — Don't derive values by slicing related fields

**Rule:** Give logically independent values their own struct fields; never derive by string-slicing a sibling field.
**Why:** Derived slice creates invisible coupling that breaks when either field changes independently.
**Tags:** zig
**Ref:** M3_001 HMAC version "v0" derived by slicing "v0=" prefix — fixed with explicit hmac_version field.

## RULE SCH — Pre-v2.0 schema removal: full teardown, no markers, no DROP

**Rule:** While `cat VERSION` < 2.0.0 (teardown-rebuild era), removing tables MUST be a full teardown: (1) delete the SQL file (`rm schema/NNN_foo.sql`), (2) remove the `@embedFile` constant from `schema/embed.zig`, (3) remove the migration array entry from `src/cmd/common.zig` and update its array length + any index-based tests. Never write ALTER TABLE, DROP TABLE, or `SELECT 1;` placeholders. Never keep version-marker files. Migration slot numbers are not sacred pre-v2.0 — the DB is wiped on every rebuild, and gaps in numbering are fine. After VERSION >= 2.0.0, switch to proper ALTER/DROP migrations in new numbered files.
**Why:** Markers accumulate dead code and still force CI to splitter-parse them. Pre-v2.0 there is zero production data to protect; full removal is cleaner and leaves no false grep hits or stale migration slots.
**Tags:** sql, process
**Ref:** M17_001 harness teardown — supersedes prior "replace with `SELECT 1;`" guidance. Under the old rule, M10_001 comment-only markers broke CI (apostrophe in "slots" opened unterminated string in splitter); full deletion avoids the marker problem entirely.

## RULE EP4 — Removed endpoints return 410 Gone, not 404

**Rule:** Intentionally removed endpoints return HTTP 410 Gone with a named error code, not 404.
**Why:** 410 signals permanent intentional removal to clients and monitors; 404 implies a routing error.
**Tags:** zig, api
**Ref:** M10_001 all /v1/runs/* and /v1/specs return ERR_PIPELINE_V1_REMOVED 410.

## RULE FXS — Fixed-size scan buffers are security bypasses

**Rule:** Scan security-relevant input in overlapping chunks; never silently truncate at a fixed buffer size.
**Why:** Attacker prepends padding larger than the buffer to push payload past the scan window.
**Tags:** zig, security
**Ref:** M6_001 64KB normalization buffer in injection_detector.zig bypassed via 65KB padding.

## RULE RSP — Reject unsupported patterns at parse time, not match time

**Rule:** Validate pattern syntax at parse time with a clear error; never silently fall through to a different match mode.
**Why:** Silent fallthrough makes configuration bugs invisible to operators.
**Tags:** zig
**Ref:** M6_001 globMatch silently treated mid-path wildcards as exact match — fixed in parseEndpointRules.

## RULE OBS — Every observable state must have a log/event entry

**Rule:** Every result variant operators need to know about must emit a log line or activity event; .truncated without an event is a blind spot.
**Why:** Silent state transitions are invisible in dashboards and incident response.
**Tags:** zig
**Ref:** M6_001 content scanner returned .truncated but eventTypeForScan mapped it to null — no event fired.

## RULE BRQ — ~~Large comptime loops need @setEvalBranchQuota~~ ELIMINATED (M16_001)

**Status:** ELIMINATED by M16_001 — no cross-file comptime loop. Validation is inside error_registry.zig with its own quota.
**Tags:** zig, comptime

## RULE EMB — @embedFile cannot cross src/ boundary

**Rule:** Never use `@embedFile` to reach files outside `src/`. For external files (OpenAPI specs, config fixtures), write a Python/shell validator and wire it into a `make` target under `lint-zig`.
**Why:** Zig's embed security model restricts `@embedFile` to the package directory. `@embedFile("../../public/openapi.json")` is a hard compile error, not a runtime failure. There is no workaround except an external script.
**Tags:** zig, comptime, testing
**Ref:** M11_001 §3.1 — OpenAPI ErrorBody validation moved to scripts/check_openapi_errors.py + make check-openapi-errors.

## RULE SNT — ~~Registry sentinels must be distinct from real entries~~ ELIMINATED (M16_001)

**Status:** ELIMINATED by M16_001 — UNKNOWN sentinel is defined outside REGISTRY; comptime assertion prevents collision.
**Tags:** zig, error-handling

## RULE SSM — Use StaticStringMap for O(1) static registry lookup

**Rule:** Use `std.StaticStringMap` for comptime-generated O(1) lookup on static string→value registries. Build the map from the existing TABLE array at comptime with a `const LOOKUP_MAP = blk: { ... }` block.
**Why:** Linear scan over 130+ entries on every error response is unnecessary when the table is known at compile time. `StaticStringMap.initComptime()` generates a perfect hash at zero runtime cost.
**Tags:** zig, performance, comptime
**Ref:** M11_001 error_table.zig — lookup() replaced O(n) for-loop with StaticStringMap(usize) mapping code→TABLE index.

## RULE HLP — Don't ship helpers without a consumer

**Rule:** Do not ship convenience helpers without at least one consumer. Remove dead API surface (unused pub fns) before merge.
**Why:** Unused helpers invite misuse. A `db()`/`releaseDb()` pair with no consumer trains future devs to use it, but without a `defer`-friendly wrapper, they'll leak pool connections.
**Tags:** zig, api-design
**Ref:** M11_001 hx.zig — db()/releaseDb() shipped with zero callers, all handlers used ctx.pool directly. Removed in greptile fix.

## RULE CIV — CI validators must verify $ref targets

**Rule:** CI validators must verify `$ref` targets, not silently skip them. When a response has no inline `content`, check that its `$ref` points to the expected shared response.
**Why:** A `$ref` to `LegacyError` (using `application/json`) passes the validator undetected if the script only checks inline content blocks.
**Tags:** python, ci, openapi
**Ref:** M11_001 check_openapi_errors.py — $ref responses were silently skipped; added target validation.

## RULE TNM — Test names must state what they verify

**Rule:** Test names must state what they verify, not narrate the author's reasoning. No mid-sentence corrections ("does not X — wait, it does").
**Why:** When a test fails, the name is the first thing an engineer reads. A self-contradicting name wastes investigation time.
**Tags:** zig, testing
**Ref:** M11_001 error_registry_test.zig — renamed "does not start with UZ- — wait, it does" to "has sentinel code UZ-UNKNOWN and is 500".

## RULE TXN — Every DELETE in a transaction must ROLLBACK on failure

**Rule:** When multiple `DELETE` (or `UPDATE`) statements share a transaction, every `catch` branch must issue `ROLLBACK` and return. Never log-and-continue — that causes a partial commit where one row is deleted and another is left orphaned.
**Why:** Silent partial commits create vault inconsistencies. For example, deleting the api-key row but leaving the preference row means future GET reports no key while the preference pointer persists.
**Tags:** zig, database, transactions
**Ref:** M11_002 workspace_credentials_http.zig — second DELETE swallowed the error; ROLLBACK was missing. Caught by greptile on PR #189.

## RULE ZIG — Zig struct init/deinit/ownership conventions

**Rule:** Follow this pattern for all Zig structs:
- **`init` signature:** `fn init(allocator: Allocator, ...) !Self` for stack-returned structs that can fail; `fn init(...) !*Self` for heap-allocated. Never return `Self` from `init` when setup is fallible — use `!Self`.
- **`deinit` signature:** Always `pub fn deinit(self: *Self) void` — pointer receiver, no error return. Cleanup must be infallible from the caller's perspective.
- **Multi-step init:** Wrap each allocator.create or sub-init with `errdefer` so partial construction always cleans up. Pattern: `const x = try alloc.create(T); errdefer alloc.destroy(x);` before the next allocating step.
- **Ownership comments:** When a field is borrowed (not owned), say so explicitly: `// owned by caller, not freed here`. Silence implies ownership.
- **Private fields:** No underscore prefix convention. Use Zig visibility (`pub` vs none) and `const`/`var` to control mutability. Helper functions are private by default (no `pub`).
- **Refcounting:** Use `std.atomic.Value(usize)` with `ref()` / `unref()` helpers for shared ownership. `unref()` calls `destroy` when count hits zero.

**Why:** bvisor/src/core shows these patterns consistently. Deviating breaks the ownership contract — missed errdefer means memory leak on partial init; missing ROLLBACK on deinit-equivalent means inconsistent state.
**Tags:** zig, memory, ownership, patterns
**Ref:** bvisor src/core/Supervisor.zig, Thread.zig, ThreadGroup.zig, OverlayRoot.zig, LogBuffer.zig.
