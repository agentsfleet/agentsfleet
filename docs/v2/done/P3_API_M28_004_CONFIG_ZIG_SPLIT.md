# M28_004: Split `src/zombie/config.zig` into per-concern modules

**Prototype:** v0.19.0
**Milestone:** M28
**Workstream:** 004
**Date:** Apr 18, 2026
**Status:** DONE
**Priority:** P3 — deferrable; no user-visible behavior change
**Batch:** B1
**Branch:** feat/m28-config-split
**Depends on:** M28_001 (webhook auth; last addition that pushed the file over the 350-line cap)

---

## Overview

**Goal (testable):** `src/zombie/config.zig` (currently 448 lines, with a 104-line `parseZombieConfig` method violating the 50-line method cap) is split along a **lifecycle axis** (types / parser / markdown / validate) so no single file exceeds 350 lines and no method exceeds 50 lines. Zero behavior change. Public API preserved via a thin façade so no caller updates its imports.

**Why deferred from M28_001.** M28_001 §3 added 17 lines (`WebhookSignatureConfig` + `InvalidSignatureConfig` + deinit wiring) to a file already at 431 lines. Splitting at that point would have ballooned the M28_001 diff with a mechanical refactor unrelated to the security delivery. The violation was documented in the M28_001 Ripley's Log and deferred here for clean history.

**Why lifecycle axis, not type-family axis (revises original proposal).** The original spec proposed splitting by type-family (trigger/skill/budget). That doesn't match the code: `ZombieBudget` is 3 lines and `skill` is `?[]const u8` — those "modules" would be near-empty. The real bulk is **parsing logic**, and `parseZombieConfig` itself violates the 50-line method cap. A lifecycle split (types / parser / markdown / validate) produces balanced, single-concern modules and forces decomposition of the 104-line orchestrator.

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `src/zombie/config.zig` | MODIFY | Shrink to ≤80L façade: re-exports + thin wrappers delegating to parser/markdown/validate. Preserves all caller import paths. |
| `src/zombie/config_types.zig` | CREATE | `ZombieConfigError`, `ZombieStatus`, `ZombieTriggerType`, `MAX_SIGNATURE_HEADER_LEN`, `WebhookSignatureConfig`, `ZombieTrigger` (union), `ZombieBudget`, `ZombieNetwork`, `ZombieConfig` struct + `deinit`. |
| `src/zombie/config_parser.zig` | CREATE | `parseZombieConfig` decomposed into per-field helpers (`parseNameField`, `parseTriggerField`, `parseSkillsField`, `parseCredentialsField`, `parseNetworkField`, `parseBudgetField`, `parseGatesField`, `parseExtendedFields`). Each ≤ 50 lines. |
| `src/zombie/config_markdown.zig` | CREATE | `extractZombieInstructions` + `parseZombieFromTriggerMarkdown` + shared frontmatter delimiter scanner. |
| `src/zombie/config_validate.zig` | CREATE | `validateSkillsAndCredentials` + `validateZombieSkills` + credential charset constants. |
| `src/zombie/config_types_test.zig` | CREATE | Tests for `ZombieStatus.toSlice`/`fromSlice`/`isTerminal`/`isRunnable`. |
| `src/zombie/config_parser_test.zig` | CREATE | Tests for `parseZombieConfig` happy-path + missing-field + invalid-trigger + skill-field cases. |
| `src/zombie/config_markdown_test.zig` | CREATE | Tests for `extractZombieInstructions` + `parseZombieFromTriggerMarkdown`. |
| `src/zombie/config_validate_test.zig` | CREATE | Tests for unknown skill + invalid credential ref. |
| `src/main.zig` | MODIFY | Add `_ = @import(...)` lines for the four new `*_test.zig` files. |
| `src/zombie/config_helpers.zig` | UNCHANGED | Already in cap (319L). Out of scope. |
| `src/zombie/config_gates.zig` | UNCHANGED | Already in cap (315L). Out of scope. |

---

## Applicable Rules

- RULE FLL — Files ≤ 350 lines, methods ≤ 50 lines.
- RULE XCC — Cross-compile x86_64-linux + aarch64-linux before commit.
- RULE ORP — Cross-layer orphan sweep after moves.
- RULE TST-NAM — Test filenames + `test "..."` names never embed milestone IDs.
- Struct Init Partial Leak (M6_001) — per-field parser decomposition must preserve `errdefer` chain; no struct literal with multiple `try alloc.dupe` calls in a single expression.

---

## §1 — Lifecycle split

**Status:** PENDING

Extract four modules along a lifecycle axis (types / parse / markdown-frontmatter / validate). `config.zig` becomes a façade that re-exports the types and delegates parse/markdown/validate calls. Caller imports do not change.

**Dimensions (test blueprints):**

| Dim | Status | Target | Input | Expected | Test type |
|-----|--------|--------|-------|----------|-----------|
| 1.1 | DONE | `config_types.zig` | N/A (type module) | File compiles, exports 9 symbols, 137L (with tests split out) | unit |
| 1.2 | DONE | `config_parser.zig:parseZombieConfig` | Valid config JSON | Returns populated `ZombieConfig` identical to pre-split | unit |
| 1.3 | DONE | `config_markdown.zig:extractZombieInstructions` | TRIGGER.md with frontmatter + body | Body slice (borrowed) | unit |
| 1.4 | DONE | `config_validate.zig:validateZombieSkills` | Config with unknown skill | `ZombieConfigError.UnknownSkill` | unit |

## §2 — `parseZombieConfig` decomposition

**Status:** PENDING

The current 104-line `parseZombieConfig` is broken into eight per-field helpers, each ≤ 50 lines. The orchestrator becomes a ~30-line sequence of helper calls threaded with `errdefer` so partial-build failures free all already-duped fields (per ZIG_RULES "Struct Init Partial Leak").

**Dimensions:**

| Dim | Status | Target | Input | Expected | Test type |
|-----|--------|--------|-------|----------|-----------|
| 2.1 | DONE | `config_parser.zig:parseZombieConfig` | Invalid budget after valid skills | Earlier dupes freed; no leak (see `partial-build leak check` test) | unit (leak) |
| 2.2 | DONE | Each per-field helper | awk 50-line gate | All ≤ 50 lines | lint |
| 2.3 | DONE | `parseZombieConfig` orchestrator | awk 50-line gate | 47 lines | lint |

## §3 — Per-module test files

**Status:** PENDING

Every new module gets a sibling `_test.zig`. Tests currently inline in `config.zig` migrate to the appropriate module's test file. `main.zig` test discovery block updated.

**Dimensions:**

| Dim | Status | Target | Input | Expected | Test type |
|-----|--------|--------|-------|----------|-----------|
| 3.1 | DONE | `config_{types,parser,markdown,validate}_test.zig` | `zig build test` | All tests run; assertion count ≥ pre-split (tests relocated + new leak + cron/api/empty-body cases added) | integration |
| 3.2 | DONE | Test filenames | grep `M[0-9]` in test filenames + `test "..."` strings | 0 matches | lint |

---

## Interfaces

**Status:** PENDING

### Public surface preserved on `config.zig` (façade)

```zig
// Re-exports (types)
pub const ZombieConfigError = config_types.ZombieConfigError;
pub const ZombieStatus = config_types.ZombieStatus;
pub const ZombieTriggerType = config_types.ZombieTriggerType;
pub const ZombieTrigger = config_types.ZombieTrigger;
pub const WebhookSignatureConfig = config_types.WebhookSignatureConfig;
pub const MAX_SIGNATURE_HEADER_LEN = config_types.MAX_SIGNATURE_HEADER_LEN;
pub const ZombieBudget = config_types.ZombieBudget;
pub const ZombieNetwork = config_types.ZombieNetwork;
pub const ZombieConfig = config_types.ZombieConfig;
pub const GateBehavior = config_gates.GateBehavior;
pub const GateRule = config_gates.GateRule;
pub const AnomalyPattern = config_gates.AnomalyPattern;
pub const AnomalyRule = config_gates.AnomalyRule;
pub const GatePolicy = config_gates.GatePolicy;

// Delegating wrappers
pub const parseZombieConfig = config_parser.parseZombieConfig;
pub const parseZombieFromTriggerMarkdown = config_markdown.parseZombieFromTriggerMarkdown;
pub const extractZombieInstructions = config_markdown.extractZombieInstructions;
pub const validateZombieSkills = config_validate.validateZombieSkills;
```

All signatures identical to pre-split.

### Error contracts

Unchanged — `ZombieConfigError` enum is moved but its variants and their trigger conditions are preserved exactly.

---

## Implementation Constraints (Enforceable)

| Constraint | How to verify |
|-----------|---------------|
| `config.zig` ≤ 80L | `wc -l src/zombie/config.zig` |
| Every new file ≤ 250L | `wc -l src/zombie/config_{types,parser,markdown,validate}*.zig` |
| Every method ≤ 50L | Awk pass on `fn ` → next `^}` depth-0 line |
| Cross-compiles x86_64-linux + aarch64-linux | `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` |
| `std.testing.allocator` detects zero leaks | `zig build test` clean |
| Orphan sweep: façade + all callers still resolve | `grep -rn "zombie_config\.\|@import.*zombie/config" src/ --include='*.zig'` → identical shape pre/post |

---

## Invariants (Hard Guardrails)

| # | Invariant | Enforcement mechanism |
|---|-----------|----------------------|
| 1 | Every new `.zig` file appears in `main.zig` test discovery | Test compile/run — missing file means no tests fire, caught by assertion count check |
| 2 | Façade re-export names match originals exactly | Compile failure in any caller if drift |
| 3 | `parseZombieConfig` preserves errdefer chain | Leak test with intentionally malformed late-field input |

---

## Test Specification

### Unit Tests (migrated from inline tests in config.zig)

| Test name | Dim | Target | Input | Expected |
|-----------|-----|--------|-------|----------|
| `valid config parses all fields` | 1.2 | `config_parser.zig:parseZombieConfig` | Full webhook config JSON | All fields populated |
| `missing name returns MissingRequiredField` | 2.1 | same | JSON without `name` | `MissingRequiredField` |
| `invalid trigger type returns InvalidTriggerType` | 2.1 | same | JSON with `trigger.type: "invalid"` | `InvalidTriggerType` |
| `skill field parsed from JSON` | 1.2 | same | Chain trigger + skill ref | `cfg.skill` non-null |
| `credential names validated (no op:// paths)` | 1.4 | `config_validate.zig` | credentials: `["op://..."]` | `InvalidCredentialRef` |
| `parses frontmatter into config` | 1.3 | `config_markdown.zig:parseZombieFromTriggerMarkdown` | YAML frontmatter + body | Populated config |
| `no frontmatter returns error` | 1.3 | same | Plain text | `MissingRequiredField` |
| `returns body after frontmatter` | 1.3 | `config_markdown.zig:extractZombieInstructions` | MD with `---` block | Trimmed body |
| `no frontmatter returns empty` | 1.3 | same | Plain markdown | `""` |
| `unknown skill returns UnknownSkill` | 1.4 | `config_validate.zig:validateZombieSkills` | skills: `["unknown_tool"]` | `UnknownSkill` |

### Leak Detection Tests

| Test name | Dim | What it proves |
|-----------|-----|---------------|
| `parseZombieConfig frees partial state on late failure` | 2.1 | Invalid `gates` field after valid skills/credentials — no leak |

### Regression Tests

All existing callers under `src/http/handlers/`, `src/cmd/worker_zombie.zig`, `src/cmd/doctor.zig`, `src/zombie/event_loop*.zig` continue to compile and pass their test suites without edits.

### Spec-Claim Tracing

| Spec claim | Test that proves it | Test type |
|------------|-------------------|-----------|
| "Zero behavior change" | Full test suite passes with identical assertion count | integration |
| "All caller imports unchanged" | No diff in `src/**/*.zig` other than `src/zombie/config*.zig` + `src/main.zig` | grep |

---

## Execution Plan (Ordered)

| Step | Action | Verify |
|------|--------|--------|
| 1 | CHORE(open): branch + worktree + spec pending→active | `git worktree list` shows worktree; `ls docs/v2/active/*M28_004*` |
| 2 | Create `config_types.zig`; move types; leave re-exports in `config.zig` | `zig build` passes |
| 3 | Create `config_validate.zig`; move validators; façade re-exports | `zig build` passes |
| 4 | Create `config_markdown.zig`; move frontmatter fns; façade delegates | `zig build` passes |
| 5 | Create `config_parser.zig`; decompose `parseZombieConfig` into per-field helpers with errdefer chain | `zig build && zig build test` passes |
| 6 | Create per-module `*_test.zig` files; remove inline tests from `config.zig`; update `main.zig` discovery | `zig build test` — assertion count ≥ pre-split |
| 7 | Shrink `config.zig` to pure façade (re-exports + `pub const fn = other_mod.fn`) | `wc -l src/zombie/config.zig` ≤ 80 |
| 8 | VERIFY gates: lint, cross-compile, 350L/50L, gitleaks, orphan sweep | All green |
| 9 | CHORE(close): changelog `<Update>` (Internal tag), Ripley log, spec active→done, PR | All gates, PR opened |

---

## Acceptance Criteria

- [ ] `src/zombie/config.zig` ≤ 80 lines. Verify: `wc -l src/zombie/config.zig`
- [ ] Every new `*.zig` file ≤ 250 lines. Verify: `wc -l src/zombie/config_*.zig`
- [ ] Every method in touched files ≤ 50 lines. Verify: awk scan.
- [ ] No caller outside `src/zombie/config*.zig` / `src/main.zig` requires edits. Verify: `git diff --stat` scope.
- [ ] `zig build test` passes. Verify: `make test`.
- [ ] `make test-integration` passes. Verify: env + command.
- [ ] `make memleak` passes (baseline — no allocator wiring changed, but confirm). Verify: command.
- [ ] Cross-compile clean. Verify: `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux`.
- [ ] `make lint` clean. Verify: command.
- [ ] `gitleaks detect` clean. Verify: command.
- [ ] No test filename or `test "..."` string contains `M[0-9]`, `§`, or workstream IDs. Verify: `grep -rn 'M[0-9]' src/zombie/config_*_test.zig`.

---

## Eval Commands

```bash
# E1: Build
zig build 2>&1 | tail -5; echo "build=$?"

# E2: Tests
zig build test 2>&1 | tail -5; echo "test=$?"

# E3: 350L file gate
wc -l src/zombie/config*.zig | awk '$1 > 350 && $2 != "total" { print "OVER: " $2 ": " $1 }'

# E4: 50L method gate (rough)
awk '/^(pub )?fn / { n=$0; c=0 } /^}$/ { if (c>50) print FILENAME":"NR": method "c" lines: "n; n="" } n { c++ }' src/zombie/config*.zig

# E5: Cross-compile
zig build -Dtarget=x86_64-linux 2>&1 | tail -3; echo "x86=$?"
zig build -Dtarget=aarch64-linux 2>&1 | tail -3; echo "arm=$?"

# E6: Lint + leaks
make lint 2>&1 | grep -E "✓|FAIL"
gitleaks detect 2>&1 | tail -3; echo "gitleaks=$?"

# E7: Orphan sweep (re-exports must preserve all public names)
for sym in ZombieConfig ZombieStatus ZombieTrigger WebhookSignatureConfig parseZombieConfig extractZombieInstructions validateZombieSkills parseZombieFromTriggerMarkdown; do
  echo "== $sym =="
  grep -rn "config\.$sym\|zombie_config\.$sym" src/ --include='*.zig' | wc -l
done

# E8: Test-name hygiene
grep -rn 'M[0-9]\|§' src/zombie/config_*_test.zig; echo "expected: 0 matches"
```

---

## Dead Code Sweep

N/A — no files deleted. Re-exports preserve every public name. Inline tests migrate (not deleted) to per-module `_test.zig`.

---

## Out of Scope

- `config_helpers.zig` (319L, in cap).
- `config_gates.zig` (315L, in cap).
- Any behavioral change — pure mechanical move + method decomposition.
- Renaming public API. Re-exports preserve the surface.
