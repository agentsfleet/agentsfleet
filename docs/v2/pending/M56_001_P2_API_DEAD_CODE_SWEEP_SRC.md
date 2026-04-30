# M56_001: Dead-Code Sweep — orphaned modules under src/

**Prototype:** v2.0.0
**Milestone:** M56
**Workstream:** 001
**Date:** Apr 30, 2026
**Status:** PENDING
**Priority:** P2 — pre-v2.0.0 hygiene; no consumer impact, but every orphan rots the codebase and lies to future readers (RULE NLG, RULE ORP).
**Categories:** API
**Batch:** B1
**Branch:** feat/m56-dead-code-sweep (to be created)
**Depends on:** none

**Canonical architecture:** `docs/ARCHITECHTURE.md` — N/A (no flow change; pure removal/wiring).

---

## Implementing agent — read these first

1. `AGENTS.md` — RULE NLG (no legacy framing pre-v2.0.0), RULE ORP (orphan sweep), RULE TST-NAM (milestone-free test names), Milestone-ID Gate.
2. `docs/greptile-learnings/RULES.md` — RULE ORP, RULE TST-NAM, RULE FLL.
3. `docs/ZIG_RULES.md` — `pub` audit (any kept symbol with no external consumer must drop `pub`); test-discovery model (`comptime { _ = @import("…_test.zig"); }`).
4. `build.zig` — entry points are `src/main.zig`, `src/executor/main.zig`, `src/auth/tests.zig`, `src/zbench_micro.zig`. A `*.zig` file unreachable from these by transitive `@import` is dead from `zig build` and `zig build test` regardless of how many `pub` symbols it carries.

---

## Applicable Rules

- `docs/greptile-learnings/RULES.md` — RULE ORP (orphan sweep), RULE TST-NAM (milestone-free test names), RULE FLL (file/function length), RULE UFS (no inline literals).
- `docs/ZIG_RULES.md` — `pub` audit, test-discovery via parent `comptime { _ = @import(...) }`.
- `AGENTS.md` — RULE NLG (no legacy framing), Milestone-ID Gate, Verification Gate.

---

## Overview

**Goal (testable):** After this workstream, every `*.zig` file under `src/` (excluding `vendor/`) is either reachable by transitive `@import` from one of the four build entry points, OR is intentionally retained with documented justification. The orphan-sweep command in `Eval Commands` returns zero unjustified files.

**Problem:** A senior-engineer audit (Apr 30, 2026) of the 355 `*.zig` files under `src/` found 7 orphaned modules (zero `@import` references in the rest of the tree). They fall into three classes:

1. **Dead production modules** — code that ships no behavior because nothing calls it: `src/reliability/reliable_call.zig` (239 LOC), `src/reliability/rate_limit.zig` (70 LOC), `src/types/defaults.zig` (35 LOC), `src/observability/prompt_events.zig` (164 LOC).
2. **Dead test modules** — `test "..."` blocks in files no parent `@import`s, so they never compile into `make test`: `src/git/pr_comment_test.zig` (59 LOC), the `test "integration: prompt lifecycle events …"` in `src/observability/prompt_events.zig`.
3. **Dead fixtures** — `src/db/test_fixtures_uc2.zig` (110 LOC) and `src/db/test_fixtures_uc3.zig` (51 LOC) — UC1 sibling has 4 consumers, UC2/UC3 have zero.

The most insidious is `src/types/defaults.zig`: its header comment claims "if a constant diverges from its schema DEFAULT, the tests below will fail" — but the file is never imported, so those tests never run. It is a self-deceiving guard.

**Solution summary:** Delete the dead production modules and dead fixtures (728 LOC removed). Wire the legitimate test file (`pr_comment_test.zig`) into its parent via `comptime { _ = @import("pr_comment_test.zig"); }` so its test cases run under `make test`. Strip the `M16_001 §3.4` Milestone-ID violation from `pr_comment_test.zig` while wiring it. Re-run `make lint`, `make test`, `make test-integration`, and the orphan-sweep eval to confirm zero regressions and zero remaining orphans.

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `src/reliability/reliable_call.zig` | DELETE | Zero callers; 239 LOC of unused retry wrapper. RULE NLG (no "in case we need it"). |
| `src/reliability/rate_limit.zig` | DELETE | Zero callers; 70 LOC unused TokenBucket. |
| `src/types/defaults.zig` | DELETE | Zero callers; the "guard" is a lie because it never compiles into a test binary. |
| `src/observability/prompt_events.zig` | DELETE | Zero production callers (only `id_format.zig` exposes the `generatePromptLifecycleEventId` helper, which itself has no live caller — drop is safe; if reintroduced later, M{N+1} re-lands it with a real producer wiring). |
| `src/db/test_fixtures_prompt_events.zig` | DELETE | Sole consumer was `prompt_events.zig`; orphans together. |
| `src/db/test_fixtures_uc2.zig` | DELETE | Zero consumers. |
| `src/db/test_fixtures_uc3.zig` | DELETE | Zero consumers. |
| `src/git/pr_comment_test.zig` | EDIT | Strip `M16_001 §3.4` header (Milestone-ID Gate) and rename test names if they carry milestone tags. |
| `src/git/pr.zig` | EDIT | Add `comptime { _ = @import("pr_comment_test.zig"); }` so its tests participate in `make test`. |
| `src/types/id_format.zig` | EDIT | Drop `generatePromptLifecycleEventId` (and its test rows in `id_format_test.zig`) iff `prompt_events.zig` is deleted — it was the only caller. |
| `src/types/id_format_test.zig` | EDIT | Drop the two rows referencing `generatePromptLifecycleEventId`. |

No schema, no HTTP handlers, no auth surface, no architecture changes. Pure deletion + one wiring + one downstream symbol cleanup.

---

## Sections (implementation slices)

### §1 — Delete dead production modules

Remove `reliable_call.zig`, `rate_limit.zig`, `types/defaults.zig`, `observability/prompt_events.zig`, and the now-orphaned `db/test_fixtures_prompt_events.zig`. Implementation default: `git rm` (not `trash`) because the files are tracked and removal is intentional. After deletion, run the orphan grep from `Eval Commands` and confirm zero hits across `src/`.

### §2 — Delete dead fixtures

Remove `db/test_fixtures_uc2.zig` and `db/test_fixtures_uc3.zig`. If a future spec adds UC2/UC3 tests, the fixture lands with the test that needs it — not on speculation.

### §3 — Wire `pr_comment_test.zig` into its parent

Add a `comptime { _ = @import("pr_comment_test.zig"); }` line at the top of `src/git/pr.zig` so Zig's test discovery picks up the existing tests. Strip the `M16_001 §3.4` reference from the file header (Milestone-ID Gate). Verify each test name complies with RULE TST-NAM (no `M{N}_{NNN}` token, no `§X.Y`, no `T{N}`, no `dim X.Y`).

### §4 — Drop the orphaned `id_format` helper

`generatePromptLifecycleEventId` exists only to mint IDs for `prompt_events.zig`. With that file deleted, the helper becomes orphan production code. Remove the function from `src/types/id_format.zig` and the two rows in `src/types/id_format_test.zig` that exercise it. Confirm the remaining `id_format` surface still compiles and its tests still pass.

### §5 — Re-run the orphan eval and update spec

Run the orphan-sweep command in `Eval Commands`. Expected output: empty (or only the four entry points). If any new orphan surfaces (e.g. a sibling helper that becomes unreachable after deletion), surface it in `Discovery` and decide remove-or-wire before COMMIT.

---

## Interfaces

N/A — no public interface (HTTP, CLI, RPC, library) changes. The deleted symbols had no live caller.

---

## Failure Modes

| Mode | Cause | Handling |
|------|-------|----------|
| Build break after delete | Hidden `@import` discovered post-grep (e.g. dynamically loaded via `@embedFile` or string-literal path) | `zig build` fails loudly. Restore the file via `git restore <path>`, file the discovered consumer in `Discovery`, decide remove vs keep. |
| `make test` count drops | A wired test was previously passing under a different harness | Compare `make test` test count before/after via `zig build test --summary all`. Investigate any drop. |
| `make test-integration` regression | Deleted fixture was used by an integration test the local grep missed (e.g. via `@import` with non-literal path — disallowed in Zig, so this is mostly impossible) | Run `make test-integration` and `make down && make up && make test-integration` (Tier 3) before COMMIT. |
| Orphan sweep still non-empty | New orphan surfaced after deleting a downstream consumer (cascade) | Repeat §5 until fixed-point reached. Each cascade orphan gets a one-line entry in `Discovery`. |

---

## Invariants

1. **No `*.zig` file under `src/` (excluding `vendor/`) is unreachable from the four build entry points** — enforced by the orphan-sweep eval (E8 below). If the eval produces output, CHORE(close) blocks.
2. **Every `*_test.zig` is discovered by `zig build test`** — enforced by Zig's test-discovery model: the file must be in the transitive `@import` graph from `src/main.zig` (via `comptime { _ = @import(...) }` if necessary).
3. **No file under `src/` carries `M{N}_{NNN}`, `§X.Y`, `T{N}`, or `dim X.Y` tokens** — enforced by the Milestone-ID Gate self-audit grep (E9 below).

---

## Test Specification

| Test | Asserts |
|------|---------|
| `git_pr_extract_pr_number_happy` | `extractPrNumber("https://github.com/owner/repo/pull/42")` returns `42`. (Existing test, just newly wired.) |
| `git_pr_extract_pr_number_invalid` | Malformed URLs return `null` / error. (Existing test.) |
| `make_test_count_does_not_decrease` | Before/after test count from `zig build test --summary all` is `>=` baseline minus only the count of intentionally-deleted tests (the two in `prompt_events.zig`). |
| `orphan_sweep_eval_clean` | The orphan-sweep grep in E8 produces empty output. |
| `milestone_id_self_audit_clean` | E9 self-audit grep produces empty output. |

Negative tests covered by Failure Modes; the deletes themselves are negative-by-construction (the absence of a build error is the assertion).

---

## Acceptance Criteria

- [ ] All seven files in §1+§2 deleted from disk and tracked by `git rm` — verify: `git status` shows them under `deleted:`.
- [ ] `src/git/pr.zig` contains `comptime { _ = @import("pr_comment_test.zig"); }` — verify: `grep -F 'comptime { _ = @import("pr_comment_test.zig"); }' src/git/pr.zig`.
- [ ] `src/git/pr_comment_test.zig` contains no Milestone-ID tokens — verify: `grep -E 'M[0-9]+_[0-9]+|§[0-9]+\.[0-9]+|\bT[0-9]+\b|\bdim [0-9]+\.[0-9]+\b' src/git/pr_comment_test.zig` returns empty.
- [ ] `generatePromptLifecycleEventId` removed from `src/types/id_format.zig` and `src/types/id_format_test.zig` — verify: `grep -rn 'generatePromptLifecycleEventId' src/` empty.
- [ ] `make lint` clean.
- [ ] `make test` passes (with `pr_comment_test` cases now counted).
- [ ] `make test-integration` passes (or `N/A — no handler/schema/redis touched`; this spec touches none, so likely N/A — confirm at VERIFY).
- [ ] Cross-compile clean: `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux`.
- [ ] `make check-pg-drain` clean.
- [ ] `gitleaks detect` clean.
- [ ] Orphan sweep (E8) returns empty.
- [ ] Milestone-ID self-audit (E9) returns empty.
- [ ] No file added; deletions only (plus 3 small edits) — verify: `git diff --name-status origin/main` shows only `D` and `M` rows.

---

## Eval Commands (Post-Implementation Verification)

```bash
# E1: Build (no source-tree breaks from removed files)
zig build 2>&1 | tail -5

# E2: Unit tests
make test 2>&1 | tail -10

# E3: Lint
make lint 2>&1 | tail -5

# E4: Cross-compile
zig build -Dtarget=x86_64-linux 2>&1 | tail -3
zig build -Dtarget=aarch64-linux 2>&1 | tail -3

# E5: pg-drain hygiene
make check-pg-drain 2>&1 | tail -5

# E6: Gitleaks
gitleaks detect 2>&1 | tail -3

# E7: 350-line gate (no growth introduced)
git diff --name-only origin/main | grep -v -E '\.md$|^vendor/' | xargs -I{} sh -c 'test -f "{}" && wc -l "{}"' 2>/dev/null | awk '$1 > 350 { print "OVER: " $2 ": " $1 " lines (limit 350)" }'

# E8: Orphan sweep — every src/*.zig file should be reachable from an entry point.
# Approximation: every non-entry file should have at least one inbound @import reference.
ENTRYPOINTS='^(src/main\.zig|src/executor/main\.zig|src/auth/tests\.zig|src/zbench_micro\.zig|src/zbench_fixtures\.zig|src/crypto/hmac_sig\.zig)$'
while IFS= read -r f; do
    base=$(basename "$f")
    if echo "$f" | grep -qE "$ENTRYPOINTS"; then continue; fi
    if ! rg -l "@import\\(\"[^\"]*${base}\"\\)" src/ 2>/dev/null | grep -v "^${f}\$" > /dev/null; then
        echo "ORPHAN: $f"
    fi
done < <(find src -name "*.zig" -not -path "*/vendor/*")
echo "E8: orphan sweep done (empty above = pass)"

# E9: Milestone-ID self-audit (Milestone-ID Gate)
git diff --name-only HEAD | grep -vE '(^docs/|\.md$)' | xargs -r grep -nE 'M[0-9]+_[0-9]+|§[0-9]+(\.[0-9]+)+|\bT[0-9]+\b|\bdim [0-9]+\.[0-9]+\b' | head
echo "E9: milestone-id self-audit done (empty above = pass)"

# E10: Confirm deletes-only diff shape
git diff --name-status origin/main
```

---

## Dead Code Sweep

**1. Orphaned files — must be deleted from disk and git.**

| File to delete | Verify deleted |
|----------------|----------------|
| `src/reliability/reliable_call.zig` | `test ! -f src/reliability/reliable_call.zig` |
| `src/reliability/rate_limit.zig` | `test ! -f src/reliability/rate_limit.zig` |
| `src/types/defaults.zig` | `test ! -f src/types/defaults.zig` |
| `src/observability/prompt_events.zig` | `test ! -f src/observability/prompt_events.zig` |
| `src/db/test_fixtures_prompt_events.zig` | `test ! -f src/db/test_fixtures_prompt_events.zig` |
| `src/db/test_fixtures_uc2.zig` | `test ! -f src/db/test_fixtures_uc2.zig` |
| `src/db/test_fixtures_uc3.zig` | `test ! -f src/db/test_fixtures_uc3.zig` |

**2. Orphaned references — zero remaining imports or uses.**

| Deleted symbol or import | Grep command | Expected |
|--------------------------|--------------|----------|
| `reliable_call` import | `grep -rn 'reliable_call' src/` | 0 matches |
| `rate_limit` import | `grep -rn 'rate_limit\.zig' src/` | 0 matches |
| `types/defaults` import | `grep -rn 'types/defaults' src/` | 0 matches |
| `prompt_events` import | `grep -rn 'prompt_events' src/` | 0 matches |
| `test_fixtures_uc2` import | `grep -rn 'test_fixtures_uc2' src/` | 0 matches |
| `test_fixtures_uc3` import | `grep -rn 'test_fixtures_uc3' src/` | 0 matches |
| `generatePromptLifecycleEventId` symbol | `grep -rn 'generatePromptLifecycleEventId' src/` | 0 matches |

---

## Discovery (consult log)

(Empty at creation. Populate as Legacy-Design Consults / Architecture Consults fire during EXECUTE.)

---

## Skill-Driven Review Chain (mandatory)

| When | Skill | What it does | Required output |
|------|-------|--------------|-----------------|
| After implementation, before CHORE(close) | `/write-unit-test` | Audits test coverage of the diff. For a deletion-only diff, expects "no new tests required; pre-existing tests still pass." | Skill returns clean. |
| After tests pass, before CHORE(close) | `/review` | Adversarial diff review against this spec, RULE ORP, RULE NLG, ZIG_RULES.md `pub` audit. Confirms no public surface was reachable from outside the deleted scope. | Skill returns clean OR every finding dispositioned. |
| After `gh pr create` | `/review-pr` + `kishore-babysit-prs` | Greptile PR review polling and triage. | Comments addressed inline before merge. |

---

## Verification Evidence

(Filled in during VERIFY phase.)

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| Unit tests | `make test` | | |
| Lint | `make lint` | | |
| Cross-compile (x86_64) | `zig build -Dtarget=x86_64-linux` | | |
| Cross-compile (aarch64) | `zig build -Dtarget=aarch64-linux` | | |
| pg-drain | `make check-pg-drain` | | |
| Gitleaks | `gitleaks detect` | | |
| Orphan sweep (E8) | (see above) | | |
| Milestone-ID self-audit (E9) | (see above) | | |

---

## Out of Scope

- **Duplicate-basename normalization** (the two `mod.zig` files vs the rest using `<folder>.zig` next to the folder) — leave for a follow-up spec; pure naming convention, not dead code.
- **`pub` audit sweep across all 355 files** — the ZIG_RULES.md PUB GATE handles this incrementally on every touched file. A bulk sweep is M{N+1} territory.
- **Rewiring `prompt_events` into a real producer** — if the prompt-lifecycle event stream becomes a product requirement, a future spec lands the producer + the file together. This spec deletes on the principle that pre-v2.0.0 we don't keep code "in case we need it" (RULE NLG).
- **Reliability layer rewrite** — `backoff.zig` and `error_classify.zig` remain; only the unused `reliable_call.zig` and `rate_limit.zig` are removed. If a generic retry wrapper is needed later, M{N+1} re-lands it with a wired call site and a test that proves it executes in production.
