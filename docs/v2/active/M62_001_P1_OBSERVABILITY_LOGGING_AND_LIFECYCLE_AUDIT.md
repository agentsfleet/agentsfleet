# M62_001: Observability — Logging Standard, Lifecycle Patterns, Error-Surface Audit

**Prototype:** v2.0.0
**Milestone:** M62
**Workstream:** 001
**Date:** May 06, 2026: 11:00 AM
**Status:** IN_PROGRESS
**Branch:** feat/m62-001-observability
**Priority:** P1 — observability discipline gates production debuggability; non-blocking for current launch but blocking for confident incident response.
**Categories:** Observability, Internal
**Batch:** —
**Depends on:** M42_002 (redaction harness — landed; redaction itself is one form of structured-output discipline). Independent of M42_003 (pub/sub-failure non-blocking) and M42_006 (adversarial-zombie test harness). Absorbs M42_008 (logging-call-site tail).

<!--
SPEC AUTHORING RULES (load-bearing — do not delete):
- No time/effort/hour/day estimates anywhere in this spec.
- No effort columns, complexity ratings, percentage-complete, implementation dates.
- Priority (P0/P1/P2) is the only sizing signal. Use Dependencies for sequencing.
- If a section below contradicts these rules, the rule wins — delete the section.
- See docs/TEMPLATE.md "Prohibited" for canonical list.
-->

## Bundled scope amendments (CHORE-open delta from original draft)

1. **Full fix-pass, no top-50 cap.** Every audit-flagged violation across `src/**` + `zombiectl/**` is fixed in this milestone. M42_008 dissolves into M62_001.
2. **Four gates, not three.** Adds **SPEC TEMPLATE GATE** alongside LOGGING, LIFECYCLE, ERROR REGISTRY — catches drift like the "Estimated effort" section that originally appeared in this very spec.
3. **CI Zig-mirror swap bundled.** Two Docker `wget` blocks (`cross-compile.yml`, `release.yml`) move to `pkg.machengine.org` with a fallback chain; `actions/cache` for the Alpine zig tarball + `~/.cache/zig` global package cache; pin `mlugg/setup-zig@v2` to a specific patch tag.
4. **Architecture cross-links.** `docs/architecture/README.md` gains a Conventions section linking the two new standards docs alongside `ZIG_RULES.md`, `BUN_RULES.md`, `REST_API_DESIGN_GUIDELINES.md`, `AUTH.md`. Bidirectional back-links from each standards doc.
5. **No effort estimates.** The original `## Estimated effort` section was a TEMPLATE.md violation and has been removed. SPEC TEMPLATE GATE prevents recurrence.

## Why this is its own workstream

Three related-but-separate audits the Captain surfaced after M42_002 landed. Each could be its own milestone; bundling because they share the same reference codebase (`~/Projects/oss/bun/src`) and because the standard one defines drives the gate the other two enforce.

1. **Logging structure.** Today's logs are ad-hoc `std.log.info("name.event field={s} other={d}", ...)` with no enforced schema. Hard to grep, hard to feed to a structured collector (Loki/Vector/Mezmo), hard for an agent to consume. Captain wants: define the standard today actually follows, define the proposal, gate on every commit.
2. **Init/deinit/defer pattern.** Today's Zig code mixes patterns — sometimes `defer`, sometimes `errdefer`, sometimes manual cleanup, sometimes `arena` resets. Bun's reference codebase has a coherent convention. Captain wants ours measured against theirs.
3. **CLI + error-registry consistency.** `src/errors/error_registry.zig` is the single source of truth for error codes (`UZ-EXEC-012` etc.). Audit whether all surfaces — HTTP handlers, executor frames, `zombiectl` CLI output — actually emit codes from the registry vs. ad-hoc strings.

## Scope

### Files to read (reference, do not edit)

- `~/Projects/oss/bun/src/**/*.zig` — Bun's Zig logging, error, init/deinit conventions.
- `~/Projects/usezombie/src/observability/logging.zig` — current scoped-log helpers.
- `~/Projects/usezombie/src/errors/error_registry.zig` — canonical code list.
- `~/Projects/usezombie/zombiectl/**` — JS CLI, where errors surface to humans.
- `~/Projects/usezombie/AGENTS.md` (symlinked to `~/Projects/dotfiles/AGENTS.md`) — operating model.
- `~/Projects/usezombie/docs/ZIG_RULES.md` (symlinked) — Zig sub-rules.

### Files to add

- `docs/LOGGING_STANDARD.md` — the proposed standard. Lives alongside `ZIG_RULES.md` in dotfiles (symlinked). Format mirrors REST_API_DESIGN_GUIDELINES.md. Sections:
  - **Today's de-facto standard** (a survey-derived description, not aspirational).
  - **Proposed standard.** Required keys per log line: `ts_ms`, `level`, `scope`, `event` (snake_case verb_noun), then arbitrary structured fields. JSON-serializable. Renderable as a single line for `tail -f` *and* parseable by collectors.
  - **Severity contract.** When `info` vs `warn` vs `err`. When to use `debug`. (Today is inconsistent — successful happy-path events are sometimes logged at `info`, sometimes silent.)
  - **Error-code embedding.** `error_code=UZ-XXX-NNN` MUST appear on every `err`/`warn` line that maps to a registry code. Registry-less errors get a follow-up entry to add a code.
  - **PII / secret discipline.** Inherits M42_002's redaction list — same secret values must not appear in log lines (today's redactor doesn't cover stderr; gap to close).
- `docs/LIFECYCLE_PATTERNS.md` — the init/deinit/defer convention. Sections:
  - **Bun's convention** (synthesized from reading Bun's tree).
  - **Our convention** (synthesized from reading ours).
  - **Diff and proposal.** Where to converge, where divergence is intentional.
  - Rules for `errdefer` placement, ownership transfer, `arena.reset` vs per-allocation `defer`, RAII via struct method `deinit`.
- `scripts/audit-logging.sh` — gate that runs in `make lint`. Greps for: `std.log.info("` calls without `ts_ms` or without an `event=` field; `std.log.err` calls without `error_code=`; raw `printf`/`std.debug.print` outside `*_test.zig`. Output table with violations + line numbers.
- `scripts/audit-error-codes.sh` — gate that runs in `make lint`. Greps `src/errors/error_registry.zig` for declared codes, then greps the rest of `src/` and `zombiectl/` for any `UZ-[A-Z]+-[0-9]+` literal that isn't in the registry. Reports orphans (used but not declared) and dead codes (declared but not referenced).
- `scripts/audit-deinit-pairs.sh` — gate. For every `pub fn init(` (or struct returning `Self`), require a matching `pub fn deinit(self:`. Reports pairs that look broken (init exists, deinit missing or vice-versa).
- `docs/gates/logging.md`, `docs/gates/lifecycle.md`, `docs/gates/error-registry.md` — gate body files, mirroring existing `docs/gates/zig.md` etc.

### Files to modify

- `AGENTS.md` (symlink → dotfiles) — add three new gates to the Gate Index: LOGGING GATE, LIFECYCLE GATE, ERROR REGISTRY GATE. Per the "Rule extension protocol" (AGENTS.md §"Rule extension protocol"), each gate addition lands in the same diff as: (a) the gate body file under `docs/gates/`, (b) at least one question in `AGENTS_INVARIANCE.md`, (c) the path entry in the audit script's `DOTFILES_RESIDENT` list, (d) `make audit ALL CHECKS PASSED`.
- `AGENTS_INVARIANCE.md` (symlink → dotfiles) — three new questions, one per gate, asking whether the gate output appeared on the most recent edit.
- `Makefile` — wire the three new audit scripts into `make lint`. Order: lint → audit-logging → audit-error-codes → audit-deinit-pairs.
- `src/observability/logging.zig` — extend the scoped-log helpers to take a struct of structured fields rather than positional `{s} {d}` placeholders. The struct serializes to `key=value` pairs at info-level, JSON at warn/err. (Optional in this milestone if the gate gates on the wire format; mandatory if the convention requires it.)
- Touched files across `src/**/*.zig` — fix **every** violation the new gates flag. No cap. M42_008 absorbed into this milestone.
- `zombiectl/src/**` — every error-surface call site emits `{code: "UZ-XXX-NNN", message, hint?}` JSON when `--json` is set; human format `error UZ-XXX-NNN: <message>` otherwise. Today's surfaces are inconsistent.

### Files NOT to modify

- The redaction adapter (`src/executor/runner_progress.zig`) — its discipline is the model, not the target. M42_002 already brought it to spec.

## Open questions (resolve in PLAN, not deferred)

1. **JSON-on-the-wire vs key=value-on-the-wire.** Bun uses `key=value`. Most modern collectors prefer JSON. The proposal needs to pick one and gate it. JSON is collector-friendly but uglier on tail-f. `key=value` is the inverse. **Recommend:** structured-but-not-JSON line format (`ts_ms=... level=... scope=... event=... k1=v1 k2=v2`), which is grep-friendly AND has off-the-shelf parsers (logfmt). Bun's convention. Defer JSON for collectors to a sidecar transform.
2. **Severity for happy-path events.** Today: inconsistent. Some happy-path emit `info`, some are silent. **Recommend:** silent unless the event has operational significance (request lifecycle bookends OK; per-iteration tool calls noisy → debug). Gate doesn't enforce; document the rule, let the `audit-logging.sh` flag suspicious patterns (info-spam in hot loops).
3. **Error-code orphans in dotfiles.** The error registry is in `usezombie/`, not dotfiles. The audit script lives here. But CLAUDE.md / AGENTS.md, where the gate is documented, lives in dotfiles. Cross-repo coordination needed; settle in PLAN.
4. **CLI exit codes.** Today's `zombiectl` exits 0/1 mostly. POSIX convention has more granularity. Worth standardizing? **Recommend:** out of scope here; track as M42_009 if Captain wants it.

## Out of scope

- Replacing the logging library. `std.log` stays.
- Centralized log shipping / collector setup. This is the runtime story; we're defining the wire format that whatever collector eats.
- Distributed tracing IDs (OpenTelemetry W3C trace context). Already partially landed via `correlation.trace_id`; extending it across every log line is a separate workstream.
- Migrating Zig to a third-party logging package (logz, etc). Not warranted; `std.log` plus a thin wrapper covers it.

## Test Specification

| Test | Asserts |
|---|---|
| `audit-logging.sh on a known-good file passes` | The script applied to `src/observability/logging.zig` after the fix-pass exits 0. |
| `audit-logging.sh on a synthetic violation file fails` | A fixture file with a missing `event=` field, a raw `printf`, and an `err` without `error_code=` triggers all three categories of finding. |
| `audit-error-codes.sh detects orphan code in source` | A fixture file references `UZ-FAKE-999` (not in registry); the script flags it. |
| `audit-error-codes.sh detects dead code in registry` | A registry entry never referenced anywhere is flagged; gate is informational, not blocking, on the dead-code finding (deletion may be deferred). |
| `audit-deinit-pairs.sh detects init without deinit` | A fixture struct with `pub fn init` but no `pub fn deinit` is flagged. |
| End-to-end `make lint` pass on a clean tree | Existing usezombie tree passes, after the fix-pass cleans the top-50 violators. |

## Why now / why later

- **Why now:** post-launch incident response will hinge on log readability. Every day past launch with ad-hoc logging is more drift to clean up later. Gate-on-commit means new code lands compliant from day one; the fix-pass is one-time.
- **Why later (after M42_002 PR ships):** this is a cross-cutting refactor. Doing it in M42_002's PR would balloon scope (the M42_002 PR already grew once during Slice 4 when the redactor leaks surfaced). Better to land observability discipline as its own bounded milestone with its own review gates.

## Captain decisions to confirm during PLAN

A. **logfmt vs JSON wire format** for structured logs. Recommendation: logfmt.
B. **Whether the orphan-code gate is blocking or informational.** Recommendation: blocking on used-but-not-declared, informational on declared-but-not-referenced.
C. **Whether the init/deinit gate is per-struct or per-file.** Recommendation: per-struct, with carve-outs for short-lived inline structs that don't allocate.
D. **Cross-repo gate coordination.** The audit scripts live in `usezombie/`, the gates and AGENTS.md live partly in `dotfiles/`. Acceptable risk profile? Recommendation: yes, with the existing dotfiles-edit ritual.
