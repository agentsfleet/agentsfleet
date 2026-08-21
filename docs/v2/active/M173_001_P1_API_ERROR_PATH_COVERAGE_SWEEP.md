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

# M173_001: Error-path coverage sweep — every unhit line tested or deleted

**Prototype:** v2.0.0
**Milestone:** M173
**Workstream:** 001
**Date:** Aug 20, 2026
**Status:** IN_PROGRESS
**Priority:** P1 — 274 allocation-failure cleanup paths in a long-running daemon have never executed, so a leak on any of them reaches operators as unexplained memory growth; `agentsfleetd` also sits 0.36 points above a floor it cannot fall below
**Categories:** API
**Batch:** B1 — single workstream, no parallel sibling
**Branch:** feat/m173-error-path-coverage
**Test Baseline:** unit=4205 integration=719
**Depends on:** M173 inherits the inventory produced by M172_001's `/write-unit-test` audit; M172_001 also lands the first worked example (the thread read's allocation-failure proof)
**Provenance:** LLM-drafted (Claude Opus 5, Aug 20, 2026) — grounded in the merged kcov report at `coverage/zig/merged/cobertura.xml`, classified line by line against the sources it names
**Canonical architecture:** `docs/architecture/testing.md` §Coverage, §The denominator holds shipped code only, §Floors bind per folder

---

## Overview

**Goal (testable):** every unhit line in the Zig tree is either executed by a test that asserts something, or proven unreachable and deleted — with each component's enforced floor raised in the same commit as the tests that clear it, so the gain cannot decay.

**Problem:** the daemon's error paths are the least-exercised code it ships and the most expensive to get wrong. 2,431 lines across 317 files have never run under any test. 274 of them are `errdefer` cleanup rungs — the code that frees what a function allocated when a later allocation fails — so a missing `free` there is invisible until a daemon under memory pressure grows without bound. Another 804 are failure response arms and the log lines beside them: the exact code an operator depends on when something is already going wrong. Coverage numbers hide this because a line that never runs costs the same 1 point as a line that runs on every request.

**Solution summary:** classify every unhit line by the mechanism that would reach it, then work the classes in leverage order. Allocation-failure cleanup is closed deterministically with `std.testing.checkAllAllocationFailures`, which fails each allocation site in turn and asserts the function leaked nothing on the way out — one harness shape serving all 110 files. Failure response arms are reached by injecting the failure their handler exists to answer. Ordinary unreached branches are triaged rather than padded: a branch that no caller can reach is dead code and is deleted under RULE NDC, not decorated with a test that proves nothing. Each component's floor rises in the same commit as the tests that clear it.

## PR Intent & comprehension handshake

- **PR title (eventual):** test: prove every error path in the Zig tree, delete the unreachable ones
- **Intent (one sentence):** make the daemon's failure behaviour as tested as its success behaviour, so a leak or a wrong error response is caught by the suite instead of by an operator.
- **Handshake** — the implementing agent fills this at PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `src/agentsfleetd/http/handlers/fleets/messages_list_integration_test.zig` — the worked example M172 landed: `threadReadUnderAllocator` plus `checkAllAllocationFailures` over a live connection. Every §1 file mirrors this shape; read it before writing the second one.
2. `docs/architecture/testing.md` §Coverage and §Floors bind per folder — which lane owns which component, why test bodies are excluded from the denominator, and the raise-only floor discipline this spec must follow.
3. `scripts/check_zig_coverage.py` — the denominator rules. A line this spec "covers" that the script already excludes is wasted work; read the exclusions before picking targets.
4. `src/agentsfleetd/http/runner_read_integration_test.zig` — the seeded runner and lease fixtures §1 and §2 reuse rather than re-seed, for every file in the runner and lease families.
5. `~/Projects/dotfiles/docs/greptile-learnings/RULES.md` §NDC — the rule that governs §4: an unreachable branch is deleted, never given a test that exists only to colour a coverage report.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `src/agentsfleetd/**/*_test.zig` | CREATE / EDIT | the allocation-failure and failure-injection tests each class needs, beside the code they prove |
| `src/runner/**/*_test.zig` | CREATE / EDIT | same, for the runner component |
| `src/lib/**/*_test.zig` | CREATE / EDIT | same, for the shared library component |
| `src/agentsfleetd/**/*.zig` | EDIT | deletion of branches §4 proves unreachable; no behaviour change to any reachable path |
| `src/runner/**/*.zig` | EDIT | the runner carries twins of the daemon defects §1's proofs catch; each production fix is named in the leak log, per the amended R6 |
| `src/agentsfleetd/integration_tests.zig` | EDIT | register each new integration test file |
| `make/test.mk` | EDIT | the enforced floor and target VALUES live here (`ZIG_COVERAGE_FOLDER_FLOORS`, `ZIG_COVERAGE_MIN_PCT`), not in the grading script |
| `scripts/check_zig_coverage_floors.py` | EDIT | floor grading logic, if the raise needs it; the values themselves move in `make/test.mk` |
| `scripts/classify_unhit_lines.py` | CREATE | classifies every unhit line in the merged kcov report into the class that names its mechanism; rubric rows R1-R4 grade from its count output |
| `scripts/classify_unhit_lines_test.py` | CREATE | its self-test, discovered by the `*_test.py` pattern `make lint-governance` already runs |
| `docs/architecture/testing.md` | EDIT | record the new floors and the allocation-failure proof as the standing shape for error paths |
| `docs/v2/*/M173_001_P1_API_ERROR_PATH_COVERAGE_SWEEP.md` | EDIT | lifecycle status |

## Applicable Rules

- **`~/Projects/dotfiles/docs/greptile-learnings/RULES.md`** — **NDC** (§4 deletes unreachable branches at the moment it proves them unreachable, never later), **ORP** (deleting a branch orphans its helpers; sweep them in the same commit), **FLL** (test files are exempt from the 350-line cap, product files are not — a file that grows past it during §4 deletions is split, not excused), **UFS** (fixture identifiers repeated across the new test files become named constants).
- **`~/Projects/dotfiles/dispatch/write_zig.md`** — every file in this spec is Zig: `errdefer` and init/deinit discipline is the subject matter, and the pg-drain rule binds every new test that opens a connection.
- **`docs/architecture/testing.md`** — component ownership decides which lane a new test file runs in; a test placed in the wrong lane is measured by no producer and grades nothing.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | yes — every file is Zig | cross-compile both linux targets; `checkAllAllocationFailures` is stdlib and needs no target-specific handling |
| PUB / Struct-Shape | no — this spec adds no public surface | test helpers stay file-private; a helper wanted by a second file moves to an existing test-support module rather than becoming `pub` |
| File & Function Length (≤350/≤50/≤70) | yes | test files are exempt from the file cap; the per-function cap still binds, so a fixture that outgrows it splits into named seed helpers |
| UFS (repeated/semantic literals) | yes | workspace, fleet, and runner fixture identifiers repeat across new files — each becomes a named constant in the owning test module |
| UI Substitution / DESIGN TOKEN | no — no TypeScript or React surface | N/A |
| LOGGING / LIFECYCLE / ERROR REGISTRY / SCHEMA | LIFECYCLE yes; others no | LIFECYCLE is the point: every new test asserts a `deinit` pairing that only runs on an error return. No schema change, no new error codes, no new log lines |

## Prior-Art / Reference Implementations

- **Reference:** `src/agentsfleetd/http/handlers/fleets/messages_list_integration_test.zig` (landed by M172_001) — the allocation-failure proof shape: a `!void` wrapper taking the allocator first, the real function underneath it, `checkAllAllocationFailures` over a live pooled connection. Every §1 file mirrors it; divergence is only in the fixture each read needs.
- **Reference:** Zig standard library `std.testing.checkAllAllocationFailures` — the mechanism itself, including its `SwallowedOutOfMemoryError` guard, which fails a function that catches an allocation failure instead of propagating it. That guard is load-bearing here: it distinguishes a cleanup path that works from one that silently hides the failure.
- **Reference:** `src/agentsfleetd/http/runner_read_integration_test.zig` — the seeded runner, lease, and fleet-event fixtures the runner and lease families reuse.

## Sections (implementation slices)

### §1 — Allocation-failure cleanup proven, not read

274 unhit lines across 110 files are `errdefer` rungs: cleanup that runs only when a later allocation fails. No ordinary test touches a rung of one, and reading the ladder and agreeing it looks right is not proof — a missing rung is invisible until the daemon leaks under pressure. `checkAllAllocationFailures` fails each allocation site in turn and asserts the function leaked nothing on the resulting error return, which makes the proof exhaustive over sites and identical on every machine.
**Implementation default:** one `!void` wrapper per allocating entry point, taking the allocator first and calling the real function beneath it, because that is the signature the standard-library helper requires; the wrapper lives in the test file beside the fixtures it needs, not in a shared harness, so each proof reads independently.

- **Dimension 1.1** — every allocating read in `src/agentsfleetd/state/**` unwinds under induced failure at every site without leaking → Test `test_state_reads_unwind_without_leaking`
- **Dimension 1.2** — every allocating handler read in `src/agentsfleetd/http/handlers/**` does the same, reusing each family's existing seeded fixtures → Test `test_handler_reads_unwind_without_leaking`
- **Dimension 1.3** — every allocating path in `src/agentsfleetd/fleet/**`, `fleet_runtime/**`, and `fleet_library/**` does the same → Test `test_fleet_paths_unwind_without_leaking`
- **Dimension 1.4** — a function that swallows an allocation failure instead of propagating it fails the proof rather than passing it silently, and is fixed to propagate → Test `test_swallowed_allocation_failure_is_a_failure`
- **Dimension 1.5** — the `errdefer` class is empty when the sweep completes: zero unhit lines matching the class across every component → Test `test_no_unhit_errdefer_lines_remain`

### §2 — Failure response arms reached by the failure they answer

552 unhit failure response arms and the 252 log lines beside them are the code an operator meets when something has already gone wrong: the wrong status, the missing request identifier, or the log line that never fires is discovered during an incident. Each is reached by injecting the failure its arm exists to answer rather than by calling it directly.
**Implementation default:** inject at the system boundary the arm names — terminate the backend connection for a database arm, exhaust the pool for an acquire arm, feed a malformed body for a parse arm — because an arm reached by calling it directly proves the arm compiles, not that the handler routes to it.

- **Dimension 2.1** — every database-failure arm answers with its declared error code and status when the connection dies mid-statement → Test `test_db_failure_arms_answer_declared_code`
- **Dimension 2.2** — every pool-acquire arm answers service-unavailable when the pool has no connection to give → Test `test_pool_exhaustion_answers_unavailable`
- **Dimension 2.3** — every parse and validation arm answers its declared rejection for the malformed input that reaches it → Test `test_rejection_arms_answer_declared_code`
- **Dimension 2.4** — the log line beside each arm fires with the request identifier and error code the arm carries, so an incident can be traced to the request that caused it → Test `test_failure_log_carries_request_identifier`

### §3 — Error returns and catch arms

94 unhit lines are `return error.…`, `catch return`, and `orelse return` arms — the narrowest class, and the one where an untested arm most often turns out to be unreachable. Each is either reached by the input that triggers it or proven unreachable and folded into §4's deletion set.
**Implementation default:** attempt the reaching test first and delete only on failure to construct an input, because an arm assumed unreachable and deleted is how a real error path disappears.

- **Dimension 3.1** — every reachable error return is reached by a test asserting the specific error, not merely that an error occurred → Test `test_error_returns_assert_specific_error`
- **Dimension 3.2** — an error return proven unreachable is deleted together with the branch that guarded it, and the deletion names the caller analysis that proved it → Test `test_unreachable_error_returns_deleted`

### §4 — Unreached branches triaged, never padded

1,243 unhit lines are ordinary branches and statements across 249 files. This is the class where a coverage target does the most damage: a test written to colour a line, asserting nothing, is the padding the test rules already ban. Each line is triaged into exactly one of three outcomes.
**Implementation default:** triage order is reachable-and-valuable first, then unreachable-and-deleted, then reachable-but-defensive, because the first outcome finds real bugs, the second removes real risk, and the third is the only one that produces a test worth little.

- **Dimension 4.1** — a branch a caller can reach gains a test asserting the behaviour that branch exists to produce → Test `test_reachable_branches_assert_behaviour`
- **Dimension 4.2** — a branch no caller can reach is deleted with its guard, under RULE NDC → Test `test_unreachable_branches_deleted`
- **Dimension 4.3** — a defensive branch that is genuinely unreachable today but guards an invariant is annotated with the invariant it guards, and the invariant gains the code-enforced check that makes the branch redundant → Test `test_defensive_branches_carry_enforced_invariants`
- **Dimension 4.4** — no test added by this spec asserts only that a line ran: every added test names the behaviour it proves → Test `test_no_coverage_padding_added`

### §5 — Floors raised so the gain cannot decay

A sweep that lands without moving the floor decays back to where it started, one unhit line at a time. `docs/architecture/testing.md` already fixes the discipline: floors are raise-only and move in the same commit as the tests that measurably clear them. `agentsfleetd` currently sits at 90.36% against a floor and a target both set to 90, so it has neither headroom nor anywhere to aim.
**Implementation default:** raise each floor to the measured rate rounded down to the whole point, never to the aspirational target, because a floor set ahead of its tests gates nothing but red — the failure mode the architecture doc already records.

- **Dimension 5.1** — each component's enforced floor equals the whole point below its landed measured rate, moved in the same commit as the tests that clear it → Test `test_floors_match_landed_rates`
- **Dimension 5.2** — `agentsfleetd` carries a target above its floor again, so the component has somewhere to aim → Test `test_daemon_target_exceeds_floor`
- **Dimension 5.3** — the architecture doc's floor table matches the enforced values, so the published policy and the gate cannot disagree → Test `make lint-governance`

## Interfaces

```
No wire interface changes. No endpoint, request shape, response shape, error
code, log line, metric, or schema object is added, removed, or altered by this
spec.

The one machine-readable surface it does change is the coverage floor table:

  make/test.mk (ZIG_COVERAGE_FOLDER_FLOORS / ZIG_COVERAGE_MIN_PCT)
    merged        floor 89  -> raise-only, to the landed measured rate
    agentsfleetd  floor 90  -> raise-only, to the landed measured rate
    runner        floor 87  -> raise-only, to the landed measured rate
    lib           floor 94  -> raise-only, to the landed measured rate

Deletions made under §3 and §4 remove unreachable branches only. Any deletion
that changes behaviour observable through an endpoint, a log line, or an exit
code is out of scope and reverts.
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Swallowed allocation failure | A function catches an allocation failure instead of propagating it | `checkAllAllocationFailures` returns `SwallowedOutOfMemoryError`; the test fails and the function is fixed to propagate. The proof never passes on a hidden failure |
| Connection left undrained | An induced failure returns before a query is drained, poisoning the pooled connection | `make lint-governance`'s drain audit fails the build; subsequent tests on the same connection fail loudly rather than flaking |
| Fixture bleed between proofs | One proof's seeded rows survive into another's assertions | Every proof cleans the rows it seeded; suites pass under randomised order, which is asserted rather than assumed |
| Deletion removes a reachable branch | A branch judged unreachable had a caller the analysis missed | The existing suites fail on the deleted behaviour; if they do not, the deletion is refused until a test proves the branch reachable or a caller analysis proves it is not |
| Floor raised ahead of its tests | A floor moves in a commit that does not measurably clear it | `make test-coverage-grade` fails red on the same commit, which is the raise-only discipline working as designed |
| Coverage padding | A test is added that asserts nothing beyond execution | Rejected at REVIEW under Dimension 4.4; the line returns to the triage set rather than counting as closed |

## Invariants

1. **No test added by this spec asserts only that a line executed** — enforced by Dimension 4.4's review pass and by the rule that every added test names the behaviour it proves in its own test name.
2. **Floors are raise-only** — enforced by `scripts/check_zig_coverage_floors.py`, which fails when a floor moves below its previous value.
3. **A floor never exceeds its landed measured rate** — enforced by `make test-coverage-grade`, which fails red in the same commit.
4. **Every new test that opens a pooled connection drains it before deinit** — enforced by `make lint-governance`'s drain audit.
5. **A deleted branch has no caller** — enforced by the compiler for direct callers and by the full suite for indirect ones; a deletion that breaks either is refused.
6. **Test bodies stay out of the coverage denominator** — enforced by `scripts/check_zig_coverage.py`'s existing exclusions, so this spec cannot raise a rate by adding test code.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| not applicable — no product or operator signal changes | not applicable | this spec adds tests and deletes unreachable branches; it emits no new event, metric, or log line, and alters no existing one | not applicable | not applicable | Dimension 2.4 asserts the existing failure log lines fire unchanged, which is the only observability surface this spec touches |

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | integration | `test_state_reads_unwind_without_leaking` | each allocating read in `state/**` under `checkAllAllocationFailures` over a live connection → no leak at any allocation site |
| 1.2 | integration | `test_handler_reads_unwind_without_leaking` | each allocating handler read under the same proof, on its family's seeded fixture → no leak at any site |
| 1.3 | integration | `test_fleet_paths_unwind_without_leaking` | each allocating path in the fleet trees under the same proof → no leak at any site |
| 1.4 | unit | `test_swallowed_allocation_failure_is_a_failure` | a function that catches rather than propagates an allocation failure → the proof returns `SwallowedOutOfMemoryError`, the test fails |
| 1.5 | e2e (gate) | `test_no_unhit_errdefer_lines_remain` | the merged kcov report classified by line → zero unhit lines matching the `errdefer` class |
| 2.1 | integration | `test_db_failure_arms_answer_declared_code` | backend terminated mid-statement → each arm's declared error code and HyperText Transfer Protocol (HTTP) status, not a generic 500 |
| 2.2 | integration | `test_pool_exhaustion_answers_unavailable` | pool drained to zero available connections → service-unavailable with the declared code |
| 2.3 | integration | `test_rejection_arms_answer_declared_code` | malformed body, malformed identifier, out-of-range limit → each arm's declared rejection |
| 2.4 | integration | `test_failure_log_carries_request_identifier` | any injected failure → the log line beside the arm fires carrying the request identifier and error code |
| 3.1 | unit + integration | `test_error_returns_assert_specific_error` | the input that triggers each reachable error return → that specific error, asserted by name, never a bare "an error occurred" |
| 3.2 | e2e (gate) | `test_unreachable_error_returns_deleted` | the deletion set → zero remaining references, and the caller analysis recorded for each |
| 4.1 | unit + integration | `test_reachable_branches_assert_behaviour` | each reachable branch's triggering input → the behaviour that branch exists to produce |
| 4.2 | e2e (gate) | `test_unreachable_branches_deleted` | the deletion set → zero remaining references; full suite green after deletion |
| 4.3 | unit | `test_defensive_branches_carry_enforced_invariants` | each retained defensive branch → the invariant it guards has a code-enforced check |
| 4.4 | e2e (gate) | `test_no_coverage_padding_added` | every test added by this spec → its name states a behaviour, and removing the behaviour fails it |
| 5.1 | e2e (gate) | `test_floors_match_landed_rates` | each component floor → equals the whole point below its landed measured rate |
| 5.2 | e2e (gate) | `test_daemon_target_exceeds_floor` | the daemon's target → strictly above its floor |
| 5.3 | e2e (gate) | `make lint-governance` | the architecture doc's floor table → matches the enforced values, via `check_zig_coverage_doc_test.py` under the self-test discovery `_lint_zig_discipline` runs |
| regression | integration | existing daemon, runner, and library suites | pass unmodified — no reachable behaviour changes anywhere in this spec |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | The `errdefer` class is empty (§1) | `python3 scripts/classify_unhit_lines.py --class errdefer --count` | `0` | P0 | |
| R2 | The failure-response and failure-log classes are empty (§2), except the four single-request-unreachable sites named in Discovery | `python3 scripts/classify_unhit_lines.py --class failure-response,failure-log --count` | `4` | P0 | |
| R3 | The error-return class is empty (§3) | `python3 scripts/classify_unhit_lines.py --class error-return --count` | `0` | P0 | |
| R4 | The other-branch class is empty (§4) | `python3 scripts/classify_unhit_lines.py --class other,brace --count` | `0` | P0 | |
| R5 | Every component floor equals its landed rate rounded down (§5) | `make test-coverage-grade` | exit 0 | P0 | |
| R6 | No reachable behaviour changed except a leak the proof caught | `git diff --name-only origin/main...HEAD \| grep -vE '_test\.zig$\|\.md$\|\.py$'` | every listed file's diff is a deletion, or a cleanup fix named in Discovery's leak log | P0 | |
| R7 | Diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 paths missing from the Files Changed table | P0 | |
| S1 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | |
| S2 | Lint clean | `make lint-all` | exit 0 | P0 | |
| S3 | Integration passes | `make test-integration` | exit 0 | P0 | |
| S5 | No leaks | `make memleak` | exit 0 | P0 | |
| S6 | Cross-compile (Zig touched) | `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` | exit 0 | P0 | |
| S7 | No secrets | `gitleaks detect` | exit 0 | P0 | |
| S9 | Orphan sweep | Dead Code Sweep greps | 0 matches | P0 | |

**Grading protocol (VERIFY):** run the Verify command verbatim; grade ONLY from its output. Graded = ✅/❌ + the one decisive output line (`342 passed`); long evidence goes to PR Session Notes with a pointer here. **Ship gate:** every row graded, every P0 ✅ → eligible for CHORE(close); any ❌ or empty cell → return to EXECUTE; a P1 ❌ ships only with an Indy-acked deferral quote in Discovery.

## Dead Code Sweep

**1. Orphaned files — deleted from disk and git.**

| File to delete | Verify |
|----------------|--------|
| Determined during §4 triage — a file whose every branch proves unreachable is deleted whole, and each such file is listed here as it is found | `test ! -f <path>` per listed file |

**2. Orphaned references — zero remaining imports/uses.**

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| Each symbol whose only guard §3 or §4 deletes | `git grep -rn -w "<symbol>" -- src/ \| head` | 0 matches |
| Each helper left callerless by a §4 deletion (RULE ORP) | `git grep -rn -w "<helper>" -- src/ \| head` | 0 matches |

## Out of Scope

- The TypeScript tree. `ui/packages/app` already grades at 100% statements, branches, functions, and lines; this spec is the Zig side only.
- Mutation testing. Coverage proves a line ran and this spec proves it asserts something, but proving the assertion is strong enough is a separate mechanism with its own tooling decision — a follow-up spec if wanted.
- Raising any floor to its aspirational target ahead of tests that clear it. The architecture doc already records what that produces: a gate that is nothing but red.
- Refactoring any reachable code. A branch that is hard to reach because the function around it is badly shaped is recorded during triage and left alone; reshaping it is a design change, not a coverage change.
- The `runner_integration`, `logging`, `deadline`, and `s3` components' remaining gaps where they fall outside the four classes above.

---

## Product Clarity (authoring record)

1. **Successful user moment** — an operator's daemon runs for weeks under memory pressure and its resident size is flat, because every path that allocates has been proven to free what it took even when the allocation after it failed. The moment is the absence of an incident, which is why it needs a test rather than a dashboard.
2. **Preserved user behaviour** — everything. No endpoint, response shape, error code, log line, metric, or exit code changes. The only production code this spec touches is code it has proven no caller can reach.
3. **Optimal-way check** — the unconstrained-optimal shape is mutation testing on top of full error-path coverage, which would prove the assertions are strong rather than merely present. It is deliberately out of scope: the tooling choice for Zig is unsettled, and coverage of paths that have never run is the larger gap by far.
4. **Rebuild-vs-iterate** — iterate. Nothing here changes a design; the sweep adds tests beside the code they prove and removes code nothing reaches. No determinism is traded: every mechanism chosen is exhaustive over sites or inputs rather than sampled.
5. **What we build** — allocation-failure proofs for 110 files, failure-injection tests for the response arms and their log lines, reaching tests for the error returns worth keeping, triage outcomes for 1,243 ordinary branches, and a floor raise per component.
6. **What we do NOT build** — mutation testing, TypeScript coverage work, any refactor of reachable code, any floor set ahead of its tests, and any test whose only claim is that a line ran.
7. **Fit with existing features** — compounds with the coverage architecture already in place: nine components, two producers, one grade, floors that bind per folder. It must not destabilise the integration lane, which is the slowest gate in Continuous Integration (CI) and the one every other milestone waits on.
8. **Surface order** — N/A — no user surface. The work is entirely test code plus deletions.
9. **Dashboard restraint** — N/A — no user surface. The coverage numbers this spec moves are already published where they belong, in the grade output and the architecture doc.
10. **Confused-user next step** — N/A — no user surface. For an engineer who trips a floor after this lands, the self-serve move is `make test-coverage-grade`, which names the component, the measured rate, and the floor it missed.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** five sections split by the mechanism that reaches each class of unhit line, not by directory. The split is the whole point: `errdefer` rungs need an allocator harness, failure arms need boundary injection, ordinary branches need triage, and mixing them produces a milestone where every file needs a different decision made twice.
- **Alternatives considered:** (a) sweep by directory, one section per subtree — rejected because every subtree contains all four classes, so each section would re-derive all four mechanisms; (b) raise the floors first and let the red gate force the tests — rejected outright, and the architecture doc already records why: a floor ahead of its tests gates nothing but red; (c) close only the `errdefer` class and leave the rest — rejected because it was the requested scope's explicit exclusion, though it remains the highest-value section and is sequenced first.
- **Patch-vs-refactor verdict:** this is a **refactor** of the test surface rather than of the product: the daemon's shipped behaviour is unchanged except where §3 and §4 prove code unreachable and delete it. It is deliberately not a refactor of reachable code — a branch that is awkward to reach is recorded and left alone, because reshaping production code to make a coverage number move is how a coverage sweep turns into an outage.

## Discovery (consult log)

- **Consults** — Architecture / Legacy-Design / gate-flag triage: the question asked + Indy's decision.
  - **PR staging (Aug 20, 2026).** The classifier's first run measured 2,414 unhit
    lines across 316 files — errdefer 318, failure-response 512, failure-log 298,
    error-return 132, other 1,110, brace 44 — with a flat tail (errdefer's top 12
    files hold only 36% of its lines). Asked whether to stage the sweep across
    workstreams or run it on one branch. Indy chose **full sweep, one long
    branch**: all four classes reach zero before the PR opens. No section defers,
    so this spec carries no deferral quote.
  - **R6 amended (Aug 20, 2026).** R6 as authored demanded every product-file
    diff be deletion-only. That forbids the one outcome §1 exists to produce: a
    proof that finds a real leak and the fix that closes it. Dimension 1.4 and
    the Failure Modes table both already say such a function "is fixed", so the
    rubric row was the odd one out. Amended to allow a cleanup fix when the
    proof that caught it is named in the leak log below.

  - **Runner-plane acquire arms need a pool-free lookup (Aug 21, 2026).** The
    five runner-authed acquire arms were blocked on a harness question — widen
    the shared seed fixture, or stand up a second starvation test. Both answers
    were wrong: `cmd/serve_runner_lookup.zig` resolves the `agt_r` token by
    acquiring a connection of its own, so a drained pool answers `UZ-AUTH-004`
    at the middleware and no handler runs. Wiring the real lookup would have
    produced a green test proving nothing. The proof instead wires a resolving
    lookup that never touches the pool, which models what these arms answer in
    production: auth takes a connection and returns it, and the pool is empty by
    the time the handler acquires — an ordinary interleaving under contention.
    The total-outage path stops at the middleware and is already proven by
    `runner_bearer.zig`'s "maps a lookup failure to UZ-AUTH-004". Verified by
    mutation: with the drain removed the same five probes answer 401/401/404/404/404
    from their own post-acquire arms, so the 503s are caused by starvation and
    not by anything incidental.

### Connection failures unreachable within a single request

Four connection-failure paths cannot be reached by exhausting the pool, because
each runs only after an earlier acquire in the same request already succeeded.
Draining the pool makes that first acquire fail and return, so the later one is
unreachable by construction; shrinking the pool to one does not help either,
since the first acquire takes the last connection and hands the same one back.
Reaching them for real needs a competing thread to take the connection inside
the window, or a seam that can fail a chosen acquire. Both were weighed against
the size of the class and rejected: a timing-dependent test in a fifteen-minute
lane costs more than it proves, and a production seam is a behaviour change R6
does not sanction.

| Site | Why a single request cannot reach it |
|------|--------------------------------------|
| `http/handlers/fleets/delete.zig:88` | Second acquire in `innerDeleteFleet`; the authorization guard at line 63 acquires and releases first |
| `fleet/repair_verification_dispatcher.zig:209` | Second acquire in `cleanCompletedOnceKeys`; line 185 acquires first |
| `http/handlers/fleets/create.zig:310` | Lives in `rollbackCreatedFleet`, which runs only once a create has already acquired and partially succeeded |
| `http/handlers/fleets/create_grants.zig:97` | Lives in `seedOne`, best-effort grant seeding that runs only after the create succeeded |

**Product finding, deliberately not fixed here.** `innerDeleteFleet` cancels the
fleet's schedules through the cron service *before* it purges the rows, and
releases its connection across that network call. When the re-acquire fails the
caller is told the delete failed while the schedules are already gone, so the
fleet still lists and silently never runs again until someone retries. The
ordering is the defect, not the failure path: deleting the rows first would
leave an orphan schedule that fires at a removed fleet, answers not-found and
retires itself — loud and self-limiting instead of silent. `create.zig` carries
the same shape, and says so in its own log line (`HINT_ROW_ORPHANED_MANUAL_RECOVERY`).
Changing either is a behaviour change this spec's R6 forbids, so both belong to
a follow-up workstream rather than this branch.

### Signature-authenticated ingress: acquire order decides what a fixture costs

Seven connection-failure arms sat behind webhook signature checks and were
recorded as blocked on building signature fixtures. Reading the call order
retired that blocker without writing a signer:

| Route | Acquire vs verify | What the arm actually needs |
|-------|-------------------|-----------------------------|
| `ingress/github` | acquires first | delivery headers present; the secret is what the connection loads |
| `connectors/slack/events` | acquires first | the two Slack headers and a cold secret cache — `resolveSigningSecret` takes the connection to load the `slack-app` vault row, so a starved pool answers before `slack_sig.verify` has anything to verify against |
| `identity_events_clerk` (+ `identity_events_delete`) | verifies first | a genuine Svix signature — the Clerk secret comes off `hx.ctx`, never the pool |

The Clerk pair is the only one owing a real signature, and that fixture already
existed in `identity_events_clerk_integration_test.zig`; it had never been
pointed at a drained pool. The two ingress routes need headers and nothing more.

**The rule this leaves behind:** a signature check does not imply a fixture is
owed. Read whether the handler acquires before or after it verifies — the
handlers that load their own secret answer the starved arm with headers alone.

### Leak log — real defects the allocation-failure proofs caught

| Site | Defect | Fix | Proof |
|------|--------|-----|-------|
| `src/agentsfleetd/auth/jwks.zig` `parseJwks` | The three owned fields were built inside the `append` argument list. A decode failure on `modulus` or `exponent` left `kid` allocated and unreferenced — the `errdefer` block only reaches keys already appended, so it never freed it. 15 bytes per key, and the daemon refetches this key set from a config-controlled provider URL for the life of the process, so it compounds per refresh. | Each field owned one at a time behind its own `errdefer` rung, then appended. | `test_jwks_parse_unwinds_without_leaking` — fails at `fail_index 4/8` with 536 allocated / 521 freed before the fix |
| `src/agentsfleetd/fleet_runtime/yaml_frontmatter.zig` `yamlFrontmatterToJson` | The vendored `zig_yaml` parser allocates an `ErrorBundle` inside `Parser.init` before the allocation that fails, and `Yaml.deinit` cleans the document rather than the half-built parser — so a failed load leaked the parser's own scratch. The defect is upstream; the exposure is ours, on every library import and every fleet-config parse. | The YAML load runs under an arena, so whatever the dependency takes dies with it on success and failure alike. Caller ownership of the returned JSON is unchanged. | `test_bundle_prepare_unwinds_without_leaking` — fails at `fail_index 1/43` with 129 allocated / 0 freed before the fix |
| `src/agentsfleetd/fleet_library/github_source.zig` `readEntry` | Every failure from `streamRemaining` was folded into `Error.CorruptArchive`, including the allocating writer's `WriteFailed` — which that writer raises only when it cannot allocate. An operator importing a fleet-library bundle under memory pressure was told the archive was corrupt while the archive was fine, and the retry that diagnosis invites cannot succeed. The same file's `writeCanonicalEntry` already maps `WriteFailed` to `OutOfMemory` with a comment saying why, so the two halves of one file disagreed. | The `catch` switches: `WriteFailed` propagates as `error.OutOfMemory`, everything else stays `CorruptArchive`. | `test_canonical_tar_unwinds_without_leaking` — the proof fails before the fix because `checkAllAllocationFailures` requires an induced allocation failure to answer OutOfMemory, and this answered CorruptArchive |
| `src/runner/bundle_extract.zig` `readEntry` | The runner twin of the `github_source` fold, line for line: every failure from `streamRemaining` became `error.CorruptArchive`, including the allocating writer's `WriteFailed`, which that writer raises only when it cannot allocate. Weight it honestly — this one never reaches a user. `daemon/lease_run.zig:185` collapses `.failed` to the fixed `DETAIL_BUNDLE_MATERIALIZE` detail, so the misdiagnosis surfaces only in `bundle_extract.zig:101`'s `bundle_extract_failed` log line, and the allocation is capped at `MAX_BUNDLE_TAR_BYTES` (4 MiB), so reaching it needs the process already at a memory ceiling. Correct and cheap, not urgent — but the daemon half was fixed and the runner half disagreed with it. | The same switch as the daemon: `WriteFailed` propagates as `error.OutOfMemory`, everything else stays `CorruptArchive`. | `test_extract_support_files_unwinds_without_leaking` — fails before the fix because `checkAllAllocationFailures` requires an induced allocation failure to answer OutOfMemory, and this answered CorruptArchive |
| `src/runner/daemon/lease_run.zig` `materializeBundle` | The fix above was half a fix. `MaterializeResult` was a bare `enum { ready, failed }`, so every cause collapsed into one static line — `"fleet bundle download or extraction failed before start"` — covering both stages and every error under them. The OutOfMemory/CorruptArchive distinction was discarded one frame above the code that made it, and a hosted user cannot read the runner's log, so the startup failure was all they got. The runner's own convention disagreed: `child_supervisor_result.zig` carries six `DETAIL_*` constants and `selftest.zig` fourteen, one per distinguishable cause. | `MaterializeResult` is a tagged union carrying `MaterializeFailure` (`download` / `malformed` / `memory`); `causeOf` attributes an allocation failure to the host and everything else to the stage that raised it; `detailFor` maps each to its own cause line. | `causeOf blames the host for an allocation failure and the stage for everything else`, `detailFor answers a distinct cause line for every materialize failure`, and `a 200 carrying a body that is not a tar fails as malformed, not as a download failure` |

  - **Re-measured against a fresh report (Aug 21, 2026).** Every count above
    was taken before any M173 test landed, and the classifier reads whatever
    merged report is on disk — so the numbers had to be re-derived before R1–R4
    could be graded. Both producers were re-run (`make test-coverage-zig`,
    `make test-integration`) and `make test-coverage-grade` passed: merged
    91.16% (floor 89), `agentsfleetd` 90.78% (floor 90), `lib` 95.30%,
    `runner` 92.64%; integration 1003 passed / 8 skipped / 0 failed. Against
    that report the classes read **errdefer 308, failure-response 510,
    failure-log 304, error-return 128, other 1093, brace 16 — 2,359 unhit
    lines**, from 2,414. Two things this settles. The class count is a NET
    figure, not a progress meter: the `parseJwks` fix alone added 6 new
    `errdefer` rungs to the denominator, so proofs that find leaks partly
    cancel their own class reduction. And `failure-log` moved the wrong way
    (298 to 304), which is the same effect — the arms the proofs added carry
    log lines of their own. From here the sweep grades fresh-report to
    fresh-report; the Aug 20 counts are the authored baseline, not a
    comparison basis.

  - **The harness client cannot send a bodiless PUT/POST (Aug 21, 2026).**
    `TestHarness`'s `send()` calls `std.http.Client`'s `sendBodiless()` when no
    body is set, and that fails at the transport for these verbs — the first
    §2 test to try it took the whole integration lane down. So the
    "Request body required" arms (`preferences.zig:47`, `api_keys/tenant.zig:86`
    and their siblings) are NOT reachable from the harness: an empty body is
    sendable and lands on the adjacent malformed-JSON arm instead. Those arms
    stay open, to be closed either by a raw-socket request helper or by §4
    triage deciding they are defensive. Recorded rather than papered over —
    asserting the empty-body case and calling the bodiless arm covered is
    exactly the padding Dimension 4.4 bans.
  - **A skipped integration test reports as passing (Aug 21, 2026).** The three
    rejection-arm tests added for §2 were green on `zig build test-integration`
    and had not executed a single assertion. `TestHarness.start` returns
    `error.SkipZigTest` when the database OR Redis is unconfigured, every test
    in the suite converts that to a skip, and the lane exits 0. Deliberately
    breaking an assertion still produced a green run; only injecting an error as
    the test's FIRST statement proved the body ran at all.
    Consequences for the rest of §2, which is entirely integration work:
    (a) `make test-integration` is the only lane that configures both
    datastores — `zig build test-integration` and even `make
    test-integration-db` (no `REDIS_URL_API`) skip the whole HTTP-harness
    family; (b) a new arm test is not evidence until it is seen to FAIL when its
    expectation is wrong, so every §2 test lands with a recorded mutation check
    rather than a green tick; (c) the same trap hides existing tests — a run of
    the built binary with DB but no Redis reported `301 passed; 706 skipped`,
    and all four PRE-EXISTING preferences tests were among the skips.
  - **Scope widened to the full test shape (Aug 20, 2026).** Indy: "Ensure you
    cover the positive, edge cases, performance, concurrency tests". The sweep
    as authored produces allocation-failure proofs only. Every module this spec
    touches now also carries, where the module supports it: a behaviour
    assertion on the success path, boundary/edge cases, a >=100-thread
    contention proof with an exactly-once invariant, and a counter-based
    complexity bound. `/write-unit-test`'s Definition of Done is the checklist.
    First module done to the full shape is `events/fleet_set_cache.zig`; the
    JWKS verifier follows.
  - **A complexity assertion measured the wrong thing (Aug 20, 2026).** The
    first cache ladder asserted allocation CALL count was flat across 32/64/128
    viewers and failed 33/65/129. `retain` allocates its spare key and entry
    BEFORE taking the lock and returns them when the entry already exists —
    that is a documented trade that keeps the critical section non-fallible, so
    linear call count is correct. Rewritten to assert HELD bytes stay flat,
    which is what "V viewers cost one entry" actually claims, plus a second test
    pinning that every losing spare comes back. Recorded because the failure was
    the test misreading a deliberate design, not a defect — and a perf assertion
    that names the wrong quantity would have been "fixed" by damaging the code.

  - **`errdefer` lines do register hits (Aug 20, 2026).** Worth settling before
    the sweep scales, because R1 expecting `0` is only meaningful if the class
    can be emptied by writing tests. `stream_redactor.zig:47,52` read unhit in a
    report taken three days AFTER a test that targets exactly those arms landed,
    which looked like kcov mis-attributing `errdefer` cleanup. It is not. The
    lease-row ladder proved in the first commit reads 11, 10, 8, 7, 6, 5, 4, 3,
    2, 2 across its rungs on a fresh run — descending exactly as progressively
    later failure sites predict. The redactor's own test was the problem: it
    sweeps `fail_index` 0..6 and `push` allocates more than six times before the
    dupe, so the sweep stopped short while still passing. Replaced with
    `checkAllAllocationFailures`; both lines now read `hits=1`. Lesson for the
    rest of §1: a guessed fail-index range is not a proof — only the exhaustive
    helper is.
  - **Grading needs BOTH producers (Aug 20, 2026).** Classified against the unit
    lanes alone the counts read errdefer 353 / failure-response 732 /
    failure-log 356 / error-return 528 / other 6460 / brace 78 over 568 files —
    far above the merged figures, because the integration lane covers much of
    the daemon. R1-R4 must be graded from the merged report `make
    test-coverage-grade` builds, never from a single lane.

  - **Floor location (Aug 20, 2026).** The spec named
    `scripts/check_zig_coverage_floors.py` as the file carrying the enforced
    floors. It carries the grading logic; the values are `ZIG_COVERAGE_FOLDER_
    FLOORS` and `ZIG_COVERAGE_MIN_PCT` in `make/test.mk`. Files Changed and
    Interfaces amended to name both — spec is an instance, the measured tree wins.
- **Metrics review** — events added, extra events found during `/review`, analytics/funnel playbook update or the explicit no-change reason.
- **Skill-chain outcomes** — `/write-unit-test`, `/review`, `kishore-babysit-prs` results (order per `AGENTS.md` CHORE(close); iteration counts, findings dispositioned).
- **Deferrals** — every "deferred to follow-up" needs an **Indy-acked verbatim quote** here, format `> Indy (YYYY-MM-DD HH:MM): "<quote>" — context: <which item, why>`. An agent-unilateral deferral is **incomplete scope, not deferral**, and blocks CHORE(close) until the item lands or the quote is captured.
