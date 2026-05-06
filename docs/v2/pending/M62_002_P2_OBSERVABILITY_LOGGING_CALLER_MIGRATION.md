# M62_002: Observability — `obs.scoped` caller migration tail

**Prototype:** v0.33.x
**Milestone:** M62
**Workstream:** 002
**Date:** May 06, 2026: 04:50 PM
**Status:** PENDING
**Priority:** P2 — discipline framework already shipped in M62_001; this is mechanical sweep with no behavior change.
**Categories:** Observability, Internal
**Batch:** —
**Depends on:** M62_001 (LOGGING_STANDARD, obs.scoped API, audit-logging.sh — all shipped).

<!--
SPEC AUTHORING RULES (load-bearing — do not delete):
- No time/effort/hour/day estimates anywhere in this spec.
- No effort columns, complexity ratings, percentage-complete, implementation dates.
- No assigned owners — use git history and handoff notes.
- Priority (P0/P1/P2) is the only sizing signal. Use Dependencies for sequencing.
- If a section below contradicts these rules, the rule wins — delete the section.
- Enforced by SPEC TEMPLATE GATE (`docs/gates/spec-template.md`) and `scripts/audit-spec-template.sh`.
- See docs/TEMPLATE.md "Prohibited" section above for canonical list.
-->

## Implementing agent — read these first

1. `docs/LOGGING_STANDARD.md` §7 (Zig binding) and §10A (tightenings) — defines what the migrated form looks like.
2. `src/observability/logging.zig` — the `obs.scoped(.tag).<level>(event, .{...})` API + the `fatalStderr` helper. Mirror this style.
3. `scripts/audit-logging.sh` — `--strict` mode promotes the 91 INFO findings to BLOCK. Use it as the completeness signal: when `--strict` reports 0 findings (excluding the documented `src/auth/` carve-out), the milestone is done.
4. M62_001 commit `fafec799` (reverted via `e0f21914`) shows the per-call mapping pattern on `src/auth/jwks.zig` — same shape applies to every other caller.

## Applicable Rules

- `docs/greptile-learnings/RULES.md` — universal repo discipline.
- `docs/ZIG_RULES.md` — Zig discipline (length, pub, errdefer).
- `docs/LOGGING_STANDARD.md` — wire format and §10A.L2 (no `std.log.scoped` outside `src/observability/`).
- `docs/BUN_RULES.md` §10 — for any `console.*` migration in `zombiectl/src/**`.

## Overview

**Goal (testable):** `bash scripts/audit-logging.sh --strict --all` exits 0 (zero blocking findings) for every Zig file outside `src/auth/**` (carved out per LOGGING_STANDARD §12A).

**Problem:** M62_001 shipped the new `obs.scoped` API and the audit, but ~91 source files (and ~427 individual call sites) still call the legacy `std.log.scoped(.tag)`. The wire format is correct (the custom `logFn` in `main.zig` already wraps everything in logfmt), but the call-site discipline diverges from `LOGGING_STANDARD §7`. New code lands compliant via the gate; existing code drifts until migrated.

**Solution summary:** Mechanical migration of every `std.log.scoped(.tag)` call site outside `src/auth/**` to `obs.scoped(.tag)`. Each file: add `const obs = @import("...../observability/logging.zig");`, swap `const log = std.log.scoped(.tag);` to `const log = obs.scoped(.tag);`, convert each `log.<level>("fmt {s} {d}", .{a, b})` to `log.<level>("event_name", .{ .key = a, .key = b })`. Event names are snake_case verb_noun derived from the existing message prefix.

## Files Changed (blast radius)

| Directory | Files (approx) | Action | Why |
|---|---|---|---|
| `src/cmd/**` | ~15 | EDIT | startup, serve, worker, doctor, migrate flow log calls |
| `src/db/**` | ~6 | EDIT | DB query / connection / migration logs |
| `src/executor/**` | ~12 | EDIT | executor lifecycle, runner, transport, lease |
| `src/http/handlers/**` | ~25 | EDIT | request handlers per route |
| `src/secrets/**` | ~3 | EDIT | crypto/vault logs |
| `src/zombie/**` | ~20 | EDIT | event loop, pool, metering, notifications |
| `src/observability/**` (non-logging) | ~5 | EDIT | otel exporter, telemetry collector |
| `src/config/**`, `src/cli/**`, etc. | ~5 | EDIT | misc utilities |
| `src/auth/**` | 0 | SKIP | portability island per LOGGING_STANDARD §12A |
| `zombiectl/src/**` | ~6 | EDIT | TS files using `console.*` (if any survive); JSON error shape (deferred from M62_001) |

Final tally: ~91 files outside `src/auth/`. The audit reports the exact list via `bash scripts/audit-logging.sh --all 2>&1 | grep INFO`.

## Sections

### §1 — Migration sweep (one section per directory)

For each directory in the blast-radius table, in order:
1. Identify caller files via `grep -rE 'std\.log\.scoped' <dir> --include='*.zig'`.
2. Migrate each file (mechanical per the M62_001 jwks.zig pattern).
3. Build (`zig build`).
4. Test (`make test-unit-zombied`).
5. Commit per directory (one commit per ~10 files).

Order: cmd → db → executor → http → secrets → zombie → observability internals → config/cli → zombiectl. The order is roughly leaf-first (deps don't matter — std.log.scoped and obs.scoped are call-compatible, so partial migration is safe).

### §2 — `--strict` audit gate

After all directories migrate, run `bash scripts/audit-logging.sh --strict --all`. Must exit 0. Auth/ findings remain (carved out); the audit's auth-skip already excludes them.

### §3 — Documentation

- Update LOGGING_STANDARD.md §7 example with a real call site (currently the example is illustrative; replace with a real one from the migrated tree for traceability).
- Add a CHANGELOG entry under "Internal" — no user-visible behaviour change.

## Test Specification

| Test | Asserts |
|---|---|
| `audit-logging.sh --strict --all exits 0 (excluding auth/)` | Every non-auth Zig file under `src/**` uses `obs.scoped`, not `std.log.scoped`. |
| `make test-unit-zombied passes` | Existing tests still pass after the call-site migrations. Wire format change (msg prefix `event=NAME` instead of `NAME ...`) doesn't break any assertion that asserts on log message content. If it does, the assertion is updated in the same commit. |
| `make test-integration passes` | Integration tests pass. |
| `make memleak passes` | No new leaks from the per-call thread-local buffer in `obs.scoped`. |

## Out of scope

- `src/auth/**` migration — carved out per LOGGING_STANDARD §12A. Tracked separately via either a named-module promotion of `logging.zig` or a portable shim under `src/auth/log.zig`. Not in M62_002.
- Any new gates / standards / API changes — M62_001 closed the design surface; M62_002 is execution only.
- `zombiectl` JSON error shape — see M62_001 closing notes for status; if not closed there, surfaces as M62_003.

## Why now / why later

- **Why later (after M62_001 ships):** the framework + tooling lands in M62_001 as one coherent reviewable change. The migration tail is mechanical residue that benefits from a fresh PR scope, separate review, separate revert surface.
- **Why now (after M62_001 ships):** every commit to a non-migrated file is a compounding miss — new code may land matching the legacy pattern via copy-paste, increasing the tail. Knock the migration out before the file count grows further.
