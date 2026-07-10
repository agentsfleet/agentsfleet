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
**Status:** DONE
**Priority:** P1 — the test-harness integrity check the other four M122 workstreams' Test Delta rows silently depend on: today a Zig `test` block only runs when its file is force-imported from a test root, nothing enforces that convention, and the depth gate credits blocks that never compile — so a false assertion can sit green in a "1504 pass" suite indefinitely.
**Categories:** API, INFRA
**Batch:** B1 — runs alone; touches the Zig test roots, build graph, the checker scripts, and the make gates; no overlap with M122_001..004 subject matter.
**Branch:** `feat/m122-005-test-root-reachability`
**Test Baseline:** unit=2395 integration=267 — recorded at CHORE(open) from the textual `^test "` counter, which §4 replaces. **Final: unit=2401 integration=267 (+6 unit).** The CHORE(open) prediction that the corrected count would land *below* the baseline was wrong in sign and right in mechanism; see Discovery → "Corrections to this spec's own record".
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
3. `make/quality.mk` (`lint-zig` prerequisite list, `_lint_zig_pg_drain` calling `python3 lint-zig.py src`) — the existing private-prerequisite pattern §1/§4 mirror; do NOT add a public near-duplicate target. The two reachability recipes ended up in `make/test-reachability.mk` (RULE FLL).
4. `build.zig`, `build_runner.zig`, `src/build/lib_tests.zig`, `src/build/s3.zig` — the 8 `addTest` sites. §1 does NOT set `.test_runner` on them (see the amended §1); it attaches a parallel list-only lane sharing each root module.
5. `docs/greptile-learnings/RULES.md` — RULE TST (M2_001: router tests existed since M16 but never ran; two bugs surfaced on import — the exact class this spec closes) and RULE MIG (index/version assertions in `cmd/common.zig`).

## Files Changed (blast radius)

Amended at EXECUTE against the compiler-truth dead set (7 files, 26 blocks) and Indy's
runner-wiring decision; the pre-EXECUTE guesses are superseded, not hidden. See Discovery.

| File | Action | Why |
|------|--------|-----|
| `scripts/check_zig_test_reachability.py` | CREATE | runs the `list-tests` lane on both build graphs, unions registered test names per root, flags dead test-bearing files, and emits the reachable-block count for the depth gate |
| `scripts/check_zig_test_reachability_test.py` | CREATE | reachability + depth-count self-tests: dead file flagged, waiver exempts, un-wire turns the gate red, duplicate description does NOT mask a dead file |
| `scripts/check_zig_test_reachability_cli_test.py` | CREATE | forced by RULE FLL (the suite reached 398 lines): listing-parse and `--check`/`--count`/`--counts-out` dispatch self-tests |
| `scripts/reachability_test_support.py` | CREATE | shared fixtures for the two suites above (FLL split) |
| `make/test-reachability.mk` | CREATE | forced by RULE FLL: `make/quality.mk` reached 361 lines (cap 350, and it was already at 339), so the two reachability recipes moved out, mirroring how `make/` is already carved by concern |
| `Makefile` | EDIT | `include make/test-reachability.mk` |
| `src/build/test_runner_list.zig` | CREATE | list-only Zig test runner (`mode: .simple`): prints every `builtin.test_functions` name and exits without running. Installed ONLY on the `list-tests` lane |
| `src/build/test_list.zig` | CREATE | the `list-tests` lane — attaches a second, list-only compilation per test binary, sharing the real step's root module |
| `src/build/auth_tests.zig` | CREATE | forced by RULE FLL: `build.zig` hit 360 lines (cap 350), so the `test-auth` portability gate is extracted, mirroring `lib_tests.zig`/`s3.zig` |
| `src/build/main.zig` | EDIT | re-export `test_list` + `auth_tests` to both build graphs |
| `build.zig` | EDIT | create the `list-tests` step; attach the daemon lane; delegate `test-auth` to `auth_tests.zig` |
| `build_runner.zig` | EDIT | create its own `list-tests` step; attach the runner unit + integration lanes |
| `src/build/lib_tests.zig` | EDIT | attach the lib / logging / call_deadline lanes |
| `src/build/s3.zig` | EDIT | attach the s3 lane (the eighth `addTest`, missing from the pre-EXECUTE table) |
| `make/quality.mk` | EDIT | `_lint_zig_test_reachability` added to the `lint-zig` prerequisite list (the recipes themselves live in `make/test-reachability.mk`) |
| `src/agentsfleetd/cmd/common.zig` | EDIT | self-updating migration assertions (count-equals-embedded + version contiguity + a gap/duplicate negative), replacing the literal-`26` traps |
| `src/agentsfleetd/tests.zig` | EDIT | force-import the 6 dead `cmd/*` files |
| `src/lib/tests.zig` | EDIT | force-import `common/backoff.zig` |
| `src/agentsfleetd/cmd/preflight_test.zig` | EDIT | had not compiled since the environment-injection refactor: `std.posix.setenv`/`unsetenv` are gone in Zig 0.16, two call sites used pre-refactor arities, and the signal-handler type had moved. Rebuilt on `common.env.fromPairs`; the tautological `expect(true)` becomes a sigaction read-back |
| `src/agentsfleetd/cmd/doctor_render.zig` | EDIT | product fix (Indy-approved, see Discovery): `CheckResult.detail` had mixed ownership, leaking 5 allocations per `agentsfleetd doctor`. `appendCheck` now copies; `freeResults` is the single free path |
| `src/agentsfleetd/cmd/doctor.zig` | EDIT | call `freeResults`; delete the false "callers must provide an arena-style allocator" comment (`main.zig:139` passes a `DebugAllocator`) |

Not edited, contrary to the pre-EXECUTE guess: `src/runner/tests.zig` and `src/agentsfleetd/auth/tests.zig` — the compiler-truth dead set contained no runner or auth file, so neither barrel needed a line.

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

A checker enumerates the true dead set and fails the build on it. The authority is compiler truth, not a static import graph: a relative-import walk under-predicts liveness and would flag live files as dead.

**Implementation (amended at EXECUTE; supersedes the two defaults below).**

1. **Parallel list-only lane, not a runner swap.** The spec's default said "set `.test_runner` on the `test` and `test-auth` `addTest` modules." In Zig 0.16 `TestRunner` is `{ path, mode: .simple | .server }`, and `zig build test --summary all` gets its pass/skip totals from the build system over `std.zig.Server` — i.e. `server` mode. Swapping the runner would therefore either break `--summary all` or force vendoring upstream's ~460-line `test_runner.zig` (over the 350-line cap, and a compiler-internal fork owned forever). Instead, all 8 `addTest` steps keep Zig's default runner untouched, and `src/build/test_list.zig` attaches a **second** compilation of the same root module carrying `src/build/test_runner_list.zig` (`mode: .simple`), behind `zig build list-tests`. Invariant 4 and Dimension 1.4 therefore hold by construction rather than by assertion — the normal path is not modified at all. Lanes carry no `filters`, since `-Dtest-filter` prunes `builtin.test_functions` and a filtered listing would report live blocks as dead.

2. **Prefix matching on the namespace, not the test description.** The spec's default said to "mark a source file dead when none of its `test "<desc>"` descriptions appear in that union." That is unsound, and a fixture proves it: a dead file sharing a description with a live one is reported reachable, because the description *does* appear in the union. Zig names each test `<namespace>.test.<description>` (or `<namespace>.test_<N>` when anonymous), where `<namespace>` is the source path relative to that binary's root directory with `/`→`.` and `.zig` stripped. The checker therefore inverts the mapping: for each candidate file it computes the expected namespace and asks whether any registered name carries that prefix. `test_duplicate_description_does_not_mask_a_dead_file` locks this in.

A file may opt out only with an explicit `// no-test-root: <reason>` line. The checker prints every offender and exits non-zero from `_lint_zig_test_reachability`, a private prerequisite of the existing `lint-zig` — no new public target. That target runs the checker's own self-tests first, so the gate guarding the gate cannot itself go dead.

- **Dimension 1.1** — DONE — a planted test-bearing fixture reachable from no root makes the checker exit non-zero and print its path → Test `test_reachability_flags_unwired_fixture`
- **Dimension 1.2** — DONE — a file carrying `// no-test-root: <reason>` is not reported even with no registering root → Test `test_reachability_waiver_exempts`
- **Dimension 1.3** — DONE — the checker runs inside `make lint-zig` and its non-zero exit fails the target → Tests `test_reachability_wired_into_lint_zig`, `test_reachability_target_invokes_the_checker`
- **Dimension 1.4** — DONE (by construction) — the real `test` steps are never swapped, so normal execution cannot regress. Evidenced instead by every suite staying green: daemon 1528 pass / 495 skip / 0 fail, auth 223 pass, lib 35 pass, runner 356 pass / 7 skip
- **Dimension 1.5** — DONE — a registered name maps to a file by namespace prefix, so a duplicate description cannot mask a dead file → Test `test_duplicate_description_does_not_mask_a_dead_file`

### §2 — cmd/common.zig wired in with self-updating migration assertions

Force-import `cmd/common.zig` from the agentsfleetd root so its 11 blocks compile, then repair the two stale assertions. **Implementation default:** the repaired assertions derive everything from `schema.migrations` — no literal count. Assert that `canonicalMigrations().len` equals the embedded `schema.migrations.len`, that versions are strictly monotonic and contiguous from 1, and that the last version equals the registered count. This is deliberate per RULE UFS and RULE MIG: swapping the literal `26` for `27` would only re-arm the same trap the next time a migration lands; a derived assertion never drifts.

- **Dimension 2.1** — DONE — after wiring, the checker (§1) sees `cmd/common.zig` as reachable and all 11 of its blocks register → proven by `check_zig_test_reachability.py --check` exiting 0
- **Dimension 2.2** — DONE — `canonicalMigrations().len` equals `schema_migrations.len` and the last version equals that count, with no literal migration count in the file → Test `canonical schema bootstrap: last version equals the registered count`
- **Dimension 2.3** — DONE — migration versions are strictly increasing and contiguous from 1 → Test `canonical migrations: versions are contiguous and strictly increasing`
- **Dimension 2.4** — DONE — a fixture migration list with a duplicated or gapped version fails the contiguity assertion → Test `canonical migrations: a gapped or duplicated version is rejected`

Both stale assertions failed the moment they compiled (`expected 26, found 27`), exactly as predicted. `V_CONNECTOR_INSTALLS`/`V_CHANNEL_TABLES`/`FIRST_MIGRATION_VERSION` are named constants (RULE UFS); no bare `26` survives.

### §3 — Enable the remaining unreachable set; triage what fires

Every file the §1 checker lists is force-imported under one root, or waived with a stated reason — the set is the checker's output, not a number written into this spec. A newly-executing test that fails is the payoff, not an obstacle. **Implementation default (gate-flag triage, per AGENTS.md):** a mechanical cause (a stale literal, a renamed symbol, a moved import path) is auto-repaired in the same diff and reported in one line; a judgment call (a genuine product bug or a weakened guarantee) STOPS and is surfaced to Indy as fix-or-defer. An agent-unilateral deferral is incomplete scope, not deferral — it blocks CHORE(close) without an Indy-acked quote in Discovery. Two of `common.zig`'s previously-dead blocks are the guards for findings the Jul 09 audit re-discovered independently (see Failure Modes).

- **Dimension 3.1** — DONE — the §1 checker exits zero across the whole tree: 351 files reachable, 0 dead, 0 waived
- **Dimension 3.2** — DONE — the previously-dead SqlStatementSplitter and concurrent-migration-race guards now execute in the daemon suite (both live in `cmd/common.zig`)

**Dead set (compiler truth, not an estimate): 7 files, 26 blocks.** All 7 were force-imported; none was waived.

| File | Blocks | Triage outcome |
|------|--------|----------------|
| `cmd/common.zig` | 11 | 2 failed on wiring (`expected 26, found 27`) — **mechanical**, repaired in §2. Re-arms the SqlStatementSplitter parse guard, the concurrent-migration-race guard, and a migrator-role separation guard |
| `cmd/preflight_test.zig` | 5 | did not **compile**: `std.posix.setenv`/`unsetenv` removed in Zig 0.16, two pre-refactor call arities, moved signal-handler type — **mechanical**, rebuilt on `common.env.fromPairs` |
| `cmd/doctor.zig` | 2 | passed on wiring |
| `cmd/doctor_args.zig` | 2 | passed on wiring |
| `cmd/doctor_render.zig` | 1 | crashed on wiring: **real product leak**, 5 allocations per `agentsfleetd doctor`. **Judgment** → surfaced to Indy → fixed (Discovery) |
| `cmd/serve_shutdown.zig` | 1 | passed on wiring |
| `lib/common/backoff.zig` | 4 | passed on wiring (jitter bounds, cap saturation) |

### §4 — Depth gate counts only reachable blocks

`_lint_zig_test_depth` counts `^test "` textually across `src/`, crediting blocks that cannot compile, so Test Baseline / Test Delta — the mechanism VERIFY uses to prove tests were added — is unsound. Recount from the compiler-registered set the §1 checker already computes. **Implementation default:** the recipe reads the checker's reachable count rather than re-deriving it, so the two can never disagree.

- **Dimension 4.1** — DONE — `_lint_zig_test_depth` counts only blocks in files proven reachable, which is ≤ the textual `^test "` count and excludes any dead file → Tests `test_depth_counts_registered_only`, `test_count_never_exceeds_the_textual_block_count`, `test_depth_gate_consumes_the_checker_count`
- **Dimension 4.2** — DONE — un-wiring one currently-reachable file (a fixture mutation) drops the reachable count and turns the depth gate red → Test `test_depth_gate_red_on_unwire`

**What `--count` counts.** Not the raw registered-name set: a file reachable from two roots (every `src/agentsfleetd/auth/**` file registers in both the daemon and auth binaries) would be double-counted, and anonymous barrel `test {}` blocks would be credited. Summing registered names across the 8 binaries gives 2913 — *above* the textual 2395, which would make this Dimension's own "≤ textual" assertion false. Registration is per-file, so `--count` instead sums the `^test "` blocks of every file the compiler proved live. That is compiler-grounded, comparable to the historical metric, and ≤ it by construction.

## Interfaces

```
scripts/check_zig_test_reachability.py
  --check   -> exit 0 iff every src/**/*.zig with a `test "` line registers >=1 test
               in some binary OR carries `// no-test-root: <reason>` with a NON-EMPTY
               reason; exit non-zero listing each offending path otherwise. Also
               rejects a filename whose namespace would be ambiguous (a `.` before
               `.zig` collides with a directory separator). Waived files are named
               with their reason, excluded from the reachable count, and a waiver on
               a file that DOES register tests is reported as stale.
  --count   -> print `reachable_test_cases=<N>` and `reachable_integration_cases=<M>`.
  --counts-out PATH
            -> with --check, also write those two lines to PATH. Listing all 8 binaries
               costs ~10s, so `_lint_zig_test_depth` consumes this artifact instead of
               paying for a second, identical listing (it depends on the reachability
               target for exactly that reason).

Waiver grammar (source line, exact):  // no-test-root: <reason>   (reason required)

Build side: `zig build list-tests` (defined in BOTH graphs) attaches a list-only
  compilation per test binary via src/build/test_list.zig, carrying
  src/build/test_runner_list.zig (`mode: .simple`). It prints one `ROOT\t<dir>` line
  per lane, then `TEST\t<root_dir>\t<name>` per registered test. Each TEST line
  carries its own root because zig build runs lanes on a thread pool; positional
  attribution would misfile a test the day two lanes interleave on stdout.
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
| R1 | Reachability checker self-tests pass (§1/§4) | `python3 -m unittest discover -s scripts -t scripts -p 'check_zig_test_reachability*_test.py'` | exit 0 | P0 | ✅ `Ran 33 tests … OK` |
| R2 | No dead test-bearing file remains (§1/§3) | `python3 scripts/check_zig_test_reachability.py --check` | exit 0, no paths printed | P0 |  ✅ `✓ [zig] test-root reachability: 351 file(s) reachable` |
| R3 | `cmd/common.zig` is force-imported (§2) | `grep -n 'cmd/common.zig' src/agentsfleetd/tests.zig` | ≥1 match | P0 |  ✅ `1` match |
| R4 | No bare migration-count literal survives (§2) | `grep -nE '@as\([a-z0-9]+, 2[0-9]\)' src/agentsfleetd/cmd/common.zig` | no output | P0 |  ✅ no output (grep rewritten — the old `\|` was a literal pipe) |
| R5 | Depth gate counts registered blocks (§4) | `make _lint_zig_test_depth` | exit 0, count == checker `--count` | P0 |  ✅ `✓ [zig] test depth gate passed (unit=2401 integration=267)` |
| R6 | Daemon suite green with the newly-wired blocks (§2/§3) | `make test-unit-agentsfleetd` | exit 0, 0 failures | P0 |  ✅ `1532 pass, 495 skip` (0 fail) |
| R7 | Diff stays inside Files Changed | `git diff --name-only origin/main` | 0 paths missing from the Files Changed table | P0 |  ✅ 22 files, 0 missing from Files Changed |
| S1 | Zig lint clean incl. the new gate | `make lint-zig` | exit 0 | P0 |  ✅ exit 0 |
| S2 | Cross-compile | `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` | exit 0 | P0 |  ✅ exit 0 both targets |
| S3 | No secrets | `gitleaks detect` | exit 0 | P0 |  ✅ exit 0 |
| S4 | No oversize source file | `git diff --name-only origin/main \| grep -v '\.md$' \| xargs wc -l 2>/dev/null \| awk '$1>350 && $2!="total"'` | no output | P0 |  ✅ no output (after the FLL splits) |

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
- Fixing the product bugs a newly-executing test may expose (the SqlStatementSplitter mis-split and the pool-migrations advisory-lock ordering finding each get their own spec, M123_002) — this spec makes the guards run and triages what fires; it does not carry their fixes. **Amended at EXECUTE:** the exception is a bug whose newly-live test *fails*, since the suite cannot be left red and waiving the test would silence a guard. Exactly one qualified — the `agentsfleetd doctor` detail leak (§3, Indy-approved, Discovery). The two named bugs above do not: their guards now run and pass, because both are latent rather than live.
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

### Consults — gate-flag triage

- **Runner wiring (design fork, surfaced at PLAN).** Zig 0.16's `TestRunner` requires choosing `mode: .simple | .server`, and the real suites' `--summary all` accounting comes from the build system over `std.zig.Server`. The spec's literal instruction (swap `.test_runner` on the existing steps) would break that output or force a 460-line compiler-internal fork. Presented three forms to Indy with the build-graph consequences drawn out.
  > Indy (2026-07-09): selected **"A: Parallel list-only lane"** — leave all 8 `addTest` steps on the default runner; attach a second list-only compilation sharing each root module. Amends the spec's Implementation default + Files Changed.

- **`agentsfleetd doctor` memory leak (judgment flag, surfaced at EXECUTE §3).** Wiring `cmd/doctor_render.zig` into a test root made its dead `"dynamic check details stay valid through render with GPA"` block compile for the first time; it aborted on a leak assertion. Root cause: `CheckResult.detail` carried mixed ownership — 25 `appendCheck` sites pass string literals (borrowed), 5 `appendFmtCheck` sites pass `allocPrint` results (owned) — and `doctor.zig` freed only the `ArrayList` buffer. The comment at `doctor.zig:79` claimed callers "must provide an arena-style allocator"; `main.zig:139` passes a `DebugAllocator` straight through, so the claim was false and the binary leaked 5 allocations per `doctor` run. Bounded (the process exits immediately) but real, and waiving the test would have been silencing a guard.
  > Indy (2026-07-09): selected **"A: Own the detail"** — `appendCheck` copies every detail, `freeResults` is the single free path, the false comment is deleted. Carried in this workstream because the alternative was a red suite.

  This **amends the spec's Out of Scope**, which anticipated only the two *known* product bugs (SqlStatementSplitter mis-split, advisory-lock ordering) and assumed each could be deferred to its own spec. Those two remain deferred and untouched — their guards now merely *run*. This third bug blocked the suite and could not be. One caveat recorded honestly: the test body changed by one line (`results.deinit` → `freeResults`), but its `gpa.deinit() == .ok` leak assertion — the part that caught the bug — is untouched.

- **Architecture / Legacy-Design consult** — none required; no flow, stream, namespace, or trust boundary changed.

### Empirical findings

- **Zig's test-collection rule, established by experiment, not by reading.** A file's `test` blocks register only when the file is force-referenced at comptime (`test { _ = @import("x.zig"); }` or `comptime { _ = x; }`) from a file that is itself registered. A plain `const x = @import("x.zig")` registers nothing even from an analyzed file. This matters because `src/agentsfleetd/tests.zig` → `main.zig` → `cmd/migrate.zig` → `cmd/common.zig` is a real, unbroken import chain — a barrel grep looks reassuring — yet collection does not travel down it.
- **Dead set: 7 files, 26 blocks** (see §3). Two of the 26 were provably false (`migrations.len == 26` against 27 embedded migrations); one file (`preflight_test.zig`) no longer compiled at all; one (`doctor_render.zig`) exposed a live product leak.
- **`src/build/s3.zig` owns an eighth `addTest`** the spec's Files Changed never listed; a correct implementation would have failed rubric row R7 as written.

### Corrections to this spec's own record

- **The predicted negative Test Delta did not occur — the mechanism was right, the sign was wrong.** At CHORE(open) this section warned that §4's corrected counter would read *below* the inflated `Test Baseline: unit=2395 integration=267`, since that baseline credits blocks that never compile. It measured `unit=2369 integration=261` mid-EXECUTE, exactly −26/−6 (the phantom population). But §3 wired **every** dead file rather than leaving any phantom, so at close the corrected count is `unit=2401 integration=267` — **+6 unit**: the two migration-contiguity tests from §2, plus four added while closing `/write-unit-test` and `/review`. The corrected count now equals the textual count *because zero dead files remain*; it would diverge again the instant one is added, and the reachability gate fires first. The CHORE(open) warning is left above in the header rather than deleted, and superseded here: predicting a number and then measuring it is the point.
- **Baseline honesty** — `2395/267` was recorded verbatim rather than pre-corrected, because CHORE(open) records what the gate said at open. Substituting a hand-computed "true" baseline would have fabricated a measurement no command produced.

### Remaining

- **Metrics review** — not applicable: build-time gates and a test runner; no runtime event added, renamed, or removed.
- **Skill-chain outcomes**
  - `/write-unit-test` — diff ledger 24/24 resolved (3 `won't-test` with reasons: the list runner cannot execute inside a test binary; build-graph wiring has no unit surface; no concurrent or hot-path surface exists). Checker branch coverage 69% → 99%. Mutation on changed lines 5/5 killed, including a mutant that restored the spec's original description-matching rule. Red-green confirmed: pre-fix the same suite reported `2 fail, 1 crash`.
  - `/review` — adversarial diff review, with an independent fresh-context pass. **Seven findings, all closed:**
    1. **CRITICAL — the depth gate failed open.** `_lint_zig_test_depth` ran `counts=$(… --count)` with no `set -e`. When the checker errored, `unit_count` was empty, `[ "" -lt 25 ]` raised *"integer expression expected"* and returned 2, the `if` treated non-zero as false, and the recipe **printed success and exited 0** while writing blank counts. The gate whose entire purpose is to make a number trustworthy would silently pass whenever `zig build list-tests` failed. The old textual recipe could not fail this way, because `wc -l` always emits a number. Fixed with `set -eu` plus an explicit numeric guard; both mutants (non-zero exit, non-numeric output) now fail the build.
    2. **R4 was a no-op.** `grep -nE '…\|…'` — the `\|` a markdown table forces is a *literal pipe* in an extended regular expression, not alternation. R4 matched nothing regardless of the code, so the earlier "clean" grading of R4 was worthless. Rewritten without a pipe (`@as\([a-z0-9]+, 2[0-9]\)`), verified to catch a planted literal and to ignore `@as(i32, @intCast(...))`.
    3. **Waivers laundered into "reachable".** `run_check` printed `len(candidates)` as the reachable count, which includes waived files, and never named them. Now waived files are named with their reason, excluded from the count, and a waiver on a file that *does* register tests is reported as stale.
    4. **An empty waiver reason waived the file.** `// no-test-root:` with nothing after it passed the gate. A silent opt-out is how a block goes dark; a reason is now required.
    5. **Positional attribution was fragile.** `TEST` lines were attributed to the most recent `ROOT` line. `zig build` runs lanes on a thread pool, so interleaved stdout would misfile a test and flip a file's live/dead verdict. Each `TEST` line now carries its own root dir.
    6. **The namespace map was not injective.** `a/b/c.zig` and `a/b.c.zig` both map to `a.b.c`, so a live file could mask a dead twin — the same unsoundness for which description-matching was rejected. Latent (no such filename exists) and now rejected outright.
    7. **`lint-zig` paid for the ~10s listing twice** (`--check` then `--count`). `--check --counts-out` now hands its counts to the depth gate, which depends on it. Standalone `make _lint_zig_test_depth`: 6.3s, one listing.

    Found clean by the independent pass and re-verified: the doctor ownership fix (every `results` list is freed via `freeResults`; `id` is never freed and is always a literal; no double-free or borrowed-slice free remains), root coverage (all 8 `addTest` steps have lanes; `src/build/**` has zero test blocks), per-root namespace scoping against cross-root masking, lanes correctly omitting `.filters`, and fixtures attached before each `addLane`.
  - `kishore-babysit-prs` — runs after the push.
- **Deferrals** — none. The two product bugs named in Out of Scope (SqlStatementSplitter mis-split, advisory-lock ordering) were already spec'd out to M123_002 at authoring time and are not deferrals introduced here.
