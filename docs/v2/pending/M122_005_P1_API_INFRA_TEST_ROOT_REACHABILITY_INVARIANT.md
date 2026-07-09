<!--
SPEC AUTHORING RULES (load-bearing — the one comment that survives):
- Body order = the executing agent's read order. Fill via the kishore-spec-new
  skill (authoring order lives there); after filling, DELETE every "tpl:"
  guidance comment — the SPEC TEMPLATE GATE blocks tpl residue, unfilled
  {slots}, and missing required sections (audits/spec-template.sh --staged).
- No time/effort/hour/day estimates anywhere. No effort columns, complexity
  ratings, percentage-complete, implementation dates, assigned owners.
- Priority (P0/P1/P2/P3) is the only sizing signal; Dependencies are the only
  sequencing signal. A section that contradicts these rules loses — delete it.
-->

# M122_005: Test-root reachability invariant — every `test` block runs, or is provably waived

**Prototype:** v2.0.0
**Milestone:** M122
**Workstream:** 005
**Date:** Jul 09, 2026
**Status:** PENDING
**Priority:** P1 — the test-harness integrity check the other four M122 workstreams' Test Delta rows silently depend on: today a Zig `test` block only runs when its file is force-imported from a test root, nothing enforces that convention, and the depth gate credits blocks that never compile — so a false assertion can sit green in a "1504 pass" suite indefinitely.
**Categories:** API, INFRA
**Batch:** B1 — runs alone; touches the Zig test roots, build graph, one checker script, and one make file; no overlap with M122_001..004 subject matter.
**Branch:** {added at CHORE(open)}
**Test Baseline:** set at CHORE(open) — `unit=<N> integration=<M>` via `make _lint_zig_test_depth`
**Depends on:** none — sibling M122_004 shares the "make the guard actually fire" theme but is independent; this is the test-harness instance of that class.
**Provenance:** agent-generated — discovered Jul 09, 2026 while re-verifying the Jul 02, 2026 `fleet-wide-refactor-audit`. The dead-block proof is empirical (a false assertion living in a green suite, reproduced below); the broader dead-set size is explicitly unresolved — the static reachability estimate is an upper bound with false positives, so this spec pins no file count and derives the true set from the compiler.
**Canonical architecture:** `docs/architecture/direction.md` — platform determinism + gate discipline; the force-import convention itself is documented in the header comment of `src/agentsfleetd/tests.zig`.

---

## Overview

**Goal (testable):** every `src/**/*.zig` file that contains a line beginning `test "` either registers at least one test in a compiled test binary (proven from the compiler, not from grep) or carries an explicit `// no-test-root: <reason>` waiver; `src/agentsfleetd/cmd/common.zig` is wired into a test root with its migration assertions repaired to derive from `schema.migrations` itself; and `_lint_zig_test_depth` counts only compiler-registered blocks so Test Baseline / Test Delta measure reality.

**Problem:** Zig collects a file's inline `test` blocks only when the file is reachable, by relative `@import`, from the root of a test compilation. The four roots (`src/agentsfleetd/tests.zig`, `src/runner/tests.zig`, `src/lib/tests.zig`, `src/agentsfleetd/auth/tests.zig`) force-import the files whose tests should run — but nothing verifies the list is complete. `cmd/common.zig` is imported only by `migrate.zig` / `doctor.zig` / `preflight.zig`, none reachable from a root, so its 11 `test` blocks have never compiled. Two of them assert `migrations.len == 26` and "last version is 26" while `schema/embed.zig` now has 27 entries — a provably false assertion sitting inside a suite that reports `1504 pass, 493 skip, 0 fail`, because a test that never compiles cannot fail. Worse, `_lint_zig_test_depth` counts `^test "` textually, so those 11 phantom blocks inflate the very number VERIFY uses to prove new tests were added.

**Solution summary:** Make the force-import convention an enforced, compiler-truth invariant. Add a checker that builds each test binary in a list-only mode, unions the registered test names, and flags any test-bearing source file whose blocks appear in none of them; wire it into the existing `lint-zig` target and let it feed a corrected reachable-block count into `_lint_zig_test_depth`. Force-import `cmd/common.zig` and repair its two stale assertions so they derive count and version-contiguity from `schema.migrations` (never a re-armed literal `27`). Force-import — or explicitly waive — every remaining file the checker enumerates, treating each newly-executing failure as a finding under the standard gate-flag triage.

## PR Intent & comprehension handshake

- **PR title (eventual):** Enforce test-root reachability from the compiler; wire cmd/common.zig; make the depth gate count only runnable tests
- **Intent (one sentence):** a `test` block that cannot run can no longer masquerade as a passing test, and the number VERIFY trusts counts only blocks that actually compiled.
- **Handshake** — the implementing agent fills this at PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `src/agentsfleetd/tests.zig` (header + force-import block) — the convention this spec enforces; the barrel §2/§3 extend. Its sibling headers in `src/runner/tests.zig`, `src/lib/tests.zig`, `src/agentsfleetd/auth/tests.zig` note that the test runner collects only root-module tests — named-module (`common`, `log`, `schema`) tests do NOT collect in an importing binary, which is why compiler truth, not a relative-import walk, is the authority.
2. `src/agentsfleetd/cmd/common.zig` (lines 25-31, 110-121, 226-229) — `canonicalMigrations()` returns `[schema_migrations.len]`; the two stale assertions §2 repairs; `schema/embed.zig` is the single source of the version list.
3. `make/quality.mk` (`_lint_zig_test_depth`, `lint-zig` prerequisite list, `_lint_zig_pg_drain` calling `python3 lint-zig.py src`) — the existing private-prerequisite pattern §1/§4 mirror; do NOT add a public near-duplicate target.
4. `build.zig` (the `test` + `test-auth` `addTest` sites, ~lines 216-267), `build_runner.zig` (`test` at ~120), `src/build/lib_tests.zig` (`addTestStep`) — the `addTest` modules whose `.test_runner` field §1 sets.
5. `docs/greptile-learnings/RULES.md` — RULE TST (M2_001: router tests existed since M16 but never ran; two bugs surfaced on import — the exact class this spec closes) and RULE MIG (index/version assertions in `cmd/common.zig`).

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `scripts/check_zig_test_reachability.py` | CREATE | builds each test binary in list mode, unions registered test names, flags dead test-bearing files, and emits the reachable-block count for the depth gate |
| `scripts/check_zig_test_reachability_test.py` | CREATE | fixture-based self-tests: dead file flagged, waiver exempts, un-wire turns the gate red |
| `src/build/test_runner.zig` | CREATE | custom Zig test runner; under a list-mode env flag it prints every `builtin.test_functions` name and exits without running; otherwise behavior-preserving |
| `build.zig` | EDIT | set `.test_runner` on the `test` and `test-auth` `addTest` modules |
| `build_runner.zig` | EDIT | set `.test_runner` on the runner `test` `addTest` module |
| `src/build/lib_tests.zig` | EDIT | set `.test_runner` on the lib `addTestStep` module |
| `make/quality.mk` | EDIT | add the checker as a `lint-zig` prerequisite; rewrite `_lint_zig_test_depth` to consume the checker's reachable count instead of a textual `^test "` grep |
| `src/agentsfleetd/cmd/common.zig` | EDIT | self-updating migration assertions (count-equals-embedded + version contiguity), replacing the literal-`26` traps |
| `src/agentsfleetd/tests.zig` | EDIT | force-import `cmd/common.zig` and the remaining agentsfleetd files the checker enumerates |
| `src/runner/tests.zig` | EDIT | force-import the runner files the checker enumerates (if any) |
| `src/lib/tests.zig` | EDIT | force-import the lib files the checker enumerates (if any) |
| `src/agentsfleetd/auth/tests.zig` | EDIT | force-import the auth files the checker enumerates (if any) |
| `src/**/*.zig` (the checker's dead-set output) | EDIT | each newly-executing test repaired per gate-flag triage; the set is the checker's output, deliberately NOT enumerated here |

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — **TST** (test discovery requires explicit import from a test root — the root cause); **MIG** (every index/version assertion in `cmd/common.zig` tracks the array — the §2 repair); **UFS** (the repaired assertions derive count/version from `schema.migrations`; swapping literal `26`→`27` is an explicit anti-goal); **TST-NAM** (new test identifiers carry no milestone/section IDs); **NDC/NLR** (no dead assertion residue, touch-it-fix-it on every root barrel edited); **ORP** (grep the old literal assertions and any removed symbol before commit); **FLL** (the checker script, the custom runner, and each edited file stay under the caps — split if the runner nears 350 lines).
- **`dispatch/write_zig.md`** — `*.zig` edits (the runner, `common.zig`, the barrels): ownership/`errdefer`, cross-compile both linux targets.
- **`dispatch/write_any.md`** — `*.py` / `make` authoring invariants: File & Function Length, UFS named constants, milestone-free test identifiers.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | yes — `common.zig`, the barrels, the custom runner | cross-compile `x86_64-linux` + `aarch64-linux`; the runner's normal path stays behavior-preserving (suite pass/skip counts unchanged) |
| PUB / Struct-Shape | no — the custom runner exposes only Zig's expected test-runner entry point; no new project pub surface | shape verdict recorded at EXECUTE |
| File & Function Length (≤350/≤50/≤70) | yes — the custom runner and the checker | keep the runner minimal (list branch + delegated normal path); split the checker into focused functions |
| UFS (repeated/semantic literals) | yes | the waiver marker `// no-test-root:`, the list-mode env-var name, and the depth-gate output keys live as single-declaration constants shared between the runner, the checker, and the make recipe |
| UI Substitution / DESIGN TOKEN | no | no UI surface |
| LOGGING / LIFECYCLE / ERROR REGISTRY / SCHEMA | no | build-tooling and a test runner; no handler log lines, error codes, or schema edits — `schema/embed.zig` is read, never modified |

## Prior-Art / Reference Implementations

- **Reference:** `scripts/check_route_registration_doc.py` + its `_lint_zig_pg_drain` / `check-route-registration-doc` wiring — the shape §1 mirrors: a Python checker invoked from a make prerequisite that exits non-zero and names offenders. Divergence: this checker shells out to `zig build` to obtain compiler truth rather than scanning text alone.
- **Reference:** `src/agentsfleetd/tests.zig` force-import barrel — the canonical convention every root already follows; §2/§3 extend it, they do not invent a new mechanism.

## Sections (implementation slices)

### §1 — Compiler-truth reachability gate

A checker enumerates the true dead set and fails the build on it. **Implementation default:** the authority is compiler truth, not a static import graph — build each test binary (the four roots plus any other `addTest` the build defines) in a list-only mode via a shared custom test runner, union every `builtin.test_functions` name, and mark a source file dead when none of its `test "<desc>"` descriptions appear in that union. A relative-import walk is rejected as the authority because it under-predicts liveness (the audit's static model predicted ~1735 live blocks while the daemon binary registers 1997) and would flag live files as dead. A file may opt out only with an explicit `// no-test-root: <reason>` line. The checker prints every offender and exits non-zero, and is added to the existing `lint-zig` prerequisite list — no new public target.

- **Dimension 1.1** — a planted test-bearing fixture reachable from no root makes the checker exit non-zero and print its path → Test `test_reachability_flags_unwired_fixture`
- **Dimension 1.2** — a file carrying `// no-test-root: <reason>` is not reported even with no registering root → Test `test_reachability_waiver_exempts`
- **Dimension 1.3** — the checker runs inside `make lint-zig` and its non-zero exit fails the target → Test `test_reachability_wired_into_lint_zig`
- **Dimension 1.4** — with the custom runner installed, a normal `make test-unit-agentsfleetd` run reports the same pass/skip totals and zero failures as before the swap → Test `test_runner_normal_execution_unchanged`

### §2 — cmd/common.zig wired in with self-updating migration assertions

Force-import `cmd/common.zig` from the agentsfleetd root so its 11 blocks compile, then repair the two stale assertions. **Implementation default:** the repaired assertions derive everything from `schema.migrations` — no literal count. Assert that `canonicalMigrations().len` equals the embedded `schema.migrations.len`, that versions are strictly monotonic and contiguous from 1, and that the last version equals the registered count. This is deliberate per RULE UFS and RULE MIG: swapping the literal `26` for `27` would only re-arm the same trap the next time a migration lands; a derived assertion never drifts.

- **Dimension 2.1** — after wiring, the checker (§1) sees `cmd/common.zig` as reachable and every one of its blocks registers → Test `test_common_zig_reachable_and_registered`
- **Dimension 2.2** — `canonicalMigrations().len` equals `schema.migrations.len` and the last version equals that count, with no literal migration count in the file → Test `test_migration_count_equals_embedded`
- **Dimension 2.3** — migration versions are strictly increasing and contiguous from 1 → Test `test_migration_versions_contiguous_monotonic`
- **Dimension 2.4** — a fixture migration list with a duplicated or gapped version fails the contiguity assertion → Test `test_migration_version_gap_rejected`

### §3 — Enable the remaining unreachable set; triage what fires

Every file the §1 checker lists is force-imported under one root, or waived with a stated reason — the set is the checker's output, not a number written into this spec. A newly-executing test that fails is the payoff, not an obstacle. **Implementation default (gate-flag triage, per AGENTS.md):** a mechanical cause (a stale literal, a renamed symbol, a moved import path) is auto-repaired in the same diff and reported in one line; a judgment call (a genuine product bug or a weakened guarantee) STOPS and is surfaced to Indy as fix-or-defer. An agent-unilateral deferral is incomplete scope, not deferral — it blocks CHORE(close) without an Indy-acked quote in Discovery. Two of `common.zig`'s previously-dead blocks are the guards for findings the Jul 09 audit re-discovered independently (see Failure Modes).

- **Dimension 3.1** — the §1 checker exits zero across the whole tree: no test-bearing file is dead and unwaived → Test `test_no_unwired_test_files`
- **Dimension 3.2** — the previously-dead SqlStatementSplitter and concurrent-migration-race guards now execute in the daemon suite → Test `test_previously_dead_guards_now_run`

### §4 — Depth gate counts only reachable blocks

`_lint_zig_test_depth` counts `^test "` textually across `src/`, crediting blocks that cannot compile, so Test Baseline / Test Delta — the mechanism VERIFY uses to prove tests were added — is unsound. Recount from the compiler-registered set the §1 checker already computes. **Implementation default:** the recipe reads the checker's reachable count rather than re-deriving it, so the two can never disagree.

- **Dimension 4.1** — `_lint_zig_test_depth` reports the count of compiler-registered blocks, which is strictly ≤ the textual `^test "` count and excludes any dead file → Test `test_depth_counts_registered_only`
- **Dimension 4.2** — un-wiring one currently-reachable file (a fixture mutation) drops the reachable count and turns the depth gate red → Test `test_depth_gate_red_on_unwire`

## Interfaces

```
scripts/check_zig_test_reachability.py
  --check   -> exit 0 iff every src/**/*.zig with a `^test "` line registers
               >=1 test in some test binary OR carries `// no-test-root: <reason>`;
               exit non-zero listing each offending path otherwise.
  --count   -> print `reachable_test_cases=<N>` and `reachable_integration_cases=<M>`
               (the registered-block totals _lint_zig_test_depth consumes).

Waiver grammar (source line, exact):  // no-test-root: <reason>
List-mode env flag (build side): when set, src/build/test_runner.zig prints one
  registered test name per line and exits 0 without executing any test.
```

No command-line surface of `agentsfleet`, no HTTP route, and no on-disk schema path changes. `schema/embed.zig` and the migration SQL files are read-only inputs.

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Dead test file | a `test`-bearing file reachable from no root | checker exits non-zero, prints the path; `lint-zig` fails (Dimension 1.1) |
| Intentional exclusion | a file whose tests only run under a filtered/special build | `// no-test-root: <reason>` suppresses the report (Dimension 1.2) |
| SqlStatementSplitter guard, long dead | `test "every migration SQL is parseable…"` never compiled; the Jul 09 audit re-found the splitter mis-split by hand | wiring `common.zig` re-arms the guard; a real mis-split now fails the suite (Dimension 3.2) |
| Migration-race guard, long dead | `test "integration: startup blocks on concurrent migration race…"` never compiled; the audit re-found the advisory-lock ordering bug | the guard now runs; the ordering invariant is enforced by test again (Dimension 3.2) |
| Stale literal assertion | a re-armed literal migration count drifts on the next migration | derived count/version assertions can't drift (Dimensions 2.2/2.3); a gap is rejected (2.4) |
| Depth gate credits a phantom | textual `^test "` count includes non-compiling blocks | count comes from the registered set (Dimension 4.1); un-wire self-test proves it (4.2) |
| Custom runner regresses the suite | list-mode runner alters normal execution | pass/skip totals and zero-failure state asserted unchanged (Dimension 1.4) |

## Invariants

1. No `src/**/*.zig` with a `^test "` line is dead — enforced by the compiler-truth checker in `lint-zig` (Dimension 1.1), waiver-gated, never by review.
2. `cmd/common.zig`'s migration count/version assertions derive from `schema.migrations` — enforced by Dimensions 2.2/2.3 plus a RULE UFS grep confirming no bare migration-count literal remains around `migrations.len`.
3. `_lint_zig_test_depth` counts only compiler-registered blocks — enforced by feeding it the checker's `--count` output and by the un-wire self-test (Dimension 4.2).
4. The custom test runner's normal path is behavior-preserving — enforced by the unchanged pass/skip/zero-failure assertion (Dimension 1.4).

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| not applicable — no product/operator signal changes | — | build-time gates and a test runner only; no runtime event added, renamed, or removed | unchanged | no secret material read or printed | the new tests assert exit codes and registered counts, not events |

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit | `test_reachability_flags_unwired_fixture` | fixture file with `test "…"` reachable from no root → checker exit non-zero, path printed |
| 1.2 | unit | `test_reachability_waiver_exempts` | same fixture + `// no-test-root: fixture` → checker exit 0, path absent |
| 1.3 | unit (grep) | `test_reachability_wired_into_lint_zig` | `make/quality.mk`: checker is a `lint-zig` prerequisite; its failure fails the target |
| 1.4 | integration (regression) | `test_runner_normal_execution_unchanged` | `make test-unit-agentsfleetd` pass/skip totals equal the pre-swap baseline, 0 failures |
| 2.1 | integration | `test_common_zig_reachable_and_registered` | checker `--check` reports `cmd/common.zig` reachable; its blocks appear in the registered union |
| 2.2 | unit | `test_migration_count_equals_embedded` | `canonicalMigrations().len == schema.migrations.len`; last version == len; no literal count in file |
| 2.3 | unit | `test_migration_versions_contiguous_monotonic` | versions strictly increasing, contiguous from 1 |
| 2.4 | unit (negative) | `test_migration_version_gap_rejected` | fixture list with a gap/duplicate → contiguity assertion fails |
| 3.1 | unit | `test_no_unwired_test_files` | checker `--check` over the whole tree → exit 0 (all reachable or waived) |
| 3.2 | integration | `test_previously_dead_guards_now_run` | the splitter-parse and migration-race test names appear in the daemon binary's registered set, green |
| 4.1 | unit | `test_depth_counts_registered_only` | `_lint_zig_test_depth` count == checker `--count`, ≤ textual `^test "` count |
| 4.2 | unit (negative) | `test_depth_gate_red_on_unwire` | un-wiring one reachable file drops the count and the depth gate exits non-zero |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Reachability checker self-tests pass (§1/§4) | `python3 scripts/check_zig_test_reachability_test.py` | exit 0 | P0 | |
| R2 | No dead test-bearing file remains (§1/§3) | `python3 scripts/check_zig_test_reachability.py --check` | exit 0, no paths printed | P0 | |
| R3 | `cmd/common.zig` is force-imported (§2) | `grep -n 'cmd/common.zig' src/agentsfleetd/tests.zig` | ≥1 match | P0 | |
| R4 | No bare migration-count literal survives (§2) | `grep -nE '@as\(usize, 2[0-9]\)\|i32, 2[0-9]\)' src/agentsfleetd/cmd/common.zig` | no output | P0 | |
| R5 | Depth gate counts registered blocks (§4) | `make _lint_zig_test_depth` | exit 0, count == checker `--count` | P0 | |
| R6 | Daemon suite green with the newly-wired blocks (§2/§3) | `make test-unit-agentsfleetd` | exit 0, 0 failures | P0 | |
| R7 | Diff stays inside Files Changed | `git diff --name-only origin/main` | 0 paths missing from the Files Changed table | P0 | |
| S1 | Zig lint clean incl. the new gate | `make lint-zig` | exit 0 | P0 | |
| S2 | Cross-compile | `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` | exit 0 | P0 | |
| S3 | No secrets | `gitleaks detect` | exit 0 | P0 | |
| S4 | No oversize source file | `git diff --name-only origin/main \| grep -v '\.md$' \| xargs wc -l 2>/dev/null \| awk '$1>350 && $2!="total"'` | no output | P0 | |

**Grading protocol (VERIFY):** run the Verify command verbatim; grade ONLY from its output. Graded = ✅/❌ + the one decisive output line (`342 passed`); long evidence goes to PR Session Notes with a pointer here. **Ship gate:** every row graded, every P0 ✅ → eligible for CHORE(close); any ❌ or empty cell → return to EXECUTE; a P1 ❌ ships only with an Indy-acked deferral quote in Discovery.

## Dead Code Sweep

**1. Orphaned files — deleted from disk and git.**

| File to delete | Verify |
|----------------|--------|
| N/A — no files deleted (this spec adds a gate and wires existing files) | — |

**2. Orphaned references — zero remaining imports/uses.**

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| stale literal migration-count assertion | `grep -nE 'expectEqual\(@as\(usize, 26\)' src/agentsfleetd/cmd/common.zig` | 0 matches |
| textual-count depth-gate recipe body | `grep -n "grep -hE '\\^test \"'" make/quality.mk` | 0 matches |

## Out of Scope

- Retroactively re-grading the Test Delta rows of already-merged specs (M109_*, M120_*, and earlier) whose baselines were computed from the textual count — the corrected count applies going forward only.
- Fixing the product bugs a newly-executing test may expose (the SqlStatementSplitter mis-split and the pool-migrations advisory-lock ordering finding each get their own spec) — this spec makes the guards run and triages what fires; it does not carry their fixes.
- Changing how Zig itself registers tests, or migrating away from inline `test` blocks — the custom runner only adds a list mode; collection semantics are unchanged.
- Broadening the invariant to non-Zig test surfaces (Bun/Vitest packages) — those use directory-based discovery, not force-import.

---

## Product Clarity (authoring record)

1. **Successful user moment** — an engineer adds a Zig `test` block, and if it can never run, `make lint-zig` says so by name before the PR opens — instead of the block sitting green-by-omission for months like `cmd/common.zig`'s false `== 26` assertion did.
2. **Preserved user behaviour** — `make test-unit-agentsfleetd`, `test-auth`, `test-lib`, and the runner suite all keep their exact pass/skip semantics; the custom runner only adds a list mode, and the depth gate keeps its ≥25/≥3 floors.
3. **Optimal-way check** — compiler truth is the most direct sound signal: the build already knows which tests it registered; the checker just reads that, sidestepping the static walk's false positives.
4. **Rebuild-vs-iterate** — iterate: one checker, one small runner, four barrel edits, one assertion repair, one recipe change; nothing here trades determinism for anything.
5. **What we build** — a reachability checker, a list-mode test runner, the four-root wiring, the `common.zig` repair, and the corrected depth-gate count.
6. **What we do NOT build** — the product-bug fixes the enabled tests expose, a re-grade of merged baselines, or any change to Zig's collection model — see Out of Scope.
7. **Fit with existing features** — compounds the `lint-zig` gate suite and the M122_004 "make the guard fire" theme; must not destabilize the four test suites' pass/skip counts (Dimension 1.4 guards exactly that).
8. **Surface order** — N/A — no user surface. This is repo test-harness tooling and Continuous Integration (CI) gates; there is no end-user product surface.
9. **Dashboard restraint** — N/A — no user surface. No UI, controls, or quality claims are added.
10. **Confused-user next step** — N/A — no user surface. The engineer-facing recovery is the checker's own stderr: it prints each dead file's path and names the `// no-test-root:` waiver as the explicit opt-out.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** four Sections — the gate (§1), the proven case (§2), the enumerated remainder (§3), the depth-gate correction (§4). Each is independently testable and DONE-markable; §1 must land before §3 (which consumes its list) and §4 (which consumes its count).
- **Fifth-workstream justification:** `docs/TEMPLATE.md` caps a milestone at four workstreams and reserves a fifth only for a cross-cutting concern. This qualifies: it is test-harness integrity, orthogonal to the subject matter of M122_001..004, and it is precisely what makes those workstreams' Test Delta rows trustworthy — a fifth stream about the measurement apparatus itself, not a fifth feature.
- **Alternatives considered:** (a) a pure static relative-import reachability walk — rejected as the authority: it under-predicts liveness (the audit model showed ~1735 vs 1997 registered) and would fail live files, so it is at most a fast pre-filter, never the gate; (b) swapping the literal `26` for `27` in `common.zig` — rejected: it re-arms the identical trap on the next migration (RULE UFS); the derived assertion is the durable fix.
- **Patch-vs-refactor verdict:** this is a **patch** on the test harness — it adds a gate and wires existing files; the only structural addition (a list-mode test runner) hardens the build rather than restructuring it.

## Discovery (consult log)

- **Consults** — Architecture / Legacy-Design / gate-flag triage: empty at creation.
- **Metrics review** — empty at creation.
- **Skill-chain outcomes** — empty at creation.
- **Deferrals** — empty at creation.
