<!--
SPEC AUTHORING RULES (load-bearing — the one comment that survives):
- Body order = the executing agent's read order. Fill via the orly-spec-new
  skill (authoring order lives there); after filling, DELETE every "tpl:"
  guidance comment — the SPEC TEMPLATE GATE blocks tpl residue, unfilled
  {slots}, and missing required sections (.orly/audits/spec-template.sh --staged).
- No time/effort/hour/day estimates anywhere. No effort columns, complexity
  ratings, percentage-complete, implementation dates, assigned owners.
- Priority (P0/P1/P2/P3) is the only sizing signal; Dependencies are the only
  sequencing signal. A section that contradicts these rules loses — delete it.
-->

# M216_001: Complete the Jev measurement and remove its pilot directory

**Prototype:** v2.0.0
**Milestone:** M216
**Workstream:** 001
**Date:** Oct 08, 2026
**Status:** DONE
**Priority:** P2 — bounded contributor experiment
**Categories:** Documentation (DOCS), Infrastructure (INFRA)
**Batch:** B1 — independent pilot artifacts
**Branch:** feat/m216-jev-assertion-pilot
**Baseline revision:** dd917b7ef42dcb883b5192fafe894060aab5845d
**Test Baseline:** unit=10613 integration=899 — unit comparison retains one failure
**Baseline evidence:** https://github.com/agentsfleet/agentsfleet/blob/8704a47c9c6ab020b93944d87948eef9ad34b735/pilots/jev-assertions/report.md
**Depends on:** recorded comparison baselines and green declared repository checks; owner review before merge
**Provenance:** agent-generated from Indy's Oct 08, 2026 pilot instruction
**Canonical architecture:** existing product architecture remains unchanged; this is an isolated contributor measurement

## Overview

**Goal (testable):** Retain the completed Jev measurement in Git history, remove the entire pilot directory, and verify the orly 0.14.0 consumer pin.
**Problem:** The completed contributor experiment leaves a runner, copied fixtures and raw receipts that Indy requires removed from the repository tree.
**Solution summary:** Delete `pilots/` without relocating its contents. Keep the completed measurement at immutable revision `8704a47c9c6ab020b93944d87948eef9ad34b735`, update current checking instructions and retain the engine pin. Model advice remains experimental; fresh repository and hosted checks cover the resulting branch before owner review.

## PR Intent & comprehension handshake

- **PR title (eventual):** Upgrade orly to 0.14.0 and retire the Jev pilot
- **Intent:** Ship the verified engine pin with the completed experiment removed and its historical evidence still reviewable.
- **Handshake:** ASSUMPTIONS I'M MAKING: the removal covers all of `pilots/`; Git history retains the evidence; this Pull Request remains open for Indy.

## Implementing agent — read these first

1. `.orly/LOCAL.md` and `.orly/dispatch/lifecycle.md` — project commands and lifecycle timing.
2. `.orly/orly.json` and `scripts/check_orly_pin.sh` — preserve the installed version and declared checks.
3. https://github.com/agentsfleet/agentsfleet/tree/8704a47c9c6ab020b93944d87948eef9ad34b735/pilots/jev-assertions — immutable completed experiment, not a current command surface.

## Files Changed (blast radius)

All current paths below are relative to this repository. Indy authorized the engine update and then removal of the completed pilot.
No production, catalog, threshold, hook or gate edits are authorized.

| File | Action | Why |
|------|--------|-----|
| `docs/v2/pending/M216_001_P2_DOCS_INFRA_JEV_ASSERTION_REVIEW_PILOT.md` → `docs/v2/active/M216_001_P2_DOCS_INFRA_JEV_ASSERTION_REVIEW_PILOT.md` → eventual same basename in `done/` | CREATE / MOVE / EDIT | Lifecycle and measured evidence |
| `.orly/orly.json` | EDIT | Update the managed engine pin to published 0.14.0 through `orly update --no-hooks`. |
| `pilots/` | DELETE | Remove all pilot-only files; retain historical evidence at immutable commit `8704a47c9c6ab020b93944d87948eef9ad34b735`. |

Temporary proof copies and dependency installation outputs are disposable local runtime state, never committed. Existing `orly` replay files remain local private cache, not model inputs or deliverables.

## Applicable Rules

- `.orly/docs/greptile-learnings/RULES.md`: No Dead Code (NDC), standard parsers (PSR), a test must fail when its subject is incorrect (TCF), Unified Form for Symbols (UFS), Prompt-injection Resistance from user Input (PRI), test naming (TNM) and grounding in source (GRD).
- `.orly/dispatch/write_spec.md` and `.orly/docs/TEMPLATE.md`: preparation precedes measurement and human approval remains a named dependency.
- `.orly/dispatch/write_any.md`, `.orly/dispatch/write_ts_adhere_bun.md`, `.orly/dispatch/write_python.md`: bounded files, copied evidence provenance and standard-library orchestration.
- `.orly/dispatch/verify.md`: focused proofs establish Sections; declared repository checks remain due before the Pull Request (PR).

## Applicable Gates

Source-specific rows describe the historical pilot. The final branch contains only the spec and engine pin; its declared checks still apply.

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| Spec template and document reads | Yes | Local template; per-document read log; staged structural checks |
| File and function length | Yes | Small concern modules; source files ≤350 lines and functions ≤50 lines |
| UFS, logging, milestone naming | Source-dependent | Named pilot constants; no credentials or milestone identifiers in executable symbols; bounded structured receipts |
| User interface and design tokens | No new product interface | Copied existing components and helpers remain fixture evidence |
| Schema, auth, error registry | No | No product schema, authentication or fallible Rust signatures change |
| Secret scan and conform | Yes | Existing scanner and `make harness-verify`; no suppressions or gate edits |

## Prior-Art / Reference Implementations

- `ui/packages/design-system/vitest.config.ts`, `src/test-setup.ts` and the four source tests: use the incumbent Vitest and React Testing Library stack for faithful copies.
- `scripts/check_documentation_rules_test.py`: standard-library Python unittest style for pilot admission and accounting checks.
- Read-only `/Users/kishore/Projects/orly/docs/JUDGMENTS.md:110`: existing command and five classes. Installed `orly` 0.13.0 source verifies one request per item, no retries and native answer retention; no earlier evaluation fixtures are used.
- Supabase's local `oss/js/supabase/packages/dev-tools/vitest.config.ts`: isolated jsdom setup; use the discovered local reference location without cloning.

## Sections (implementation slices)

Sections 1–3 and their named proofs describe completed historical work at the immutable revision linked above.
Their relative evidence paths resolve inside that revision's pilot directory, including the historical Test Specification and Discovery records.
They are not commands or files required in the current checkout. Sections 4–5 define the final tree.

### §1 — Frozen offline evidence

**Status:** DONE — executable offline preparation; evidence in `pilots/jev-assertions/receipts/checks.json`.

Record requirements, source revision, selectors, expected labels and justifications before advice. Use four cases per existing class and preserve family correlation in the report. Independent expected values stay outside uploads.

- **Dimension 1.1** — DONE — Balanced neutral cases and manifests have complete selected evidence except deliberate missing assertion helpers → Test `test_frozen_cases`.
- **Dimension 1.2** — DONE — Correct and faulted copies demonstrate what each assertion detects; faults remain isolated → Test `test_fault_discrimination`.
- **Dimension 1.3** — DONE — Byte changes, answer leakage or evidence outside fixtures refuse execution → Test `test_freeze_refusal`.

### §2 — Pre-advice review and approved requests

**Status:** DONE — pre-advice inspection frozen, actual approval recorded and both one-shot batches retained.

Freeze the author's unaided inspection before any Jev output. Specify paired sessions with blinded identifiers, equal evidence, independent reviewers and recorded elapsed review time. Author-created expectations cannot establish an independent comparison. Missing sessions are reported unmeasured.

- **Dimension 2.1** — DONE — Unaided inspection and session protocol predate advice → Test `test_review_freeze`; frozen `reviews/unaided-author.json` remains unchanged.
- **Dimension 2.2** — DONE — Actual approval binds frozen upload, runner, model, price and total budget → Test `test_live_approval`; `approval.json` records Indy's actual quote.
- **Dimension 2.3** — DONE — Both ten-request reservations precede launch and consume the twenty-request allowance → Test `test_request_reservations`; `receipts/reservations.json` and both live receipts reconcile.

### §3 — Measurements and recommendation

**Status:** DONE — measurements, recommendation and timeout-diagnostics retention pass their scoped proofs. Fresh checks must cover the final pushed repair.

After owner approval, invoke `orly judge verify --input <manifest> --refresh --json` once per batch. Retain failures even if no usage is returned. Replay with the same command without refresh. Do not manufacture replies to make offline preparation green.

- **Dimension 3.1** — DONE — Native classes, withholding, twenty attempts, timing, tokens and calculated cost retained → Test `test_attempt_accounting`; `receipts/summary.json`.
- **Dimension 3.2** — DONE — Findings, misses and false alarms separated by origin; original attempts and actual agent times retained → Test `test_comparison_metrics`; `report.md` and `receipts/summary.json`.
- **Dimension 3.3** — DONE — Two fresh blinded agent sessions, sealed initial decisions and recommendation recorded; human time remains unmeasured → Test `test_measurement_completion` (manual); both review records and `receipts/review/blinded/`.
- **Dimension 3.4** — DONE — Timeout cleanup retains both command output streams in the failed-attempt receipt, including forced killing; spent reservations cannot launch again → Test `test_live_failure_consumes_reservation_without_retry`; `receipts/review/timeout/verification.json`.

### §4 — Authorized engine update

**Status:** DONE — published 0.14.0 installed, managed pin verified and declared checks green; the consumer Pull Request awaits Indy's review.

- **Dimension 4.1** — DONE — The installed engine and managed pin equal 0.14.0; existing configuration and hooks survive the update → Test `test_orly_014_pin` (manual).

### §5 — Remove the completed pilot directory

**Status:** DONE — the complete pilot directory is removed; no replacement runner or fixture location.

- **Dimension 5.1** — DONE — Remove the complete pilot directory, preserve the engine pin and point historical evidence to its immutable revision → Test `test_pilot_directory_removed` (manual).

## Interfaces

No pilot runner is exposed in the current checkout. Historical check, replay, live and summarize interfaces remain in the immutable revision.
The installed `orly` commands and managed configuration keep their existing interfaces; only the engine pin changes to 0.14.0.

## Failure Modes

The first five rows describe the historical measurement; the last row covers current directory removal.

| Mode | Cause | Handling (system response + caller observation) |
|------|-------|------------------------------------------------|
| Changed or leaking evidence | Digest mismatch, labels in upload or wrong path | Refuse before command; `test_freeze_refusal` |
| Misleading proof | Syntax/import failure mistaken for discriminating assertion | Require correct-copy success and matching assertion failures; `test_fault_discrimination` |
| No approval / quota exhausted / duplicate launch | Missing bound approval or spent reservation | Refuse before upload; `test_live_approval`, `test_request_reservations` |
| Process crash or unavailable provider | Timeout, invalid reply, interrupted batch or missing usage | Keep reservation, failure and timeout output; no retry; unknown usage stays unknown; `test_attempt_accounting`, `test_live_failure_consumes_reservation_without_retry` |
| Reviewer contamination / missing session | Author knows labels, reused session, advice disclosed early | Paired comparison remains unmeasured; `test_comparison_metrics` |
| Incomplete removal or dangling caller | Retained pilot file, moved copy or active reference | Fail `test_pilot_directory_removed`; remove the file or correct the caller before pushing |

## Invariants

Items 1–4 and 6 describe the completed measurement at its immutable revision. Its approval permits no additional provider requests.

1. Frozen files and uploads are immutable after registration: hashes are checked before proofs, replay and live admission.
2. Model inputs omit expected classes, justifications and review decisions: admission compares exact allowed manifest fields and fixture-only references.
3. Total provider requests ≤20, including failed attempts: exclusive creation and locked append-only batch reservations; each batch is admitted once.
4. Proposed ceiling is United States dollars (USD) 0.06 at the documented USD 0.042 per million input tokens, output free: reserve 65,536 input tokens per admitted request, giving a conservative twenty-request bound of USD 0.05505024. Approval must confirm pricing; unsupported pricing refuses live admission.
5. Final-tree preservation: the branch differs from its comparison only in this spec and the authorized engine pin; no pilot files remain.
6. Native answers, uncertainty and availability remain distinct: summarize original receipt fields and retain unmeasured usage without zero substitution.

## Metrics & Observability

No product or operator analytics change. The historical receipts retain native output, timing, usage, spending and original approval at the immutable revision.
Removal makes no provider request and adds no telemetry. Proof: `test_pilot_directory_removed`; historical `test_attempt_accounting` and `test_live_approval`.

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit | `test_frozen_cases` | Twenty cases → four of each class, ten in each valid manifest; insufficient evidence identifies the omitted helper |
| 1.2 | integration | `test_fault_discrimination` | Forty fixture executions → correct implementations all pass; precise assertions reject the deliberate fault; weaker tests survive |
| 1.3 | unit | `test_freeze_refusal` | Changed bytes, leakage and path escape → refusal before any request |
| 2.1 | unit | `test_review_freeze` | Frozen author review → predates live receipt and explicitly disclaims independence |
| 2.2 | unit | `test_live_approval` | Missing approval, wrong freeze/model/price → no launch |
| 2.3 | unit | `test_request_reservations` | Duplicate/concurrent/third batch → refusal; crash reservation survives |
| 3.1 | unit | `test_attempt_accounting` | Failed or withheld native answer → retained attempt, unknown usage preserved |
| 3.2 | unit | `test_comparison_metrics` | Known seeded answers and absent independent reviews → truthful counters and unmeasured paired gain |
| 3.3 | manual | `test_measurement_completion` | Two sealed independent agent records → complete paired counts and actual monotonic timing; no human or causal time-saving claim |
| 3.4 | unit | `test_live_failure_consumes_reservation_without_retry` | Real child prints to both streams before timeout → exact persisted failed receipt and spent reservation; no second launch. `test_subprocess_timeout_stops_child` additionally proves graceful and forced cleanup retain both streams |
| 4.1 | manual | `test_orly_014_pin` | Published 0.14.0 install, `orly doctor` and `bash scripts/check_orly_pin.sh` → matching engine/pin, current managed files and preserved commands, surfaces and hooks |
| 5.1 | manual | `test_pilot_directory_removed` | `test ! -e pilots`, empty staged pilot inventory and repository-wide reference search → no pilot files or active callers; historical references name immutable commit `8704a47c9c6ab020b93944d87948eef9ad34b735` |

Regression scope: product behavior, source tests, catalog, confidence threshold, hooks and gates receive no diff. At least half of runner checks exercise refusal, failed attempts or contamination. No performance or concurrency claim is made about product code.

## Acceptance Rubric (single scoring surface)

| # | Criterion | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|-----------|---------------------|----------|----------|-----------------|
| R1 | Complete pilot removal | `test ! -e pilots` and `git ls-files --cached pilots` | exit 0; zero tracked pilot files | P0 | ✅ Directory absent; zero tracked pilot files; 220 files remain in historical revision |
| R2 | Scope and lifecycle | `git diff --name-only origin/main` | Only the spec and managed pin; completed spec in done before the final push | P0 | ✅ Net scope is the managed pin and this completed spec |
| R3 | Consumer pin | `orly doctor` and `bash scripts/check_orly_pin.sh` | exit 0; installed engine and pin equal 0.14.0 | P0 | ✅ Both exit 0; installed 0.14.0 matches the pin |
| S1 | Conform | `make harness-verify` | exit 0 | P0 | ✅ ALL GATES GREEN; no source files in final diff scope |
| S2 | Unit boundary | `make test-unit-all` | exit 0 at Pull Request boundary | P0 | Fresh final gate due; earlier 10614 passed is historical |
| S3 | Lint boundary | `make lint-all` | exit 0 at Pull Request boundary | P0 | Fresh final gate due |
| S4 | Integration boundary | `make test-integration-rustd` | exit 0 when applicable; engine may skip when the final branch has no code | P0 | Fresh gate determines applicability; earlier 899 passed is historical |
| S5 | Version preservation | `make check-version` | exit 0 | P0 | Fresh final gate due |
| S6 | No secrets | `gitleaks protect --staged --redact --no-banner` | exit 0 | P0 | Fresh scan due |

## Dead Code Sweep

| File to delete | Verify | Expected |
|---|---|---|
| `pilots/` | `test ! -e pilots`; `git ls-files --cached pilots` | exit 0; zero tracked paths |

| Deleted symbol/import | Grep | Expected |
|---|---|---|
| Pilot directory and its unique runner modules | `git grep -n -w -e jev-assertions -e pilot_checks.py -e pilot_measure.py -e pilot_test.py` | Only this spec's removal description and explicitly historical references |

No production symbol is removed. No copied test, helper, receipt, alternate runner directory or orphan configuration is retained.

## Out of Scope

- Runtime adoption, new questions, calibration changes, production fixes and gate integration.
- Earlier `orly` evaluation fixtures, tuning after freezing, retrying failed requests and extra planning-model calls.
- Claiming independent reviewer improvement from the author's labels or from classification agreement alone.
- Publishing, merging, a product release, docs-repository edits or another agent's worktree.

## Product Clarity (authoring record)

The original measurement rationale follows. Sections 4–5 and the current Acceptance Rubric define the final tree and its checks.

1. **Successful user moment:** A reviewer sees a weak test pass an incorrect copied value, then can assess Jev's actual usefulness with traceable evidence.
2. **Preserved user behaviour:** Every product caller and existing verification command retains its behavior.
3. **Optimal-way check:** Existing advice and source tests supply the smallest relevant measurement; independent reviewer availability limits the paired result.
4. **Rebuild-vs-iterate:** Iterate with isolated artifacts; a general evaluation service adds upkeep without improving this bounded sample.
5. **What we build:** Frozen cases, two manifests, executable counterexamples, bounded orchestration and one results report.
6. **What we do NOT build:** A new reviewer gate or autonomous adoption policy.
7. **Fit with existing features:** Measures the existing assertion question while preserving its advisory authority.
8. **Surface order:** N/A — internal pilot; command-line checking only.
9. **Dashboard restraint:** N/A — no product interface or quality badge.
10. **Confused-user next step:** Inspect the archived report; run the current pin checks and repository Make commands from the Acceptance Rubric.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** Retain the completed measurement in Git history, remove its entire directory and ship the verified engine pin.
- **Alternatives considered:** Direct custom provider integration duplicates the pinned command and its scanner; manual batch execution cannot enforce a cross-run request ceiling.
- **Patch-vs-refactor verdict:** Directory removal is sufficient. Quality ceiling: an alternate runner or evaluator adds upkeep; no product refactor is justified.
- **Surface-area checklist:** OpenAPI no; product command-line interface no; user docs no; engine version yes, product version no; schema no; spec amended for removal. The consumer pin accompanies pilot retirement.

## Discovery (consult log)

- **Removal instruction:** Indy: "Well the pilot directory must be removed." Remove `pilots/` from the same branch and Pull Request. Keep the completed measurement evidence in Git history, retain the 0.14.0 pin and revise current verification commands so they require no removed runner. Quality ceiling: deletion removes the upkeep; relocating the experiment adds none of the requested value. Surface-area checklist: OpenAPI no; product command-line interface no; user docs no; engine pin retained, product version unchanged; schema no; spec amended to match the removal instruction.
- **Removal proof:** Repository-wide word-boundary search for the pilot directory and unique runner filenames has only this spec's six historical/removal reference lines. The complete directory and all 220 tracked files are absent; all seven original record hashes still match their blobs at `8704a47c9c6ab020b93944d87948eef9ad34b735`. Parsed configuration differs from the comparison only in `orly_version`; `orly doctor` and `scripts/check_orly_pin.sh` pass. No product test or runtime symbol changes; the 25 pilot-only checks leave with their runner. The exact pushed removal revision must pass its own final gate and hosted review, recorded in Pull Request Session Notes.

- **Consults:** Indy authorized offline preparation and focused commits, required one budget approval before refresh, and required the overall pilot to remain IN_PROGRESS. Source comparison is `dd917b7ef42dcb883b5192fafe894060aab5845d`; engine 0.13.0 checked with `scripts/check_orly_pin.sh`. Earlier fixture contents were not read. The `orly` checkout remains read-only.
- **Metrics review:** No product analytics/funnel playbook update; local receipts record all approved attempts. Fresh blinded agents used identical selected evidence and separately shuffled orders, with retained advice only in the assisted condition.
- **Skill-chain outcomes:** `context-restore` completed; `orly-spec-new` applied. `orly-write-unit-test` maps each Dimension to a named check; 25 runner tests include refusal and failed-attempt checks. gstack's native review identified four pilot-runner accounting defects; regression checks cover their repairs. Functional offline probes have verdict pass in `receipts/review/evidence.json`; code audit remains separate from blinded pilot reviews. Comparison baselines and every declared boundary check are recorded. Babysitting follows each push; hosted checks and owner review remain separate from local verification.
- **Offline findings:** `pilot.py check`: 20 correct passes, 8 fault rejections, 12 survivors; Node v26.9.0 runs the incumbent Vitest 5.0.3/jsdom 30.1.2 stack. Initial Bun execution started no tests because jsdom workers raised EventTarget errors; these were refused, never counted as controls. Frozen case bytes stayed unchanged. Both offline `orly` commands returned ten unavailable replay slots and zero requests; no Jev accuracy claim follows.
- **Freeze correction:** Conformance found 82 copied literal violations. Named-constant repairs stayed in pilot copies, are recorded as exact source adaptations, and retain all 40 proof outcomes. Review also corrected p16's import order. Both corrections preceded any Jev answer; requirements, classifications and justifications stayed fixed. Final digest: `7c1117c91f17918519b550dd1b27c5e04980bafe53313adac726f175bf015caa`.
- **Approval:** Indy: "okay approved" — approved the frozen upload scope, at most twenty requests including failures, United States dollars (USD) 0.06, no retries or extra planning-model calls. `approval.json` binds input/runner digests and pricing reconfirmed at `2026-10-08T13:21:11.785254+00:00` before admission.
- **Model results:** `pilot.py summarize` reports 20 complete attempts, 20 requests, 51,813 input tokens, 1,122 output tokens and calculated USD 0.002176146. Native agreement 14/20; seeded findings 12/12; native false alarms 3, all withheld; actionable seeded findings 7/12. Nine answers have low strength; zero native insufficient answers. Both subsequent replays report zero requests. Evidence: both live/replay receipts and `receipts/summary.json`.
- **Blinded method approval:** Indy: "yes" — use two fresh in-host agent reviewers with no inherited conversation. Both initial records and forty actual timing intervals are retained; the assisted record binds the frozen unaided bytes and original live-advice hashes. No new provider request ran.
- **Paired results:** `pilot.py summarize`: both agents classify 20/20 correctly and find 12/12 seeded weaknesses; misses and false alarms are zero. Unaided elapsed time is 258.305690044 seconds; assisted is 229.249124959 seconds. Different reviewers and orders prevent causal attribution; human active-review time remains unmeasured.
- **Fixture repair approval:** Indy: "Yes — fix fixture isolation and rerun the checks." — `pilot_test.py` disposable copies now omit the two real review records, alongside omitted receipts. The unchanged suite first failed twice, then passed all 25 checks after repair. Original reviews and approval remain unchanged; the original runner digest identifies historical provider attempts.
- **Recommendation:** Keep experimental advice; paired finding gain is zero. The observed agent-time difference does not establish human savings or justify adoption. Required measurements and repository boundary checks are complete. No newly confirmed production defect, threshold change or gate integration follows.
- **Overnight scope:** Indy: "in agentsfleet - prune merged, pull origin main, and update orly to 0.14, verify and push the PR, ensure the CI job is green, all greptile commits are resolved and we are good to go fater an eye ball from Indy". Fold the pin into this stream; leave its Pull Request unmerged for Indy. The earlier read-only `orly` restriction described pilot preparation; the later release instruction authorizes the separate engine publication.
- **Comparison baseline:** Exact `dd917b7ef42dcb883b5192fafe894060aab5845d`, isolated checkout and owned datastore project: unit 10613 passed / 1 failed, Rust 924 ignored, command-line 16 skipped; integration 899 passed / 0 failed. Both failed unit attempts remain recorded. A focused clipboard rerun passes all seven tests without replacing the failed full result. Evidence: `receipts/review/boundary-baseline.json` and `report.md`.
- **Completed boundary:** `orly gate pr` at pushed `6a9a8d3c7b01e3c73dba4ffeaa4cef241a924656` exits 0. Unit: 10614 passed, zero failed; integration: 899 passed, zero failed; lint and version checks exit 0. Reported package coverage is 100%; command-line function/line floors are 100%. The +1 product pass is the unchanged comparison-run clipboard failure recovering, not a new product test. The pilot adds 25 scoped runner checks outside product Make selections. Earlier disk-full and stopped-datastore attempts remain recorded; no gate or test was patched. Evidence: `receipts/review/boundary-verification.json`. The exact pushed closing revision must pass its own gate before opening the Pull Request.
- **Consumer update:** `bun install -g @agentsfleet/orly@0.14.0 --registry=https://registry.npmjs.org --no-cache` installs 0.14.0. The public tarball hash matches the successful release log. `orly update --no-hooks` writes one file, with 70 already current; parsed configuration differs only in its engine version. `orly doctor` and `scripts/check_orly_pin.sh` pass.
- **Deferrals:** None. Human time remains unmeasured; no human experiment was performed. The approved request allowance is exhausted; no retries or additional provider calls are authorized by the pilot approval.
- **Hosted review follow-up:** Greptile review `5462577534` at `eceefad0512638abb47b3d9e9667977a39316ce3` reported lost timeout output in `pilot_checks.py:148–152`, thread `4223898848`. Indy's recorded overnight instruction authorizes resolving review feedback in this stream. Reopen the same spec, preserve both output streams through cleanup and receipt collection, and prove graceful/forced timeout handling without provider calls. Existing green hosted checks and immutable measurements remain historical evidence; the repaired pushed revision requires fresh checks and review.
- **Timeout repair proof:** Both cleanup branches retain standard output and standard error in a specific timeout exception; the collector saves both in the failed receipt. Unknown usage, unavailable slots and spent reservations remain unchanged. The old-source regression run has three failing assertions across two methods; current source passes all 25 checks, including randomized order with seed 37. The native adversarial review finds no further defect; fixture/test/raw-receipt review coverage is reduced. Three fresh functional probes pass with twelve unchanged input fingerprints and zero provider requests. Source, output hashes and the two-row test ledger are in `receipts/review/timeout/verification.json`. The final pushed repair still requires `orly gate pr` and fresh hosted review.
