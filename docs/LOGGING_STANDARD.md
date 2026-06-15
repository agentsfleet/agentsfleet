# LOGGING_STANDARD — wire format and discipline for structured logs

The contract between code that emits log records and the systems (humans, collectors, dashboards) that consume them. Cross-language: applies to every Zig, TypeScript, JavaScript, and shell call site that emits to `stderr`/`stdout`.

Pre-design rules, decisive defaults, and the anti-patterns each rule exists to prevent. Every rule has a one-line "why" so you can judge edge cases.

## §1 · Scope

Triggers on every `Edit`/`Write` that adds, removes, or changes a log emit:

- `*.zig` outside `vendor/`/`third_party/`/`.zig-cache/` — `std.log.*`, `std.debug.print`, `std.io.getStdErr().writer().print`, any helper in `src/lib/logging/`.
- `*.ts`/`*.tsx`/`*.js`/`*.jsx` outside `vendor/`/`node_modules/` — `console.*`, custom logger calls.
- `*.sh` outside generated directories — `echo`, `printf` to `&2`.

Out of scope (explicitly):

- Test-only diagnostic prints inside `*_test.zig`, `*.test.ts`, `*.spec.ts`. Tests render to humans, not collectors; gates ignore.
- Build/release scripts in `~/Projects/dotfiles/`, `audits/release-*` — toolchain output, not application logs.
- Generated framework noise (Next.js startup banners, Bun runtime warnings) — out of our control.

The **LOGGING GATE** (`dispatch/write_any.md`, Logging Gate) sits on top of this file — it fires in addition to the language-level rules in `dispatch/write_zig.md` and `dispatch/write_ts_adhere_bun.md`, not instead of them.

## §2 · Today's de-facto standard (survey-derived)

Documented honestly, not aspirationally. Pre-M62 baseline (the fix-pass converges every existing emit toward §3 onward):

- Zig calls are mostly `std.log.scoped(.tag).info(comptime fmt, args)` with positional `{s} {d}` placeholders. Severity choice is inconsistent: successful happy-path events sometimes log at `info`, sometimes are silent. Per-call migration to the structured `log.<level>("event", .{ .field = val })` form is in flight.
- A small number of Zig sites use `std.debug.print` directly — bypasses the scope/level system entirely. Always a violation.
- The named module `log` (source: `src/lib/logging/mod.zig`) exposes `scoped(.tag)` returning a logger struct with `.err` / `.warn` / `.info` / `.debug` methods. M62 removed the older free-function helpers (`logErr` / `logErrWithHint` / `logWarnErr`) — there is no compat shim.
- TypeScript (`cli/src/**`) calls `console.log`/`console.error` directly. No structure. No scope. No severity beyond err vs out.
- Error-code embedding (`UZ-XXX-NNN`) appears on some `err` lines but not others. The registry (`src/agentsfleetd/errors/error_registry.zig`) is the source of truth, but enforcement is voluntary.
- No collector-friendly format. Logs are a mix of free-form English and ad-hoc `key={value}` fragments.

The proposed standard below is what every new emit must conform to and what the fix-pass converges existing emits toward.

## §3 · Wire format — logfmt

Every record is a single newline-terminated line in **logfmt**: space-separated `key=value` pairs, value-quoted only when the value contains whitespace or `=` or `"`.

```
ts_ms=1715004901234 level=warn scope=executor event=tool_call_failed correlation_id=abc-123 error_code=UZ-EXEC-012 tool=bash duration_ms=1240 msg="tool returned non-zero"
```

**Required keys per record (in this order):**

| Key | Type | Required | Notes |
|---|---|---|---|
| `ts_ms` | u64 | always | Unix epoch milliseconds. Single source of truth for ordering. |
| `level` | enum | always | One of `err`, `warn`, `info`, `debug`, `trace`. See §4. |
| `scope` | snake_case ident | always | Subsystem tag (e.g. `executor`, `http`, `auth`, `redis`). See §7 for the binding. |
| `event` | snake_case `verb_noun` | always | What happened. Examples: `request_started`, `tool_call_failed`, `worker_drained`. |
| `error_code` | `UZ-XXX-NNN` | required on `err`/`warn` mapping to a registry code | See §5. |
| `correlation_id` | string | optional | Trace context where present. |
| `msg` | string | optional | Human-readable elaboration. **Last** field by convention so multiline-ish msgs don't break grep. |
| `<arbitrary>` | any | optional | Domain fields (`tool=bash`, `duration_ms=1240`, `worker_id=42`). Snake_case keys. |

**Encoding rules:**

- Values with no whitespace, `=`, or `"`: bare (`duration_ms=1240`, `tool=bash`).
- Values with whitespace or `=` or `"`: double-quoted, with `"` and `\` backslash-escaped (`msg="tool returned: \"exit 1\""`).
- Newlines in values: replaced with `\n` literal (two chars). Logfmt is one-line-per-record; embedded newlines break tooling.
- Booleans: `true` / `false`.
- Numbers: bare integers and decimals. No thousands separators.
- Slices/arrays: omit. If you need to log a collection, log it as count + sample (`count=42 sample=abc,def,ghi`) or emit one record per item.
- Null/absent: omit the key entirely. **Don't emit `key=null` or `key=`** — empty/null fields confuse parsers.

**Why logfmt and not JSON:** logfmt is grep-friendly for `tail -f`, parses cleanly into Loki / Vector / Promtail / Mezmo / Datadog with zero transform, and is readable without tooling. JSON would also work but is uglier on a dev box. logfmt is the production-server sweet spot. See §9 for the optional pretty-printer.

## §4 · Severity contract

Five levels. Use them as defined; do not invent intermediate levels.

| Level | When | Examples | Default visibility |
|---|---|---|---|
| `err` | Operation failed; user/operator action required; data may be inconsistent. | DB connection lost, auth failure, panic recovery, syscall failure that breaks a request. | always emitted |
| `warn` | Operation succeeded but with degraded behaviour, or an expected failure on a recoverable path. | Retry succeeded after one failure; cache miss to slow path; deprecated endpoint hit. | always emitted |
| `info` | Operationally significant lifecycle events. | Server start, server stop, request bookends, worker pool resized, batch boundary crossed. | emitted in production |
| `debug` | Diagnostic detail useful to developers but noisy for operators. | Per-iteration tool calls, intermediate state in a multi-step pipeline, cache lookup outcomes. | hidden by default; opt-in via env var (see §7) |
| `trace` | Per-byte / per-event firehose. | Bytes-on-the-wire, parser tokens, allocator events. | hidden by default; opt-in only; rarely written |

**Happy-path silence rule** — successful operations are silent unless they are *operationally significant*. A request handler returning 200 to a client is **not** operationally significant; an endpoint receiving its first request after a deploy **is**.

Concrete tests for whether an event deserves `info`:

- **Would an operator paging at 3am want to see this?** If no → not `info`.
- **Does it bookend a phase the operator might want to bisect?** (server start, server stop, migration applied, batch boundary) → `info`.
- **Is this per-request or per-iteration?** → not `info`. Use `debug`, hidden by default, on-demand via env var.

`note` is **NOT** a level. Bun has it; we do not. Five levels only.

## §5 · Error-code embedding

Every `err` and `warn` record that maps to a domain error MUST carry `error_code=UZ-XXX-NNN` where `UZ-XXX-NNN` is declared in `src/agentsfleetd/errors/error_registry.zig`.

- **Used-but-undeclared** (`UZ-FAKE-999` appearing in code, no entry in registry): **blocking** in `make lint`.
- **Declared-but-unreferenced** (registry has `UZ-LEGACY-007`, no code references it): **informational**. Deletion may be deferred to a sweep milestone.

Embed the code as a struct field on the logger call: `log.err("event_name", .{ .error_code = error_codes.ERR_X, .err = @errorName(err) })`. The encoding helpers in `src/lib/logging/mod.zig` serialize it as `error_code=UZ-XXX-NNN` per §3. CLI (`agentsfleet`) renders the code in human format and as `code: "UZ-XXX-NNN"` in `--json` output (see §8).

System-level failures with no domain meaning (e.g. raw `EACCES` from a syscall before we attribute it to a tenant operation) emit without `error_code`. The follow-up rule: if a syscall failure surfaces to a user, it gets attributed to a registry code at the boundary.

## §6 · PII / secret discipline

Inherits the redaction list from M42_002 (`src/runner/engine/runner_progress.zig`). Same secret values must not appear anywhere in log records.

- **Allocator outputs** (Postgres connection strings, Redis URLs, OAuth tokens, OpenAI keys) — never log. If a record needs to identify a config source, log the *source* (`source=env:OPENAI_API_KEY`), not the *value*.
- **Tenant-supplied secrets** (workspace API keys, BYOK credentials) — redaction harness covers stdout/stderr from executor children. Log emit sites in this codebase MUST NOT bypass the harness.
- **Stderr coverage gap** — today's redactor covers child stdout only. Closing this gap (extending to stderr) is part of M62_001's fix-pass, not a separate milestone.
- **`msg=` fields** — values copy-pasted into `msg=` are the most common leak source. Audit script flags long `msg=` values; reviewer must verify they don't carry credentials.

When in doubt, omit. A missing field is recoverable; a leaked secret is not.

## §7 · Per-language binding — Zig

The wire format above is produced by helpers in `src/lib/logging/mod.zig`, exposed as the named module `log` so any layer-isolated tree (auth/, executor/) can import it without violating layer rules. Call sites use:

```zig
const logging = @import("log");                  // named module, see build.zig
const log = logging.scoped(.executor);            // compile-time enum-literal tag

log.info("tool_call_started", .{
    .correlation_id = req.id,
    .tool = "bash",
});

log.warn("tool_call_failed", .{
    .error_code = "UZ-EXEC-012",
    .tool = "bash",
    .duration_ms = elapsed,
    .msg = "tool returned non-zero",
});
```

**Mechanics:**

- `logging.scoped(.tag)` returns a type bound to that compile-time scope (the `.tag` is an enum literal). The returned type exposes `.err`, `.warn`, `.info`, `.debug` methods.
- Each method takes a comptime `event: []const u8` (snake_case `verb_noun`) and an anonymous-struct of fields.
- Field encoding to logfmt happens in the helper, not at the call site. Call sites never assemble strings.
- Optional fields with null payload are omitted (no `key=null` / `key=`) per §3.

**Compile-time visibility short-circuit** — pattern adapted from Bun's `Output.scoped` (`bun:src/output.zig:869`). Hidden scopes compile to **zero instructions**:

```zig
pub fn LoggerFor(comptime scope: Scope) type {
    return struct {
        pub inline fn debug(comptime event: Event, fields: anytype) void {
            if (comptime scope_visibility(scope, .debug) == .hidden) return;
            emit(.debug, scope, event, fields);
        }
        // ... err / warn / info / trace
    };
}
```

The `comptime if ... return` makes the entire emit path disappear when the scope is gated off. A `debug`-tagged log call in a tight loop, when its scope is gated off, costs zero. Faster than any runtime-flag library.

**Runtime visibility override** — env var `AGENTSFLEET_LOG_<SCOPE>=debug` (e.g. `AGENTSFLEET_LOG_EXECUTOR=debug`) flips a scope to debug-visible without recompiling. Read once at startup, cached. Production never re-checks env per call.

**Field encoding is allocation-free on the hot path** — the helper writes directly to a thread-local 4 KiB buffer and flushes on `\n`. Spillover to heap only on records exceeding 4 KiB, which is rare and warns at debug.

**Anti-patterns flagged by `logging.sh`:**

| Pattern | Why banned | Fix |
|---|---|---|
| `std.log.info("..."` outside `logging.scoped(.tag)` | Bypasses scope/event/field discipline. | Convert to `logging.scoped(.tag).info("event", .{...})`. |
| `std.debug.print(` in non-test source | No level, no scope, no field structure. Dev-only debugging that escaped to main. | Convert or delete before commit. |
| `std.log.err` without `error_code=` field | Drops registry traceability. | Add the registry code, or add a registry entry if missing. |
| `std.log.scoped(...)` outside `src/lib/logging/` | Free-form printf-style emit. | Switch the file's alias to `const log = logging.scoped(.tag);` and migrate every call site to the event+fields form. |
| Positional `{s} {d}` placeholders | Not logfmt; not greppable. | Convert to struct-of-fields. |

## §8 · Per-language binding — TypeScript / JavaScript (`agentsfleet`)

Wire format identical to §3. CLI obeys two render modes:

**Human mode** (default — TTY attached, no `--json` flag):

```
error UZ-EXEC-012: tool returned non-zero (executor) tool=bash duration=1.24s
  hint: check the tool's stderr above for details
  see: https://docs.agentsfleet.net/errors/UZ-EXEC-012
```

Colors: red `error`, dim parens. Rendering inspired by Bun's `SystemError.format()` (`bun:src/bun.js/bindings/SystemError.zig:85`) — same shape, our error registry as the source.

**JSON mode** (`--json` flag, or stdout is a pipe and `AGENTSFLEET_FORMAT=json` is set):

```json
{ "code": "UZ-EXEC-012", "message": "tool returned non-zero", "scope": "executor", "fields": { "tool": "bash", "duration_ms": 1240 }, "hint": "check the tool's stderr above for details", "docs": "https://docs.agentsfleet.net/errors/UZ-EXEC-012" }
```

Schema mirrors Bun's `SystemError` extern struct (`bun:src/bun.js/bindings/SystemError.zig:1`) — `{ code, message, ...context, hint?, docs? }`. Our `code` is `UZ-XXX-NNN` (registry-defined) rather than errno; the *shape* is what we mirror.

**Logging emit sites** in `cli/src/**` use a thin Bun-runtime logger that produces logfmt records to `stderr`. `console.log` / `console.error` are forbidden in source per `dispatch/write_ts_adhere_bun.md` §10 — `logging.sh` enforces this for TS/JS.

**Module-level error style** is governed by `dispatch/write_ts_adhere_bun.md` §9 (one style per module — throw OR Result, never both). The error type itself is the same:

```ts
class AgentError extends Error {
  readonly code: string;          // UZ-XXX-NNN
  readonly fields: Record<string, unknown>;
  readonly hint?: string;
}
```

Throw-style modules `throw new AgentError({...})`. Result-style modules return `{ ok: false, error: new AgentError({...}) }`. Render path picks human or JSON based on the runtime mode.

## §9 · Pretty-printer (dev-only render variant)

In production: every record on the wire is logfmt. No exceptions.

In development (TTY attached, optional `AGENTSFLEET_LOG_PRETTY=1` env var): the same record can render as a colored printf-style line for human readability:

```
14:32:18.234  WARN  executor  tool_call_failed  tool=bash duration=1.24s — tool returned non-zero  [UZ-EXEC-012]
```

Mechanics:

- TTY check happens **once at process startup** (`isatty(stderr) && env.AGENTSFLEET_LOG_PRETTY != "0"`). Cached as a compile-once boolean.
- A single sink picks the formatter at startup; from then on every record renders through the chosen formatter exactly once. **No strip step. No re-render.** The wire path never sees colors.
- LOGGING GATE audits the **wire** format only. Pretty mode is post-hoc render and not on the audit path.

Performance: zero measurable production impact. The pretty-vs-logfmt selector is one branch the predictor handles in a single cycle; in production (no TTY) it always takes the logfmt path. See `§7` for hot-path details (compile-time visibility short-circuit, thread-local 4 KiB buffer).

## §10 · Reference: Bun's conventions (citations)

We borrowed pattern, not code. Concrete file:line references for traceability:

| Concept | Bun reference | What we took |
|---|---|---|
| Scoped logger with comptime visibility | `~/Projects/oss/bun/src/output.zig:869-902` | `Scope` enum + `LoggerFor(scope)` returning a struct of inline methods. Compile-time short-circuit via `comptime if (visibility == .hidden) return;`. |
| Severity ladder | `~/Projects/oss/bun/src/logger.zig:1-31` | Five levels (we drop Bun's `note`). |
| `Output.err` rendering errno + label + syscall | `~/Projects/oss/bun/src/output.zig:1155-1170` | Inspiration for `agentsfleet` human render: `code: message (context)`. |
| `SystemError` JSON-facing shape | `~/Projects/oss/bun/src/bun.js/bindings/SystemError.zig:1-12` | `{ code, message, path?, syscall?, ... }` mirrored as our `--json` output schema. |
| `SystemError.format()` ANSI-colored render | `~/Projects/oss/bun/src/bun.js/bindings/SystemError.zig:85-116` | Pattern reused for `agentsfleet` human mode (red code, dim context). |

Zero lines copied verbatim. The patterns above each compile to ~10–30 lines of our own Zig/TypeScript in idiomatic form.

## §10A · Tightening clauses (closures of common skip rationalizations)

Failure modes the audit script and reviewer must close. These are **not aspirational** — each closes a specific way an agent could otherwise dodge the rule.

| # | Rationalization | Closure |
|---|---|---|
| L1 | "Temporary debug print, I'll remove later" | `logging.sh` greps `std.debug.print` and `console.log` / `console.debug` / `console.info` in non-test source unconditionally. Found in commit → gate fails. No "temporary" carve-out. |
| L2 | "`std.log.scoped` is fine, `obs.scoped` is just a wrapper" | `std.log.scoped` is **forbidden** in `src/**/*.zig` outside `src/lib/logging/`. Only `obs.scoped` is callable. Audit flags every `std.log.` call site. |
| L3 | "I added `error_code=UZ-NEW-001` — registry entry coming next commit" | The registry entry **must land in the same commit** as the first reference. `error-codes.sh` runs against the staged diff; missing entry = blocking. |
| L4 | "This per-iteration event matters for debugging — `info`-level" | `info` allow-list is fixed: `server_started`, `server_stopped`, `request_received`, `request_completed`, `worker_pool_resized`, `migration_applied`, `batch_started`, `batch_completed`. Anything else at `info` → audit warning; reviewer must justify or downgrade to `debug`. |
| L5 | "Operator needs the full stack trace in `msg=`" | `msg=` capped at 300 chars; total fields per record capped at 15. Stack traces emit as a separate `event=stack_trace` record at `debug` level, correlated by `correlation_id`, not stuffed into `msg`. |
| L6 | "Embedded newlines because I copy-pasted output" | Audit greps for raw newline byte inside quoted logfmt values. Must be `\n` literal (two chars). |
| L7 | "Auto-mode is on, the gate block is ceremony" | **Auto-mode does NOT cover gate skips.** Skip without an explicit user-given override = automatic violation. No size threshold lets an edit bypass the gate. |
| L8 | "I read this doc at session start; subsequent edits don't need re-print" | Gate fires **per-edit**. The printed `🚧 LOGGING GATE` block is required before every triggered Edit/Write, not once per session. |
| L9 | "Fix-pass touches every line; printing per-line is noise" | Fix-pass produces **one combined gate block per file**, not per-line. The block lists all violations addressed in that file. Still required, just consolidated. |

These are enforced by `logging.sh` (mechanical) and the dispatch façade (`dispatch/write_any.md`, Logging Gate — output discipline). When in conflict, the façade wins — it is the enforcement layer.

## §11 · Anti-patterns (named, banned)

| Pattern | Why banned |
|---|---|
| Free-form English log lines | Not greppable, not parseable, no event tag. |
| `std.debug.print` outside tests | No level, no scope, no fields — dev debugging that escaped to main. |
| `console.log` in `agentsfleet` source | Bypasses logger, breaks `--json` mode, violates `dispatch/write_ts_adhere_bun.md` §10. |
| Logging on hot per-iteration paths at `info` | Floods collectors. Use `debug` (gated off by default). |
| `error_code=` missing on `err`/`warn` mapping to registry codes | Breaks traceability; future operator can't link log to docs. |
| Logging credentials, tokens, or BYOK keys | Severe leak surface. Redaction harness mandatory. |
| Multi-line records (embedded `\n` in values) | Breaks `tail -f`, breaks Loki, breaks every parser. |
| `key=null` or `key=` (empty value) | Confuses parsers; some treat as missing, others as the literal string `null`. Omit the key. |
| Inventing severity levels (`notice`, `severe`, `fatal`) | Five levels only. New names diverge from collector dashboards. |

## §12 · Override syntax

Per-record override (rare, user-only):

```
LOGGING RULE §<N>: SKIPPED per user override (reason: ...)
```

immediately preceding the edit. Generic "scope creep" is not a valid reason — name a concrete external constraint (third-party library log shape, vendored code, pre-launch debugging that won't ship). Auto-mode does NOT cover this override.

## §13 · Family

- `dispatch/write_ts_adhere_bun.md` §10 — banned `console.log` in TS/JS source. Cross-referenced by `logging.sh`.
- `dispatch/write_ts_adhere_bun.md` §9 — module-level error style (throw vs Result) for `agentsfleet`.
- `dispatch/write_zig.md` — Zig discipline umbrella; this doc's §7 is the logging-specific layer.
- `LIFECYCLE_PATTERNS.md` — orthogonal: ownership/cleanup of structs, including allocator wiring for the thread-local log buffer.
- M42_002 redaction harness (`src/runner/engine/runner_progress.zig`) — secret-redaction precondition; this doc's §6 inherits.
- Universal rules (RULE UFS, RULE TGU, RULE PRI, RULE FLL, RULE ORP, RULE TST-NAM) live in `docs/greptile-learnings/RULES.md`.
- Length caps in `dispatch/write_any.md` (File & Function Length).
- This file is the **wire-format and discipline contract** that those universal rules cannot express. Read it once at session start; re-read on sub-task shape change.
