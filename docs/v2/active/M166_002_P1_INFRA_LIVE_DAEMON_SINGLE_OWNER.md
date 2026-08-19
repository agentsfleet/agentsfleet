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

# M166_002: One owner executes the live daemon integration suite per verification

**Prototype:** v2.0.0
**Milestone:** M166
**Workstream:** 002
**Date:** Aug 19, 2026
**Status:** IN_PROGRESS
**Priority:** P1 — every full local verification and every Pull Request (PR) pays for the live daemon integration suite twice, once instrumented and once bare.
**Categories:** INFRA
**Batch:** B1 — no other workstream owns the verification lanes.
**Branch:** perf/m166-live-daemon-single-owner
**Test Baseline:** unit=4124 integration=704
**Depends on:** M166_001 (parked — this workstream carries its audit finding and its recorded follow-up shape), M164_002 (the product-only coverage denominator, folder floors and required-component assertions this must preserve unchanged)
**Provenance:** agent-generated from the Make and workflow sources at `e0dcbb01d` and the M166_001 parking record, Aug 19, 2026
**Canonical architecture:** `docs/architecture/testing.md` §Coverage

---

## Overview

**Goal (testable):** The canonical verification sequence executes `agentsfleetd-integration-tests` exactly once against live datastores, enforces every coverage floor, required component and required root unchanged, and grades that floor only from evidence whose source, toolchain, component inventory and environment all match the run being graded.

**Problem:** A full verification runs the live daemon integration suite twice. `make test-unit-all` reaches `test-coverage-zig`, which runs `agentsfleetd-integration-tests` under kcov after its unit components; `make test-integration` then resets the datastores and runs the same suite again. Continuous Integration (CI) repeats the shape across two workflows: `test.yml`'s coverage job runs the instrumented copy and `test-integration.yml`'s job runs the bare copy, on two runners, each booting its own datastores. Commit and push pay nothing for this — the hooks run fast unit lanes only — so the whole cost lands on canonical local verification and on PR feedback.

**Solution summary:** Ownership splits at the lane boundary and nothing else moves. `test-coverage-zig` keeps its seven unit components and stops executing any live daemon binary. `test-integration` becomes the sole producer of live daemon integration coverage: it runs the suite once under kcov and runs the isolated boot-to-drain proof once under its build-time filter, so the two executions the union needs come from the one lane that already owns live datastores. Each producer records a manifest naming its components, their reports and the source, toolchain, inventory and environment it ran against. A new `make test-coverage-grade` validates both manifests and grades the same nine-component union against the same floors; `make test-integration` invokes that grade automatically when matched unit evidence is present, refuses loudly when the evidence is present but mismatched, and says so plainly when there is none. CI moves both producers and the grade into one workflow run so the artifacts they exchange are run-scoped, and one job — not two — runs the live daemon binary.

## PR Intent & comprehension handshake

- **PR title (eventual):** perf(test): give the live daemon suite a single execution owner
- **Intent (one sentence):** A developer running full verification, and a PR waiting on CI, stop paying for a second execution of the live daemon integration suite without losing a test, a covered line, a floor or a failure signal.
- **Handshake** — the implementing agent fills this at PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `make/test-unit.mk` — the `test-coverage-zig` recipe: the concurrent unit component fan-out, the per-component exit-status files, the serial `integration` block, the filtered `lifecycle` rebuild, the suite tally parsing and the run-marker assertion. The tally parsing and the marker assertion move to `test-integration` unchanged; the fan-out stays.
2. `make/test-integration.mk` — `test-integration`'s datastore reset, background runner build, migrate, and `zig build test-integration` invocation, plus the `TEST_FILTER` narrowing rules and the `TEST_INFRA=provided` escape hatch.
3. `scripts/check_zig_test_lanes_test.py` — the existing harness that drives the real Make recipes against a stubbed kcov. This is how ownership and evidence behaviour get proved without running real suites; the below-floor, missing-report and skipped-lifecycle assertions move to their new owners here.
4. `make/test.mk` — the one definition site for coverage floors, targets, required components, required roots and the lifecycle isolation strings. The component inventory split and the evidence paths belong here for the same reason.
5. `docs/architecture/testing.md` §Coverage — the canonical statement that `test-coverage-zig` runs nine components. It becomes untrue in this diff and changes with it.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `make/test.mk` | EDIT | One definition site for the unit and live component inventories, the evidence paths, and the `test-coverage-grade` owner. |
| `make/test-unit.mk` | EDIT | `test-coverage-zig` keeps the unit components, executes no live daemon binary, and records unit evidence instead of grading. |
| `make/test-integration.mk` | EDIT | `test-integration` runs the daemon suite once under kcov, runs the isolated boot-to-drain proof, records integration evidence, and grades on matched unit evidence. |
| `scripts/verification_evidence.py` | CREATE | Record and validate producer manifests, refuse unusable evidence naming the field, and reject unlike timing samples. |
| `scripts/verification_evidence_test.py` | CREATE | Failure-injecting tests for every record, validation and comparison rule. |
| `scripts/check_zig_test_lanes_test.py` | EDIT | Re-point the lane assertions at their new owners and add the single-owner assertions over the Make sources. |
| `scripts/check_ci_lane_config_test.py` | EDIT | Prove one workflow run holds both producers and the grade, and that exactly one job executes the live daemon binary. |
| `.github/workflows/test.yml` | EDIT | Drop the coverage job and its needs entry; the `test` context keeps the unit and package lanes. |
| `.github/workflows/test-integration.yml` | EDIT | Carry both coverage producers, the fail-closed grade, the unprivileged migration check and the required aggregate in one run. |
| `docs/architecture/testing.md` | EDIT | Record which lane owns which component, the evidence rule, and the grade command. |
| `docs/v2/active/M166_002_P1_INFRA_LIVE_DAEMON_SINGLE_OWNER.md` | EDIT | Mark Dimensions DONE and carry acceptance evidence during execution. |

## Applicable Rules

- **`~/Projects/dotfiles/docs/greptile-learnings/RULES.md`** — **UFS** (component inventory, evidence keys, manifest paths and provenance field names each get one definition site in `make/test.mk` or the recorder, never a second copy in a recipe or workflow), **NDC** (the superseded live-daemon blocks leave `test-coverage-zig` in the same diff that adds their replacement), **NLR** (the lane tests touched here are re-pointed, not left asserting the old owner), **ORP** (no workflow, Make target, architecture sentence or lane test may still name a removed owner), **MKP** (every Make pipeline preserves the first failing status — the producers and the grade all fail closed), **GRD** (the current Make recipes and workflow sources are the source of truth for what exists today, not this spec's summary of them).
- `~/Projects/dotfiles/dispatch/write_python.md` — the recorder and validator parse with the standard library, validate at the boundary, use context-managed file access and raise specific failures naming the offending field.
- `~/Projects/dotfiles/dispatch/write_shell.md` — every recipe and workflow shell expansion stays quoted, temporary state is cleaned up, and no pipeline swallows a failing status.
- `docs/architecture/testing.md` — architecture consult; the document and the lane ownership change together in this diff.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | no — no `*.zig` source changes; the build graph and every test root are untouched | N/A |
| PUB / Struct-Shape | no — no Zig public surface changes | N/A |
| File & Function Length (≤350/≤50/≤70) | yes — `make/test-unit.mk`, `make/test-integration.mk` and the new Python module all grow | Removing the live blocks shrinks `test-unit.mk`; the recorder splits record, validate and compare into separate functions before any cap is approached, and the evidence recipes live beside the lane that owns them rather than accumulating in one file. |
| UFS (repeated/semantic literals) | yes — component names, binary names, manifest paths, provenance field names | Inventories and paths are defined once in `make/test.mk` and passed as arguments; the recorder owns the field names and every consumer reads them from the manifest. |
| UI Substitution / DESIGN TOKEN | no — no user interface files | N/A |
| LOGGING / LIFECYCLE / ERROR REGISTRY / SCHEMA | no — no product runtime, allocator wiring, error registry entry or schema file changes | N/A |
| MILESTONE-ID | yes — non-spec files change | No milestone, Section or Dimension marker appears outside this spec. |

## Prior-Art / Reference Implementations

- **Reference:** the existing `test-coverage-zig` recipe in `make/test-unit.mk` — the live daemon blocks move to `test-integration` verbatim in behaviour: same kcov include and exclude patterns, same per-component exit-status file, same non-empty Cobertura assertion, same suite tally parsing that treats zero passes and any failure as fatal, same run-marker grep for the boot-to-drain proof. Nothing about how the suite is measured changes; only which lane runs it.
- **Reference:** `scripts/check_zig_test_lanes_test.py` — drives the real Make recipe with a stubbed `kcov` on `PATH` and a `TEST_INFRA=provided` datastore claim. Every new ownership and evidence assertion uses this same harness rather than inventing a second way to exercise a lane.
- **Reference:** `scripts/check_zig_coverage.py` — the union grader is called with the same arguments it takes today, over the same nine components. It is not modified; the change is only which lane produced each report and what is validated before it runs.
- **Reference:** `scripts/check_ci_lane_config_test.py` — source-level workflow assertions with the reason for each grant written beside it. The single-owner and grade-wiring guards follow that shape instead of a new graph model.

## Sections (implementation slices)

### §1 — One lane owns every live daemon execution

The unit coverage lane stops executing live daemon binaries and the integration lane takes them, so a full verification runs the suite once instead of twice. The two executions the union needs — the unfiltered suite and the isolated boot-to-drain proof, which the unfiltered suite skips — both come from the lane that already owns live datastores, a migrated database and the runner binary. **Implementation default:** move the existing blocks rather than rewrite them, because their tally parsing and marker assertions are the accumulated defence against measuring a suite that never ran.

- **Dimension 1.1** — the unit coverage lane executes no live daemon binary and its recipe names none → Test `test_unit_coverage_lane_runs_no_live_daemon_binary`
- **Dimension 1.2** — the integration lane executes the unfiltered suite once and the filtered boot-to-drain proof once, and still fails on zero passes, on any failure, and on a skipped proof → Test `test_integration_lane_owns_every_live_daemon_execution`
- **Dimension 1.3** — the components produced across both lanes equal the registered inventory with no omission and no duplicate → Test `test_component_union_matches_inventory_exactly`

### §2 — Evidence is provenance-matched or it is refused

A grade that reads whatever reports happen to be on disk is a grade of an unknown build. Each producer records what it measured and what it measured against; each consumer refuses anything that does not match the run it is grading. **Implementation default:** compare a digest over the working-tree sources that reach the binaries, the toolchain identity, the component inventory digest and the platform, because those are exactly the four things that change which lines exist and which components are required.

- **Dimension 2.1** — a manifest recorded and then validated against the same source, toolchain, inventory and platform is accepted → Test `test_matched_evidence_validates`
- **Dimension 2.2** — changing any single provenance field is refused, and the failure names that field and both values → Test `test_mismatched_provenance_field_is_named`
- **Dimension 2.3** — evidence that is missing, failed, empty, tampered with on disk, or recorded from a narrowed run is refused, each naming its reason → Test `test_unusable_evidence_fails_the_aggregate`

### §3 — Grading has one owner and one command

The merged floor moves out of the unit lane, which can no longer see the union, into a command that owns nothing but the grade. The canonical two-command sequence still grades automatically, so the developer-facing behaviour of `make test-unit-all && make test-integration` is unchanged in what it proves. **Implementation default:** the grade is invoked by the integration lane when matched unit evidence exists and refuses when evidence exists but does not match; an absent manifest is reported and does not fail the integration lane, because producing unit evidence was never that lane's job.

- **Dimension 3.1** — the grade enforces every floor, target, required component, required root, minimum file count and minimum measured-line count unchanged over the nine-component union, and writes the published summary file → Test `test_grade_preserves_every_coverage_assertion`
- **Dimension 3.2** — the integration lane grades on matched evidence, exits non-zero on mismatched evidence naming the field, and on absent evidence reports the ungraded floor and the command that grades it without failing → Test `test_integration_grade_reuse_rules`
- **Dimension 3.3** — running the canonical sequence executes the unit component binaries once, in the unit lane only, and the integration lane rebuilds and reruns none of them → Test `test_sequence_reuses_unit_evidence_without_rerun`

### §4 — Continuous Integration carries the whole graph in one run

Artifact storage is run-scoped, so the two producers and the grade have to share a workflow run or the grade cannot see what it grades. Both required check contexts stay substantive: `test` keeps the unit and package lanes, `test-integration` gains the whole Zig coverage graph and cannot go green while the grade is red. **Implementation default:** the producers run concurrently and the grade is a third job that needs both, so the critical path becomes the longer producer plus a report union rather than a producer plus a full suite.

- **Dimension 4.1** — exactly one job in the repository executes the live daemon integration binary, and both coverage producers plus the grade live in one workflow → Test `test_ci_runs_one_live_daemon_owner`
- **Dimension 4.2** — the grade job needs both producers, validates their manifests before grading, and no workflow step grades from unvalidated or absent artifacts → Test `test_ci_grade_is_fail_closed`
- **Dimension 4.3** — timing samples are refused for comparison unless they share commit, runner image and cache state, so a median improvement is computed only across like samples → Test `test_unlike_timing_samples_are_refused`

### §5 — The architecture page states the ownership it enforces

`docs/architecture/testing.md` currently says the coverage lane runs nine components. After this diff it runs seven and the integration lane runs two. A page that describes the previous ownership is worse than no page, because the next person reads it before the recipes.

- **Dimension 5.1** — the architecture page names each lane's components, the evidence rule and the grade command, and the lane sources agree with it → Test `test_testing_architecture_matches_lane_ownership`

## Interfaces

```
Canonical commands:

  make test-unit-all         unit lanes + unit coverage components + package coverage.
                             Executes no live daemon binary. Records unit evidence.
                             Does not grade the merged Zig floor.

  make test-integration      the single live daemon execution, under kcov, plus the
                             isolated boot-to-drain proof. Records integration evidence.
                             Grades the merged floor when matched unit evidence exists.

  make test-coverage-grade   validates both manifests, then grades the nine-component
                             union. Fail-closed on every unusable-evidence reason.

  make test-integration-db | test-integration-redis | test-integration-kernel
                             unchanged narrow selectors. Uninstrumented, record no
                             evidence, grade nothing.

Evidence manifests — .tmp/verification/{unit,integration}.json

  producer          the make target that wrote it
  source_digest     digest over the tracked and untracked-but-not-ignored sources
                    that reach the measured binaries
  toolchain         the Zig toolchain identity the binaries were built with
  graph_digest      digest over the component inventory, required components,
                    required roots and floors in force
  environment       the platform identity that decides which components are required
  filtered          true when a narrowing selector was in force; refused by the grade
  components[]      name, report path, measured line count, report digest
  outcome           passed, or the reason it is not usable

Timing samples — one JSON object per run: commit, runner image, cache state,
critical path duration. Samples that disagree on the first three are refused
before any median is computed.

.tmp/zig-coverage.txt   every existing key unchanged; written only by the grade.

Exit status is the caller interface: zero only when every owned component
executed, every manifest validated, and every coverage assertion passed.
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Duplicate live execution | A recipe or workflow job other than the integration owner names the daemon integration binary | Lane and workflow source assertions fail, naming the offending file and the second owner, before any suite runs |
| Missing component | A component in the inventory produced no report, or a manifest omits one | The grade exits non-zero naming the absent component; no rate is computed |
| Empty report | A component collected zero measured lines | The grade exits non-zero naming that component rather than shrinking the denominator |
| Suite never ran | The daemon integration binary exits zero with zero passing tests | The integration lane exits non-zero on the tally, as it does today, and records no usable evidence |
| Failing suite reported green | The daemon integration binary exits zero with failing tests | The integration lane exits non-zero on the tally and prints the failing test names |
| Skipped boot-to-drain proof | The isolation variable or datastores are absent, so the proof self-skips and still yields a valid report | The run-marker grep fails the lane, as it does today, rather than grading the daemon boot sequence as genuinely uncovered |
| Stale evidence | A manifest was recorded against a different source, toolchain, inventory or platform | The consumer exits non-zero naming the mismatched field and both values |
| Tampered evidence | A recorded report changed on disk after the manifest was written | The digest comparison fails naming that report |
| Narrowed evidence | A manifest was recorded from a run under a test filter | The grade refuses it naming the filter, because a narrowed run cannot support a floor |
| Absent unit evidence | `make test-integration` runs without a prior unit lane | The integration verdict stands, the ungraded floor is reported by name with the command that grades it, and the lane exits zero |
| Unvalidated CI artifact | A workflow grades from an artifact it never validated, or from a producer it does not need | The workflow source assertions fail naming the job |
| Unlike timing samples | A comparison mixes commits, runner images or cache states | The comparison exits non-zero naming the field that disagrees, before any median is reported |

## Invariants

1. Exactly one Make recipe executes the daemon integration binary — enforced by a lane test that greps every Make source for the binary name and fails on a second owner.
2. The published coverage summary file is written by the grade and by nothing else — enforced by a lane test asserting no other recipe writes that path.
3. Evidence is consumed only when every provenance field matches the run being graded — enforced by the validator, which the grade calls before the union grader and which exits non-zero naming the first mismatch.
4. Every component in the inventory appears exactly once across the two manifests — enforced by the grade's union check, which fails on both omission and duplication before any rate is computed.
5. A narrowed or partial run cannot supply evidence for a floor — enforced by the recorder, which marks the manifest filtered, and by the validator, which refuses a filtered manifest.
6. Both required check contexts stay substantive — enforced by workflow source assertions that each named context transitively needs at least one job executing tests.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| not applicable — no product or operator signal changes | not applicable | N/A | N/A | N/A | N/A |

This workstream changes repository verification lanes only. No product analytics event, funnel timer, operator dashboard or feature-flag exposure is added, renamed or removed, so no analytics or funnel playbook update is required. The evidence manifests are local build artifacts under `.tmp/`, not telemetry, and carry no environment values, connection strings or credentials — the recorder writes digests and counts, never the values it digested.

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit | `test_unit_coverage_lane_runs_no_live_daemon_binary` | the unit coverage recipe driven against a stubbed kcov that records every binary it is handed → the daemon integration binary appears in no invocation, and no Make source outside the integration lane names it |
| 1.2 | unit | `test_integration_lane_owns_every_live_daemon_execution` | the integration recipe against a stubbed kcov → one unfiltered invocation and one filtered invocation of the daemon integration binary; a stub reporting zero passes, a stub reporting a failure, and a stub omitting the run marker each fail the lane with its own message |
| 1.3 | unit | `test_component_union_matches_inventory_exactly` | manifests missing one component, and manifests naming one component twice → each refused naming that component; the complete pair accepted |
| 2.1 | unit | `test_matched_evidence_validates` | a manifest recorded and validated with source, toolchain, inventory and platform unchanged → accepted, with every recorded component resolved |
| 2.2 | unit | `test_mismatched_provenance_field_is_named` | four manifests, each with exactly one of source, toolchain, inventory, platform altered → four distinct non-zero failures, each naming the field and both values |
| 2.3 | unit | `test_unusable_evidence_fails_the_aggregate` | manifests that are absent, carry a failed outcome, carry a zero-line component, point at a report whose digest no longer matches, and carry the filtered marker → each refused with its own reason; none produces a rate |
| 3.1 | unit | `test_grade_preserves_every_coverage_assertion` | a stubbed nine-component union below the merged floor, one below a folder floor, one omitting a required component, one omitting a required root, one under the file minimum, one under the measured-line minimum → each fails with the assertion's existing message; the passing union writes every existing summary key |
| 3.2 | unit | `test_integration_grade_reuse_rules` | the integration lane run with matched unit evidence, with mismatched unit evidence, and with none → grades; exits non-zero naming the field; exits zero having named the ungraded floor and the grade command |
| 3.3 | integration | `test_sequence_reuses_unit_evidence_without_rerun` | the canonical two-command sequence against a stubbed kcov that logs every binary → each unit component binary appears exactly once across the whole sequence, and the integration lane invokes none of them |
| 4.1 | unit | `test_ci_runs_one_live_daemon_owner` | every workflow source → exactly one job step executes the live daemon owner, and both coverage producers plus the grade resolve to one workflow file |
| 4.2 | unit | `test_ci_grade_is_fail_closed` | the workflow sources, plus fixtures that drop the grade job's needs entry, drop its validation step, and let it continue on a missing artifact → the real sources pass and each fixture fails naming the job |
| 4.3 | unit | `test_unlike_timing_samples_are_refused` | sample sets differing in commit, in runner image, and in cache state → each refused naming the field; a like set of three cold and three warm samples yields medians and a computed change |
| 5.1 | unit | `test_testing_architecture_matches_lane_ownership` | the architecture page and the Make sources → each lane's component list, the evidence rule and the grade command agree; a page still claiming the previous ownership fails |

Regression rows: the Zig folder floors, merged floor, required components, required roots, minimum file and measured-line counts, and every published summary key stay byte-identical in meaning and are graded from the same union; `make memleak`, `make test-integration-kernel`, both Linux cross-compiles and the package coverage gates are untouched and stay green; the unit and integration test counts in `make _lint_zig_test_depth` do not fall.

Replay rows: running the canonical sequence twice on unchanged sources produces manifests whose provenance fields are identical and a grade with the same verdict; only durations and report timestamps differ. Running the grade twice against one pair of manifests is idempotent and reads the same union.

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | One Make recipe owns the live daemon binary (§1) | `grep -rn 'agentsfleetd-integration-tests' make/ \| grep -v '^make/test-integration.mk'` | no output | P0 | |
| R2 | The full component union grades green with every existing assertion (§1, §3) | `make test-unit-all && make test-integration` | exit 0; `.tmp/zig-coverage.txt` carries every pre-existing key and the folder floors pass | P0 | |
| R3 | The canonical sequence executes the daemon integration binary once (§1, §3) | `make test-unit-all && make test-integration` with the lane-test kcov stub logging every binary | exactly 1 unfiltered and 1 filtered invocation of `agentsfleetd-integration-tests` | P0 | |
| R4 | Evidence rules and lane ownership hold under injection (§1–§3, §5) | `python3 -m unittest discover -s scripts -t scripts -p '*_test.py'` | exit 0 | P0 | |
| R5 | CI runs one live daemon owner and a fail-closed grade in one run (§4) | `make check-gh-actions-valid && python3 -m unittest scripts.check_ci_lane_config_test` | exit 0 | P0 | |
| R6 | Measured CI critical path improves against the same-commit, same-image baseline (§4) | `python3 scripts/verification_evidence.py compare-timings --samples .tmp/verification/ci-samples.json` | exit 0; medians reported for cold and warm separately with a positive change | P0 | |
| R7 | Diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 paths missing from the Files Changed table | P0 | |
| S1 | Repository verification stays green | `make harness-verify && make lint-all && make check-version` | exit 0 | P0 | |
| S2 | No leaks | `make memleak` | exit 0 | P0 | |
| S3 | No secrets | `gitleaks detect --no-banner` | exit 0 | P0 | |
| S4 | No oversize source file | `git diff --name-only origin/main...HEAD \| grep -v '\.md$' \| xargs wc -l 2>/dev/null \| awk '$1>350 && $2!="total"'` | no output | P0 | |
| S5 | Orphan sweep | Dead Code Sweep greps | 0 matches | P0 | |

**Grading protocol (VERIFY):** run the Verify command verbatim; grade ONLY from its output. Graded = ✅/❌ + the one decisive output line (`342 passed`); long evidence goes to PR Session Notes with a pointer here. **Ship gate:** every row graded, every P0 ✅ → eligible for CHORE(close); any ❌ or empty cell → return to EXECUTE; a P1 ❌ ships only with an Indy-acked deferral quote in Discovery.

## Dead Code Sweep

**1. Orphaned files — deleted from disk and git.**

N/A — no files deleted.

**2. Orphaned references — zero remaining imports/uses.**

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| the live daemon components in the unit lane | `grep -n 'agentsfleetd-integration-tests' make/test-unit.mk` | 0 matches |
| the removed coverage job in `test.yml` | `grep -n 'test-coverage-zig' .github/workflows/test.yml` | 0 matches |
| the merged grade inside the unit lane | `grep -n 'check_zig_coverage.py' make/test-unit.mk` | 0 matches |
| the nine-component claim for the unit lane | `grep -n 'nine component binaries' docs/architecture/testing.md` | 0 matches |

## Out of Scope

- Sharding the daemon integration suite, worker-clock overlap claims, and any parallel execution of the live root. M166_001 attempted that and its parking record names the coverage and measurement failures; daemon execution stays serial here.
- Lowering any coverage floor, target, required component, required root, or denominator minimum. A regression in any of them stops this workstream rather than being absorbed.
- Replacing kcov, changing the pinned CI image, or altering the kcov include and exclude patterns.
- Product runtime code, Application Programming Interface (API) behaviour, schema migrations and user interface changes.
- Speeding up the memory-leak lane, the runner kernel lane, the browser acceptance lanes and the deployment workflows.

---

## Product Clarity (authoring record)

1. **Successful user moment** — an engineer runs `make test-unit-all && make test-integration`, watches the live daemon suite scroll past exactly once, and gets the same coverage verdict at the end that they get today.
2. **Preserved user behaviour** — the canonical command names, their exit-status meaning, the failure diagnostics that name failing tests, every coverage floor and key, the narrow integration selectors, `TEST_FILTER` narrowing, `KEEP_TEST_STATE`, `TEST_INFRA=provided`, and both required CI check names all keep working unchanged.
3. **Optimal-way check** — moving the live blocks to the lane that already owns live datastores is the most direct shape; the unconstrained optimum would also cut the instrumented suite's own duration, which needs either sharding or a different instrument. Both are refused here, the first because M166_001 measured it failing and the second because it discards the proven denominator.
4. **Rebuild-vs-iterate** — iterate. The duplication is one misplaced pair of blocks and one missing grade owner, not a broken design. M166_001 chose rebuild, lost the coverage floor and could not prove its timing claim; determinism is exactly what its sharding traded away.
5. **What we build** — the lane ownership move, an evidence recorder and validator, one grade command, the workflow consolidation, and the architecture page correction.
6. **What we do NOT build** — shards, a verification graph model, worker timing instrumentation, a coverage dashboard, or any new test framework. None of them is needed to stop running the suite twice.
7. **Fit with existing features** — compounds with M164_002's product-only denominator and folder floors, which it must preserve exactly; the feature it must not destabilise is the daemon integration suite's own reliability, which is why its execution moves without its measurement changing.
8. **Surface order** — N/A — no user surface. The command-line surface is the existing Make targets, one of which is added.
9. **Dashboard restraint** — N/A — no user interface. Make output, the manifests under `.tmp/` and the CI job summary carry everything a failure needs.
10. **Confused-user next step** — every refusal names the lane, the component or the provenance field that disagreed and the command that produces what is missing; `docs/architecture/testing.md` §Coverage carries the ownership table.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** ownership first, evidence second, grade owner third, CI consolidation fourth, documentation last. Each Section is independently revertible, and the coverage floor is provable green before the workflows move.
- **Alternatives considered:** M166_001's sharded graph — rejected, it is parked with a measured coverage failure and no proven timing result. Deleting the bare `make test-integration` run and keeping the instrumented copy in the unit lane — rejected, it would leave the unit lane depending on live datastores and would make the fast integration selectors dead. Passing coverage artifacts between two workflows — rejected, artifact storage is run-scoped and cross-workflow retrieval would need commit polling with cancellation and rerun races that have no local equivalent.
- **Patch-vs-refactor verdict:** this is a **patch** because the measurement machinery, the build graph, every test root and the coverage grader are all correct and stay untouched; what changes is which lane invokes which binary and when the union is graded.

## Discovery (consult log)

- **Consults** — Architecture / Legacy-Design / gate-flag triage: none at creation.
- **Metrics review** — no product or operator signal changes; no analytics or funnel playbook update required, because no user journey changes.
- **Skill-chain outcomes** — `/write-unit-test`, `/write-integration-test`, `/review` and `kishore-babysit-prs`: pending execution.
- **Deferrals** — none.
