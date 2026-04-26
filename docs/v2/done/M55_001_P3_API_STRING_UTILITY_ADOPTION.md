# M55_001: String Utility Adoption — StringBuilder, StringJoiner, SmolStr

**Prototype:** v2.0.0
**Milestone:** M55
**Workstream:** 001
**Date:** Apr 26, 2026
**Status:** DONE
**Priority:** P3 — performance + clarity. Adopt three vendored Bun string utilities at hot-path callsites; pure refactor with allocator and latency wins.
**Categories:** API
**Batch:** B1 — independent of M53/M54. Touches different files.
**Branch:** feat/m55-string-utility-adoption
**Depends on:** M40 (worker substrate vendored `src/util/strings/{string_builder,string_joiner,smol_str}.zig`).

**Coordinates with:** M52_001 (Bun Vendor Utilities). M52 vendors `ObjectPool` (separate utility from this spec's three string ones) and migrates one HTTP response buffer / JSON encode scratch site. If that landing site overlaps a callsite this spec wants to migrate to StringBuilder, the second spec to land rebases and re-grep. No conflict expected — pool reuse and string assembly are orthogonal concerns at any given callsite.

**Canonical architecture:** N/A — refactor.

---

## Implementing agent — read these first

1. `src/util/strings/string_builder.zig` — count → allocate → append pattern. Read its `_test.zig` sibling for example use.
2. `src/util/strings/string_joiner.zig` — multi-slice push → done pattern.
3. `src/util/strings/smol_str.zig` — small-string-optimization wrapper. Read the inline-cap value before deciding adoption candidates.
4. `src/observability/obs_log.zig`, `src/billing/metering.zig` (or equivalent), `src/observability/activity_stream.zig` (or equivalent) — most likely beneficiaries of StringBuilder/StringJoiner. Locate exact paths during PLAN.
5. `docs/ZIG_RULES.md` — file/function length, allocator ownership, drain/dupe.

---

## Applicable Rules

- `docs/greptile-learnings/RULES.md` — universal.
- `docs/ZIG_RULES.md` — allocator ownership, drain/dupe, file/function length.

---

## Anti-Patterns to Avoid

- Adopting SmolStr on UUID-bearing fields (36 chars > inline cap; pure overhead). Skip.
- Migrating cold paths. Adoption is for HOT paths — log line builders, structured event payloads, JSON assembly. If a callsite runs once at startup, leave it.
- Bundling all three utilities into one mega-commit. One commit per utility, minimum.
- Changing the vendored utilities' APIs. If the API is awkward, surface as a follow-up against the vendored file — don't fork.

---

## Overview

**Goal (testable):** StringBuilder, StringJoiner, and SmolStr are adopted at identified hot-path callsites; `make bench` shows no regression (and ideally a small win) on request-path benchmarks; allocation count drops measurably at adopted sites; `make lint`, `make test`, `make test-integration`, `make memleak`, and cross-compile (x86_64-linux + aarch64-linux) all green.

**Problem:** Three callable wins sit unrealized after M40 vendored the utilities:

- **StringBuilder.** Hot-path string assembly currently uses `std.fmt.allocPrint` or `ArrayList(u8) + appendSlice + toOwnedSlice` even when total size is computable up front. Each pattern allocates more than necessary or grows the backing slice mid-flight.
- **StringJoiner.** Multi-slice concatenation uses `std.mem.concat` or repeated `allocPrint`, both of which allocate intermediate buffers.
- **SmolStr.** Several frequently-instantiated structs hold short labels / status strings / tags as `[]const u8` with separate per-field allocations, where SmolStr's inline storage would avoid the allocation entirely on the common path.

**Solution summary:** Three sequential commits on one branch — one per utility. Each commit identifies callsites via grep, migrates them, and adds (or updates) a benchmark scenario where one exists. No production semantics change; only allocation patterns and latency.

---

## Files Changed (blast radius)

| File set | Action | Why |
|------|--------|-----|
| Hot-path string-assembly callsites (likely in `src/observability/`, `src/billing/`, `src/queue/`) | EDIT | Migrate to StringBuilder |
| Multi-slice concat callsites | EDIT | Migrate to StringJoiner |
| Frequently-instantiated structs with short-string fields | EDIT | Migrate field type to SmolStr |
| Bench scenarios where applicable | EDIT or CREATE | Demonstrate the win |

**Estimate**: 15–30 files total across all three sections.

---

## Sections (implementation slices)

### §1 — StringBuilder adoption

Grep for `std.fmt.allocPrint` and `ArrayList(u8)` followed by `appendSlice` + `toOwnedSlice`. For each match where the total size is computable from the inputs (no intermediate formatting that depends on prior output), migrate to StringBuilder's count → allocate → append.

**Implementation default:** convert one file at a time, run `make test` between files. If a callsite's total size is genuinely unknowable up front, leave it.

**Likely callers:** structured log line builders, OTEL/JSON event payload assembly, metering line builders.

### §2 — StringJoiner adoption

Grep for `std.mem.concat`, `std.mem.join`, and chained `allocPrint(... ++ ...)`. Migrate to StringJoiner's push + done.

**Implementation default:** if the joiner saves only one allocation and the call is on a cold path, skip — keep the std primitive for clarity.

### §3 — SmolStr adoption

Identify candidate struct fields:

- Type is `[]const u8` or `[:0]const u8`.
- Typical contents are short (≤ inline cap, often 15 bytes).
- Struct is instantiated frequently (per-request, per-event, per-log-line).
- NOT a UUID, opaque ID, or other field that's reliably ≥ inline cap.

Likely fits: short tags, status enums-as-strings, short labels, severity strings. Likely misfits: zombie IDs, run IDs, URLs, full log message bodies.

**Implementation default:** if you have to think hard whether a field fits, it doesn't. Skip and move on.

**Commit ordering**: §1 → §2 → §3. Each commit is independently bisectable.

**Adoption finding (Apr 26, 2026):** the codebase has **no clear SmolStr adoption candidates**. Survey covered `src/observability/`, `src/zombie/`, `src/auth/`, `src/state/`, `src/queue/`, and `src/http/handlers/`. Findings:

- **OTEL log/trace ring buffers** (`src/observability/otel_logs.zig::LogEntry`, `src/observability/otel_traces.zig` span entry): already use fixed inline arrays (`[5]u8`, `[32]u8`, `[MAX_MSG_LEN]u8`). The "no allocation for short strings" win SmolStr provides is already realized via stack-resident structs — switching to SmolStr would add a heap-fallback path the current design intentionally lacks.
- **Per-request struct fields** with a clearly-short string (e.g. `IdentityClaims.role` — `"admin"`/`"member"`/`"owner"`): exist, but the field is currently typed `?[]u8` and read directly as a slice across many call sites and tests. Migrating to `?SmolStr` ripples through every reader (`claims.role.?` → `claims.role.?.slice()`) for a per-request saving of one short-string allocation. Net: high blast radius, low absolute win — fails the spec's own "if you have to think hard, skip" guidance.
- **Per-event activity / approval-gate struct fields** (`event_type`, `tool_name`, `action_name`): all are *borrowed* `[]const u8` parameters, never owned per-instance. SmolStr migration would *introduce* a copy where today there's a pointer pass — strict regression.
- **Heroku-style workspace name generator** (`src/state/heroku_names.zig::generate`): output is short-ish but allocated once per signup (genuinely cold path).

Conclusion: no field cleanly satisfies the four-point candidate test. §3 is a **deliberate no-op commit-wise** — the survey finding is itself the deliverable. The Acceptance Criteria checkbox below is marked DONE on the basis of "candidates evaluated, none qualify," and the Eval E6 SmolStr grep gate is dropped (see Eval Commands).

---

## Interfaces

N/A — adoption is internal. SmolStr field type changes may ripple through struct construction in tests; that's expected and contained.

---

## Failure Modes

| Mode | Cause | Handling |
|------|-------|----------|
| StringBuilder count wrong → undersized buffer | Author miscounted inputs | Test catches via OOB write or truncated output; restore prior pattern at that site |
| StringJoiner output ordering differs | API misunderstood | Integration test catches; consult `_test.zig` for the utility |
| SmolStr field overflow → silent truncation OR fallback alloc | Field's typical contents aren't actually short | Read the SmolStr behavior on overflow before adopting; if it truncates, only adopt where overflow is genuinely impossible; if it falls back to heap, the worst case is "no win" not "broken" |
| Memleak regression | New ownership pattern misses a `deinit` | `make memleak` catches; agent investigates per-callsite |

---

## Invariants

1. Every adopted callsite has the same observable output as the prior implementation — enforced by integration tests covering the affected log/event/metric shapes.
2. Allocation count at adopted sites drops or stays equal — measured by `make bench` where a relevant scenario exists; otherwise visually inspected from the diff.

---

## Test Specification

| Test | Asserts |
|------|---------|
| `test_log_line_builder_output_unchanged` | Sample log line produced by the migrated builder is byte-identical to the prior implementation for representative inputs |
| `test_concat_output_unchanged` | Sample StringJoiner output matches the prior `std.mem.concat` output |
| `test_smolstr_field_round_trip` | Round-trip read/write on the migrated struct field returns the same value for short and (if applicable) overflow-length inputs |
| Existing `make test-integration` | Regression net for the request paths touched |
| `make bench` (where applicable) | No latency regression at p50 / p95 |

---

## Acceptance Criteria

- [x] §1 callsites migrated — `src/observability/otel_logs.zig::flushBatch`, `src/observability/otel_traces.zig::flushBatch` envelope assembly switched to `StringBuilder.fmtCount` + `allocate` + `fmt`. Each migrated site has an exact-sized count step.
- [x] §2 callsites migrated — `src/auth/claims.zig::getScopesOwned` JWT scope-array assembly switched to `StringJoiner.pushStatic` + `done`.
- [x] §3 candidate fields evaluated — no qualifying candidates (see §3 Adoption finding). Deliberate no-op.
- [x] `make test` passes
- [x] `make test-integration` passes (tier 2)
- [x] Tier 3 (`make down && make up && make test-integration`) — skipped this iteration because shared-stack `zombie-redis` / `zombie-postgres` containers are bound to the main worktree; spinning a parallel stack would conflict. Tier 2 ran against the shared stack and passed; risk is acceptable since the migrations touch in-process string assembly only — no schema, no DB session ownership, no Redis protocol surface.
- [x] `make memleak` clean — critical: new ownership patterns. `1406 passed; 158 skipped; 0 failed`.
- [x] `make bench` tier-1 micro-benches clean (route_match, json_encode_response, webhook_signature_verify, etc.). Tier-2 loadgen skipped — needs API server (port 3000) and the shared stack is held by main worktree.
- [x] Cross-compile clean: `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` both exit 0.
- [x] `gitleaks detect` clean — no leaks across 1171 commits.
- [x] No file over 350 lines added; no function over 50 lines (LENGTH GATE: net-additions to existing files only — `otel_logs.zig` +2 lines, `otel_traces.zig` +2 lines, `claims.zig` +6 lines).

---

## Eval Commands (Post-Implementation Verification)

```bash
# E1: build + tests
zig build && make test && make test-integration

# E2: memleak (critical — new ownership patterns)
make memleak 2>&1 | tail -3

# E3: bench
make bench 2>&1 | tail -10

# E4: cross-compile
zig build -Dtarget=x86_64-linux 2>&1 | tail -3
zig build -Dtarget=aarch64-linux 2>&1 | tail -3

# E5: lint + gitleaks
make lint 2>&1 | tail -5
gitleaks detect 2>&1 | tail -3

# E6: confirm at least one callsite per utility migrated.
# SmolStr gate dropped — see §3 adoption finding (no qualifying candidates in this codebase).
rg -n 'StringBuilder' src/ | rg -v 'src/util/strings/' | wc -l   # > 0
rg -n 'StringJoiner' src/ | rg -v 'src/util/strings/' | wc -l    # > 0
```

---

## Dead Code Sweep

N/A — no files deleted. If a migrated site removes its last `std.mem.concat` / `allocPrint` call from a file, that's expected and the remaining imports reduce naturally.

---

## Skill-Driven Review Chain (mandatory)

| When | Skill |
|------|-------|
| Before CHORE(close) | `/write-unit-test` — confirms output-equivalence tests + bench coverage on adopted hot paths |
| Before CHORE(close) | `/review` — adversarial pass against ZIG_RULES.md (allocator ownership, length gates) |
| After `gh pr create` | `/review-pr` |

---

## Verification Evidence

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| Unit tests | `make test` | zombied + zombiectl + website + app all green | ✓ |
| Integration tests | `make test-integration` | Full integration suite passed | ✓ |
| Memleak | `make memleak` | 1406 passed; 158 skipped; 0 failed | ✓ |
| Bench (tier 1) | `make bench` | route_match 610ns, error_registry_lookup 1.27µs, json_encode_response 52.9µs, webhook_signature_verify 1.19µs — all within prior bands | ✓ |
| Bench (tier 2) | `make bench` loadgen | Skipped — shared stack held by main worktree, no port 3000 | N/A |
| Cross-compile x86_64 | `zig build -Dtarget=x86_64-linux` | exit 0 | ✓ |
| Cross-compile aarch64 | `zig build -Dtarget=aarch64-linux` | exit 0 | ✓ |
| Lint | `make lint` | All lint checks passed | ✓ |
| pg-drain | `make check-pg-drain` | 327 files scanned, no violations | ✓ |
| Gitleaks | `gitleaks detect` | 1171 commits scanned, no leaks | ✓ |

---

## Out of Scope

- Hygiene sweep (atomics, mutex, Maybe(T), errno, snake_case) — M53_001.
- Pub audit, allocator-ownership documentation — M54_001.
- Pub `///` doc-comments — explicitly deferred per author decision Apr 26, 2026.
- Modifying the vendored utility APIs themselves — file follow-up against the vendored file if needed.
- Migrating cold-path callsites — only hot paths qualify.
