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

# M174_001: Deterministic timing assertions — no test fails for being run on a busy machine

**Prototype:** v2.0.0
**Milestone:** M174
**Workstream:** 001
**Date:** Aug 20, 2026
**Status:** DEFERRED
**Priority:** P2 — a suite that fails under load teaches everyone to rerun instead of read, which is how a real regression ships behind a shrug; three separate tests failed this way in one afternoon and each one cost a full diagnosis before it could be dismissed
**Categories:** API, UI
**Batch:** B1 — single workstream, no parallel sibling
**Branch:** {feat/m174-deterministic-timing — added at CHORE(open)}
**Test Baseline:** set at CHORE(open) — `unit=<N> integration=<M>` via `make _lint_zig_test_depth`
**Depends on:** none. M173_001 runs on a different axis (lines never executed, versus lines that execute and assert the wrong thing) and touches mostly different files; if both are active, M173's new tests must satisfy §4's margin rule like any other.
**Provenance:** LLM-drafted (Claude Opus 5, Aug 20, 2026) — grounded in a classification of all 36 wall-clock assertions across 18 Zig test files and the TypeScript timing surface, read from source rather than sampled
**Canonical architecture:** `docs/architecture/testing.md` §Coverage, §Adding a component

---

## Overview

**Goal (testable):** no test asserts a wall clock where the property under test is ordering or termination-with-huge-headroom, and every wall-clock assertion that remains states the margin between the value it expects and the bound it enforces — so no test can fail for having been run on a busy machine or under instrumentation.

**Problem:** three tests failed in one afternoon for reasons that had nothing to do with the code under test. `patch_concurrent` asserted a client-side stopwatch for a timeout Postgres enforces server-side, and the coverage lane's instrumentation spent its 500ms of slack many times over. `FleetLibrariesView` looked for a dialog it had rendered correctly, one second not being long enough to mount it under full-suite parallelism. `stream_registry` ran 60-millisecond rounds on a loaded runner. Each looked exactly like a real regression until it was read closely, and each cost that reading. The suite's own signal is what erodes: a test that cries wolf teaches its readers to rerun rather than investigate, and the next genuine failure gets the same shrug.

**Solution summary:** classify every timing assertion by the property it is actually pinning, then convert the ones that have a non-clock mechanism available — ordering becomes a causal signal between threads, complexity becomes a counter ladder. Termination ceilings are legitimate and stay, but each must state its margin: `db/pool_test.zig` bounds fifteen milliseconds of work at one second and has never flaked, while the assertion removed this week allowed five hundred milliseconds over a five-second wait. A lint keeps the class from regrowing, because a one-time sweep decays exactly as a coverage sweep does.

## PR Intent & comprehension handshake

- **PR title (eventual):** test: assert ordering and margins, not stopwatches
- **Intent (one sentence):** make a red suite mean broken code, so nobody learns to rerun a failure instead of reading it.
- **Handshake** — the implementing agent fills this at PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `src/agentsfleetd/http/handlers/fleets/patch_concurrent_integration_test.zig` — the worked example, landed by M172_001. A holder thread that releases on a signal instead of a timer, and an assertion that reads the release flag the instant the response arrives. Read the commit that produced it before converting the second one.
2. `src/agentsfleetd/db/pool_test.zig` around the migration-lock probe — the model termination ceiling. It bounds work that takes about fifteen milliseconds at one second and says why in a comment: a hang would take minutes. That ratio, not the presence of a clock, is what makes it sound.
3. `src/agentsfleetd/queue/redis_subscriber_test.zig` — the floor assertions (`elapsed >= FLOOR`). Floors prove a timeout was honoured rather than short-circuited, and load only makes them more true. They are the one shape this spec leaves alone; read them so they are not converted by reflex.
4. `docs/architecture/testing.md` §Coverage — which lane runs under kcov. Any assertion that survives this spec must hold under that instrumentation, on macOS as well as Linux.
5. `docs/greptile-learnings/RULES.md` §UFS — every bound this spec keeps becomes a named constant carrying its margin, which is the same rule that bans bare semantic literals elsewhere.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `src/agentsfleetd/**/*_test.zig` | EDIT | conversions and margin-carrying constants across the daemon's timing assertions |
| `src/runner/**/*_test.zig` | EDIT | same, for the runner component |
| `src/lib/**/*_test.zig` | EDIT | same, for the shared library component |
| `src/agentsfleetd/http/stream_registry_test.zig` | EDIT | the 60-millisecond round test that flaked on Continuous Integration (CI) during M172_001, still unconverted |
| `ui/packages/app/**/*.test.ts` | EDIT | fixture clocks made deterministic; any explicit per-assertion timeout justified or removed |
| `ui/packages/app/**/*.test.tsx` | EDIT | same |
| `scripts/check_timing_assertions.py` | CREATE | the lint: a wall-clock assertion must name a constant, and that constant must carry its margin |
| `scripts/check_timing_assertions_test.py` | CREATE | its self-test, discovered by the `*_test.py` pattern `make lint-governance` already runs |
| `make/quality.mk` | EDIT | wire the lint into `lint-governance` beside the other convention gates |
| `docs/architecture/testing.md` | EDIT | record the margin rule and the ordering-over-stopwatch preference as the standing shape |
| `docs/v2/*/M174_001_P2_API_UI_DETERMINISTIC_TIMING_ASSERTIONS.md` | EDIT | lifecycle status |

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — **UFS** (every retained bound becomes a named constant carrying its margin, never a bare literal), **NDC** (a stopwatch replaced by an ordering assertion leaves no dead helper behind), **ORP** (deleting a platform probe orphans its constants; sweep them in the same commit, as the `patch_concurrent` conversion already did).
- **`dispatch/write_zig.md`** — the bulk of the diff is Zig test code: thread spawn and join pairing, atomic ordering on the signals this spec introduces, and the pg-drain rule wherever a converted test holds a connection.
- **`docs/architecture/testing.md`** — which component owns a test decides which lane runs it, and therefore whether it runs under kcov; §4's instrumentation rule depends on getting that right.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | yes — most of the diff is Zig | cross-compile both linux targets; the signals introduced are `std.atomic.Value(bool)`, which needs no target-specific handling |
| PUB / Struct-Shape | no — this spec adds no public surface | conversion helpers stay file-private; a helper a second file wants moves to an existing test-support module rather than becoming `pub` |
| File & Function Length (≤350/≤50/≤70) | yes | test files are exempt from the file cap; the per-function cap still binds, and a converted test that outgrows it splits its holder setup into a named helper |
| UFS (repeated/semantic literals) | yes — this is the gate the spec leans on | every retained bound becomes a named constant whose declaration carries its margin; the new lint enforces exactly this |
| UI Substitution / DESIGN TOKEN | no — the TypeScript work is test files only, no rendered surface changes | N/A |
| LOGGING / LIFECYCLE / ERROR REGISTRY / SCHEMA | LIFECYCLE yes; others no | converted tests spawn and join threads and hold pooled connections, so init/deinit pairing is live. No schema change, no new error codes, no new log lines |

## Prior-Art / Reference Implementations

- **Reference:** `src/agentsfleetd/http/handlers/fleets/patch_concurrent_integration_test.zig` as landed by M172_001 — the ordering conversion in full: signal-released holder, flag read at the instant the response arrives, backstop poll so a hung subject fails the test instead of deadlocking it, and the platform-specific tracer probe deleted rather than ported.
- **Reference:** `src/agentsfleetd/db/pool_test.zig`'s migration-lock probe — the margin-carrying ceiling, including the comment that states the ratio. The margin rule generalises what that test already does by hand.
- **Reference:** the `/write-unit-test` skill's production-safety guidance, which the repository already follows elsewhere: prefer counters over clocks, and where a clock is unavoidable use median-of-K against a pinned baseline rather than a single sample.

## Sections (implementation slices)

### §1 — One ledger of every timing assertion

Thirty-six wall-clock assertions live across eighteen Zig test files, plus a smaller TypeScript surface. They are not one problem: some pin termination, some pin that a timeout was honoured, some read a stored expiry, and a few pin ordering while pretending to pin duration. Converting them by reflex would break the sound ones. The ledger is what makes the rest of the spec safe to execute.
**Implementation default:** classify by the property the assertion pins, not by the syntax it uses, because `elapsed < BOUND` appears in three of the four classes and means something different in each.

- **Dimension 1.1** — every timing assertion in the Zig tree is classified as termination, floor, value, or ordering, and the ledger names the file, the line, and the class → Test `test_timing_ledger_covers_every_assertion`
- **Dimension 1.2** — every timing assertion in the TypeScript test suites is classified the same way → Test `test_timing_ledger_covers_typescript`
- **Dimension 1.3** — the ledger is generated, not hand-maintained, so it cannot drift from the source it describes → Test `test_timing_ledger_regenerates_clean`

### §2 — Ordering replaces the stopwatch where the property is causal

An assertion that a response arrived before another thread released a lock is an ordering claim. Written as a stopwatch it inherits every source of delay in the process, which is why instrumentation and machine load can fail it while the behaviour is perfectly correct. Written as a signal between the threads it is immune to both.
**Implementation default:** the subject under test releases on a signal rather than a timer, and the assertion reads the signal at the instant the observed event arrives, because any work between the event and the read is time the other side could use to change the answer.

- **Dimension 2.1** — every assertion classified as ordering asserts a causal signal instead of an elapsed duration → Test `test_ordering_assertions_carry_no_clock`
- **Dimension 2.2** — a subject that hangs fails its test rather than deadlocking the suite, via a backstop far above every timeout it could legitimately wait on → Test `test_hung_subject_fails_not_hangs`
- **Dimension 2.3** — `stream_registry_test`'s straggler rounds assert the re-arm they exist to prove rather than a sixty-millisecond window → Test `test_await_empty_rearms_without_a_clock`

### §3 — Floors and value assertions are left alone, deliberately

Two classes look like the problem and are not. A floor (`elapsed >= FLOOR`) proves a timeout was honoured rather than short-circuited, and load only makes it more true. A value assertion reads a stored expiry or Time To Live (TTL) out of Redis and compares it to what was written — no clock the test controls is involved. Converting either would remove real coverage in the name of tidiness.
**Implementation default:** leave both untouched, with one exception: an assertion that compares a stored expiry against a bound with slack for the test's own runtime is clock-sensitive despite its shape, and takes §4's margin rule.

- **Dimension 3.1** — every floor assertion survives this spec unchanged, and the ledger records why → Test `test_floor_assertions_unchanged`
- **Dimension 3.2** — value assertions that carry runtime slack are identified and take a stated margin → Test `test_slacked_value_assertions_carry_margin`

### §4 — Every surviving ceiling states its margin

A termination ceiling is a legitimate way to prove something returned rather than hung, and the sound ones already exist: one test bounds about fifteen milliseconds of work at one second and has never flaked, because a hang there takes minutes. The assertion removed this week allowed five hundred milliseconds over a five-second wait, which instrumentation ate whole. The difference is the ratio, and today it is invisible — a reader cannot tell a hundred-fold margin from a ten-percent one without measuring the subject.
**Implementation default:** the bound becomes a named constant whose declaration states the expected value and the resulting ratio, because a margin nobody can see is a margin nobody maintains.

- **Dimension 4.1** — every retained ceiling names a constant, never a bare literal → Test `test_ceilings_use_named_constants`
- **Dimension 4.2** — every such constant's declaration states the expected value and the margin over it → Test `test_ceiling_constants_state_their_margin`
- **Dimension 4.3** — a ceiling whose margin falls below the stated minimum is either widened or converted under §2, never shipped tight → Test `test_no_ceiling_below_minimum_margin`
- **Dimension 4.4** — every surviving ceiling holds under kcov on both macOS and Linux, with no platform-conditional stand-down → Test `test_ceilings_hold_under_instrumentation`

### §5 — The TypeScript surface

The dominant TypeScript defect was ambient rather than per-test: testing-library's one-second async ceiling, which a Radix dialog exceeds under full-suite parallelism. M172_001 raised it suite-wide. What remains is smaller and different in kind — fixture data built from `Date.now()`, which makes a test's inputs different on every run.
**Implementation default:** fixture clocks are pinned to a fixed epoch rather than read from the system, because a fixture that changes every run is a test whose failures cannot be reproduced from its source.

- **Dimension 5.1** — no test fixture derives its data from the system clock → Test `test_fixtures_use_a_pinned_epoch`
- **Dimension 5.2** — every explicit per-assertion timeout either states why the ambient ceiling is wrong for it, or is removed → Test `test_explicit_timeouts_are_justified`

### §6 — A lint, so the class cannot regrow

Every sweep decays without a gate; the coverage floors in `docs/architecture/testing.md` exist for exactly this reason. A tight-margin stopwatch is easy to write and reads as reasonable, so the next one will land unless something refuses it.
**Implementation default:** the lint runs inside `lint-governance` rather than as its own target, because that is where the repository's other convention gates already live and a gate nobody runs is worse than no gate.

- **Dimension 6.1** — a wall-clock assertion against a bare literal fails the lint → Test `test_lint_rejects_literal_bounds`
- **Dimension 6.2** — a bound constant without a stated margin fails the lint → Test `test_lint_rejects_unstated_margin`
- **Dimension 6.3** — the lint runs under `make lint-governance` and its self-tests run with it → Test `make lint-governance`
- **Dimension 6.4** — the lint is proven against a fixture that would have caught the assertion M172_001 removed → Test `test_lint_catches_the_known_regression`

## Interfaces

```
No wire interface changes. No endpoint, request shape, response shape, error
code, log line, metric, or schema object is added, removed, or altered.

The machine-readable surfaces this spec adds:

  scripts/check_timing_assertions.py
    --ledger          emit the classification for every timing assertion
    --check           fail on a bare-literal bound or an unstated margin
    --min-margin N    the ratio below which a ceiling is refused

  make lint-governance
    gains the timing-assertion gate beside the existing convention checks

Every edit lands in test files, the lint, its self-test, the make wiring, and
the architecture doc. A change to any shipped code path is out of scope and
reverts.
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Conversion removes real coverage | A floor or value assertion is converted by reflex because it matched on syntax | §3 pins both classes as untouched and the ledger records the reason; a diff that changes one fails review |
| Ordering signal read too late | The assertion reads the release flag after unrelated work, so the other side had time to change it | The reference conversion reads the flag as the next statement after the observed event; any gap is a review finding |
| Backstop masks a hang | A hung subject is released by the backstop and then answers normally, so the test passes | The backstop sits far above every legitimate timeout, and the status assertion still separates the outcomes — a released hang answers differently than a fail-fast |
| Margin stated but wrong | The declared expected value does not match what the subject actually costs | The margin is measured from a real run and recorded with the constant; a subject that changes cost fails its own ceiling before the margin misleads anyone |
| Lint false positive on a legitimate literal | A bound that is genuinely incidental rather than semantic is refused | The lint matches assertions on elapsed durations only, and its self-test pins that narrowness; a real false positive widens the test, never the exemption |
| Gate added but never run | The lint lands outside the target that Continuous Integration (CI) executes | Dimension 6.3 asserts it through `make lint-governance` itself, which is the same target the repository's other convention gates prove themselves through |

## Invariants

1. **No wall-clock assertion names a bare literal bound** — enforced by `scripts/check_timing_assertions.py --check` under `make lint-governance`.
2. **Every bound constant states its expected value and margin** — enforced by the same lint, which parses the declaration's doc comment.
3. **No surviving ceiling carries a margin below the stated minimum** — enforced by `--min-margin`, which fails the build rather than warning.
4. **No timing assertion stands down on a platform condition** — enforced by the lint refusing a platform-conditional guard around an elapsed comparison, which is the exact shape M172_001 deleted.
5. **The ledger is generated from source** — enforced by Dimension 1.3, which regenerates it and fails on any difference.
6. **No shipped code path changes** — enforced by the rubric's deletion-only check over every non-test file in the diff.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| not applicable — no product or operator signal changes | not applicable | this spec edits test files, adds a lint, and updates an architecture doc; it emits no new event, metric, or log line and alters no existing one | not applicable | not applicable | the rubric's deletion-only check proves no shipped code path, and therefore no signal, was touched |

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | e2e (gate) | `test_timing_ledger_covers_every_assertion` | every Zig timing assertion → present in the ledger with a class; zero unclassified |
| 1.2 | e2e (gate) | `test_timing_ledger_covers_typescript` | every TypeScript timing assertion → same |
| 1.3 | unit | `test_timing_ledger_regenerates_clean` | regenerate over unchanged sources → byte-identical output |
| 2.1 | integration | `test_ordering_assertions_carry_no_clock` | each converted test → passes with no elapsed comparison in its body |
| 2.2 | integration | `test_hung_subject_fails_not_hangs` | a subject stubbed to never return → the test fails at the backstop rather than blocking the suite |
| 2.3 | unit | `test_await_empty_rearms_without_a_clock` | straggler holds past one round → `awaitEmpty` re-arms and returns empty, asserted by count and ordering, not by a window |
| 3.1 | e2e (gate) | `test_floor_assertions_unchanged` | the five floor assertions → byte-identical to their pre-spec form |
| 3.2 | integration | `test_slacked_value_assertions_carry_margin` | the stored-expiry comparison carrying runtime slack → names a constant stating its margin |
| 4.1 | e2e (gate) | `test_ceilings_use_named_constants` | every retained ceiling → zero bare literals |
| 4.2 | e2e (gate) | `test_ceiling_constants_state_their_margin` | every bound constant → declaration states expected value and ratio |
| 4.3 | e2e (gate) | `test_no_ceiling_below_minimum_margin` | every ceiling's stated ratio → at or above the minimum |
| 4.4 | integration | `test_ceilings_hold_under_instrumentation` | the full suite under kcov on macOS and on Linux → zero timing failures |
| 5.1 | unit | `test_fixtures_use_a_pinned_epoch` | every test fixture → no system-clock read |
| 5.2 | unit | `test_explicit_timeouts_are_justified` | every explicit per-assertion timeout → carries a stated reason or is gone |
| 6.1 | unit | `test_lint_rejects_literal_bounds` | a fixture asserting `elapsed < 5_500` → lint fails, naming file and line |
| 6.2 | unit | `test_lint_rejects_unstated_margin` | a fixture naming a constant with no margin doc → lint fails |
| 6.3 | e2e (gate) | `make lint-governance` | the gate and its self-tests → run and pass in the target CI executes |
| 6.4 | unit | `test_lint_catches_the_known_regression` | the assertion M172_001 removed, restored as a fixture → lint fails |
| regression | integration | existing daemon, runner, library, and app suites | pass unmodified — no shipped behaviour changes anywhere in this spec |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Every timing assertion is classified (§1) | `python3 scripts/check_timing_assertions.py --ledger --unclassified-count` | `0` | P0 | |
| R2 | No ordering assertion measures a clock (§2) | `python3 scripts/check_timing_assertions.py --class ordering --with-clock-count` | `0` | P0 | |
| R3 | No bare-literal bound, no unstated margin (§4, §6) | `python3 scripts/check_timing_assertions.py --check` | exit 0 | P0 | |
| R4 | No ceiling below the minimum margin (§4) | `python3 scripts/check_timing_assertions.py --check --min-margin 10` | exit 0 | P0 | |
| R5 | No platform-conditional stand-down survives (§4) | `git grep -rnE "os\.tag ?!= ?\.linux" -- '*_test.zig' \| wc -l` | `0` | P0 | |
| R6 | The suite is green under instrumentation (§4) | `make test-integration && make test-unit-all` | exit 0 | P0 | |
| R7 | No shipped code path changed | `git diff --name-only origin/main...HEAD \| grep -vE '_test\.(zig\|ts\|tsx)$\|\.md$\|\.py$\|\.mk$'` | no output | P0 | |
| R8 | Diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 paths missing from the Files Changed table | P0 | |
| S0 | Deterministic gates pass | `make harness-verify` | exit 0 | P0 | |
| S1 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | |
| S2 | Lint clean, gate wired | `make lint-all` | exit 0 | P0 | |
| S3 | Integration passes | `make test-integration` | exit 0 | P0 | |
| S6 | Cross-compile (Zig touched) | `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` | exit 0 | P0 | |
| S7 | No secrets | `gitleaks detect` | exit 0 | P0 | |
| S9 | Orphan sweep | Dead Code Sweep greps | 0 matches | P0 | |

**Grading protocol (VERIFY):** run the Verify command verbatim; grade ONLY from its output. Graded = ✅/❌ + the one decisive output line (`342 passed`); long evidence goes to PR Session Notes with a pointer here. **Ship gate:** every row graded, every P0 ✅ → eligible for CHORE(close); any ❌ or empty cell → return to EXECUTE; a P1 ❌ ships only with an Indy-acked deferral quote in Discovery.

## Dead Code Sweep

**1. Orphaned files — deleted from disk and git.**

| File to delete | Verify |
|----------------|--------|
| N/A — no files deleted; helpers retire in place as their assertions convert | — |

**2. Orphaned references — zero remaining imports/uses.**

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| Each platform probe a §2 conversion retires, with its constants (RULE ORP) | `git grep -rn -w "<symbol>" -- src/ \| head` | 0 matches |
| Each timing helper left callerless by a conversion | `git grep -rn -w "<helper>" -- src/ \| head` | 0 matches |

## Out of Scope

- Any change to shipped code. If an assertion cannot be made deterministic because the subject itself is nondeterministic, that is a finding about the subject and becomes its own spec, not a quiet edit here.
- Performance benchmarking. This spec removes wall-clock assertions used as correctness proofs; it does not add latency budgets, baselines, or a benchmark lane. `make bench` cannot authenticate against these endpoints, which is its own piece of work.
- The coverage sweep. M173_001 owns lines that never execute; this spec owns lines that execute and assert the wrong thing.
- Test parallelism and isolation more broadly. Load is the trigger here, not the disease; reducing suite contention is a separate call with its own tradeoffs.
- Flaky tests whose cause is not timing — ordering between test files, shared fixtures, leaked state. Real, and a different mechanism.

---

## Product Clarity (authoring record)

1. **Successful user moment** — an engineer sees a red suite and reads the failure instead of pressing rerun, because in this repository red has come to mean broken. The moment is a habit, which is why the gate matters more than the sweep.
2. **Preserved user behaviour** — everything. No shipped code path changes; the diff is test files, one lint, its wiring, and an architecture doc.
3. **Optimal-way check** — the unconstrained-optimal shape removes the load sensitivity at its source by isolating the suite's resource contention, so no test competes for a machine while asserting anything about it. That is a larger change to how tests run, with its own tradeoffs, and it is out of scope here; converting the assertions is the direct route to the same observable outcome.
4. **Rebuild-vs-iterate** — iterate. Each assertion converts in place against a reference conversion that already landed and is proven under the exact instrumentation that broke its predecessor. Determinism is what this spec buys, so trading any away would be self-defeating.
5. **What we build** — a generated ledger of every timing assertion, ordering conversions where the property is causal, stated margins on every ceiling that stays, deterministic fixture clocks in the TypeScript suites, and a lint inside `lint-governance` that refuses the next tight-margin stopwatch.
6. **What we do NOT build** — benchmark infrastructure, latency baselines, changes to shipped code, test-runner isolation work, or fixes for flakes whose cause is not timing.
7. **Fit with existing features** — compounds with the coverage architecture: M173_001 adds tests, and every one of them lands under this spec's margin rule. It must not destabilise the integration lane, which is the slowest gate in CI and the one every other milestone waits on.
8. **Surface order** — N/A — no user surface. The work is test code, a lint, and a doc.
9. **Dashboard restraint** — N/A — no user surface.
10. **Confused-user next step** — an engineer whose new test trips the lint gets the file, the line, and which of the two rules it broke: a bare literal bound, or a constant that never stated its margin. The fix is in the message, not in a wiki page.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** six sections ordered so the ledger comes first and the gate comes last. Classification precedes conversion because three of the four classes share the same syntax and converting on syntax would delete sound coverage; the gate closes last because it can only encode a rule the sweep has already proven workable.
- **Alternatives considered:** (a) delete every wall-clock assertion — rejected, because termination ceilings with large margins are legitimate and cheap, and the floors prove something no other mechanism does; (b) raise every bound and move on — rejected, because it treats the symptom, leaves the reader unable to tell a sound bound from a lucky one, and regrows the moment someone writes a tight one; (c) fix the three known flakes only — rejected as the shape that produced this situation, since two of the three were already known and left; (d) fold this into M173_001 — rejected, because it runs on a different axis and would make an already-large coverage milestone unreviewable.
- **Patch-vs-refactor verdict:** this is a **refactor** of how the suite makes claims about time, not of what it tests. Every assertion keeps its subject and its intent; what changes is the mechanism it uses to prove it, and in several cases the mechanism gets strictly stronger — an ordering signal cannot be defeated by a slow machine, while the stopwatch it replaces could always be defeated by one.

## Discovery (consult log)

- **Consults** — Architecture / Legacy-Design / gate-flag triage: the question asked + Indy's decision.
- **Metrics review** — events added, extra events found during `/review`, analytics/funnel playbook update or the explicit no-change reason.
- **Skill-chain outcomes** — `/write-unit-test`, `/review`, `kishore-babysit-prs` results (order per `AGENTS.md` CHORE(close); iteration counts, findings dispositioned).
- **Deferrals** — every "deferred to follow-up" needs an **Indy-acked verbatim quote** here, format `> Indy (YYYY-MM-DD HH:MM): "<quote>" — context: <which item, why>`. An agent-unilateral deferral is **incomplete scope, not deferral**, and blocks CHORE(close) until the item lands or the quote is captured.

> Indy (2026-08-23 14:11): "m173 and m174 are zig related and must move to parked and docs/v2/done folder too - add this in your punch list" — context: this spec, parked as DEFERRED before any implementation began.
> Indy (2026-08-23 14:11): "I would like to complete the specs keep merging when the agentfleetd-rust is ready to replace the agentslfeetd runner and start testing 136 at that point i will look at m173 and m174" — reactivation condition: M181_001's acceptance rows go green (the Rust daemon is ready to replace the Zig daemon); Indy then revisits this spec, re-scoped against the Rust daemon, since its subject is the Zig test surface.
