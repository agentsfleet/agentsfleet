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

# M216_001: Measure Jev assertion advice on frozen reviewer cases

**Prototype:** v2.0.0
**Milestone:** M216
**Workstream:** 001
**Date:** Oct 08, 2026
**Status:** IN_PROGRESS
**Priority:** P2 — bounded contributor experiment
**Categories:** Documentation (DOCS), Infrastructure (INFRA)
**Batch:** B1 — independent pilot artifacts
**Branch:** feat/m216-jev-assertion-pilot
**Baseline revision:** dd917b7ef42dcb883b5192fafe894060aab5845d
**Test Baseline:** unit=10613 integration=899 — unit comparison retains one failure
**Baseline evidence:** pilots/jev-assertions/report.md
**Depends on:** recorded comparison baselines and green declared repository checks; owner review before merge
**Provenance:** agent-generated from Indy's Oct 08, 2026 pilot instruction
**Canonical architecture:** existing product architecture remains unchanged; this is an isolated contributor measurement

## Overview

**Goal (testable):** Twenty frozen cases distinguish exact, weak, wrong_target, missing and insufficient assertions, retain every approved Jev attempt, and accompany a verified orly 0.14.0 consumer pin.
**Problem:** Passing tests can accept incorrect results; model advice has not been measured on this fresh `agentsfleet` sample.
**Solution summary:** Copy four real behaviors into isolated pilot fixtures, establish expected classifications and executable counterexamples offline, then measure the existing `verify.assertion` question after one owner-approved upload and budget checkpoint. Keep all model results advisory. Required measurements, the recommendation and the timeout-diagnostics repair are complete. The final pushed repair must pass fresh repository and hosted checks before owner review.

## PR Intent & comprehension handshake

- **PR title (eventual):** Measure Jev assertion advice and upgrade orly to 0.14.0
- **Intent:** Give reviewers evidence about whether Jev helps identify inadequate assertions without changing product behavior or gate authority.
- **Handshake:** Prepare offline first; submit exactly two ten-item manifests only after approval; keep seeded controls separate from real defects. ASSUMPTIONS I'M MAKING: four correlated behavior families are suitable for a small pilot; unavailable independent blinded reviews yield an explicitly unmeasured paired comparison.

## Implementing agent — read these first

1. `.orly/LOCAL.md` and `.orly/dispatch/lifecycle.md` — project commands and lifecycle timing.
2. `ui/packages/design-system/src/design-system/CopyButton.test.tsx` — exact clipboard-value starting control.
3. `ui/packages/design-system/src/utils.test.ts` and `ui/packages/design-system/src/design-system/time-utils.test.ts` — class merging and relative-time controls.
4. `ui/packages/app/lib/events/run-figures-format.test.ts` — elapsed-time rounding control.
5. https://docs.typesafe.ai/models — model, context limit and input pricing; refresh the documented price before live admission.

## Files Changed (blast radius)

All paths below are relative to this repository. The pilot prefix is `pilots/jev-assertions/`; no production, catalog, threshold, hook or gate edits are authorized.
Indy's subsequent overnight instruction also authorizes the managed engine pin update in this workstream.

| File | Action | Why |
|------|--------|-----|
| `docs/v2/pending/M216_001_P2_DOCS_INFRA_JEV_ASSERTION_REVIEW_PILOT.md` → `docs/v2/active/M216_001_P2_DOCS_INFRA_JEV_ASSERTION_REVIEW_PILOT.md` → eventual same basename in `done/` | CREATE / MOVE / EDIT | Lifecycle and measured evidence |
| `pilots/jev-assertions/README.md`, `report.md` | CREATE / EDIT | Protocol, checking commands and concise results |
| `pilots/jev-assertions/cases.json`, `freeze.json` | CREATE | Preregistered answer ledger and immutable byte inventory |
| `pilots/jev-assertions/manifests/a.json`, `manifests/b.json` | CREATE | Ten neutral identifiers per upload |
| `pilots/jev-assertions/pilot.py`, `pilot_checks.py`, `pilot_measure.py`, `pilot_test.py` | CREATE | Minimal checking, bounded command execution, accounting and negative tests |
| `pilots/jev-assertions/vitest.config.ts`, `helpers/setup.ts` | CREATE | Execute copied tests with the incumbent React test stack |
| `pilots/jev-assertions/helpers/Button.tsx`, `utils.ts`, `use-resettable-timeout.ts`, `app-utils.ts` | CREATE | Complete necessary copied helper evidence |
| `pilots/jev-assertions/helpers/check-{copy,classes,relative,elapsed}.ts` | CREATE | Executable assertion helpers deliberately omitted only in insufficient inputs |
| `pilots/jev-assertions/fixtures/p01/` through `fixtures/p20/`, each containing only `implementation.tsx` and `test.tsx` | CREATE | Twenty isolated cases; paths enumerate p01–p20 with the same two leaf names |
| `pilots/jev-assertions/reviews/unaided-author.json`, `reviews/session-template.json` | CREATE | Frozen pre-advice inspection and independent review record shape |
| `pilots/jev-assertions/receipts/{checks,offline-a,offline-b,live-a,live-b,replay-a,replay-b,summary}.json` | CREATE | Reproducible attempt and proof receipts |
| `pilots/jev-assertions/receipts/{correct,faulty,runner-tests,conform,secret-scan}.txt` | CREATE | Raw local checking output |
| `pilots/jev-assertions/receipts/review/` | CREATE | Repository-required review probe receipts and checkpoints; results stay in `report.md` |
| `.orly/orly.json` | EDIT | Update the managed engine pin to published 0.14.0 through `orly update --no-hooks`. |
| `pilots/jev-assertions/approval.json`, `receipts/reservations.json`, `reviews/{unaided,assisted}.json` | CREATE after corresponding real evidence exists | One actual approval, append-only request reservations and actual blinded reviews |

Temporary proof copies and dependency installation outputs are disposable local runtime state, never committed. Existing `orly` replay files remain local private cache, not model inputs or deliverables.

## Applicable Rules

- `.orly/docs/greptile-learnings/RULES.md`: No Dead Code (NDC), standard parsers (PSR), a test must fail when its subject is incorrect (TCF), Unified Form for Symbols (UFS), Prompt-injection Resistance from user Input (PRI), test naming (TNM) and grounding in source (GRD).
- `.orly/dispatch/write_spec.md` and `.orly/docs/TEMPLATE.md`: preparation precedes measurement and human approval remains a named dependency.
- `.orly/dispatch/write_any.md`, `.orly/dispatch/write_ts_adhere_bun.md`, `.orly/dispatch/write_python.md`: bounded files, copied evidence provenance and standard-library orchestration.
- `.orly/dispatch/verify.md`: focused proofs establish Sections; declared repository checks remain due before the Pull Request (PR).

## Applicable Gates

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

## Interfaces

`python3 pilots/jev-assertions/pilot.py check|replay|live|summarize`

- `check` runs executable proofs and validates frozen manifests locally; `replay` makes no provider request.
- `live` requires a separately recorded approval and private one-shot reservation ledger; only the existing `orly` command uploads selected source.
- Cases record identifier, family, origin, requirement, full source revision, source references, expected class, justification, missing context and proof expectations.
- Manifests contain only stage, neutral identifier, unchanged question, requirement and evidence selectors. Neither labels nor justifications are model inputs.

## Failure Modes

| Mode | Cause | Handling (system response + caller observation) |
|------|-------|------------------------------------------------|
| Changed or leaking evidence | Digest mismatch, labels in upload or wrong path | Refuse before command; `test_freeze_refusal` |
| Misleading proof | Syntax/import failure mistaken for discriminating assertion | Require correct-copy success and matching assertion failures; `test_fault_discrimination` |
| No approval / quota exhausted / duplicate launch | Missing bound approval or spent reservation | Refuse before upload; `test_live_approval`, `test_request_reservations` |
| Process crash or unavailable provider | Timeout, invalid reply, interrupted batch or missing usage | Keep reservation, failure and timeout output; no retry; unknown usage stays unknown; `test_attempt_accounting`, `test_live_failure_consumes_reservation_without_retry` |
| Reviewer contamination / missing session | Author knows labels, reused session, advice disclosed early | Paired comparison remains unmeasured; `test_comparison_metrics` |

## Invariants

1. Frozen files and uploads are immutable after registration: hashes are checked before proofs, replay and live admission.
2. Model inputs omit expected classes, justifications and review decisions: admission compares exact allowed manifest fields and fixture-only references.
3. Total provider requests ≤20, including failed attempts: exclusive creation and locked append-only batch reservations; each batch is admitted once.
4. Proposed ceiling is United States dollars (USD) 0.06 at the documented USD 0.042 per million input tokens, output free: reserve 65,536 input tokens per admitted request, giving a conservative twenty-request bound of USD 0.05505024. Approval must confirm pricing; unsupported pricing refuses live admission.
5. Production preservation: diff paths are limited to this spec, pilot artifacts and the authorized engine pin; faults execute only in temporary fixture copies.
6. Native answers, uncertainty and availability remain distinct: summarize original receipt fields and retain unmeasured usage without zero substitution.

## Metrics & Observability

No product or operator analytics change. Local pilot receipts retain native command output, case identifiers, result classes, timing, usage and calculated cost; selected source is uploaded solely after approval and secret scanning. The runner never resolves credentials. Proof: `test_attempt_accounting` and `test_live_approval`.

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

Regression scope: product behavior, source tests, catalog, confidence threshold, hooks and gates receive no diff. At least half of runner checks exercise refusal, failed attempts or contamination. No performance or concurrency claim is made about product code.

## Acceptance Rubric (single scoring surface)

| # | Criterion | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|-----------|---------------------|----------|----------|-----------------|
| R1 | Frozen cases and executable controls | `python3 pilots/jev-assertions/pilot.py check` | exit 0; 20 correct passes; 8 fault rejections and 12 surviving deliberately inadequate assertions | P0 | ✅ 20 correct passes, 8 rejections, 12 survivors; `receipts/checks.json` |
| R2 | Admission and accounting refusals | `python3 pilots/jev-assertions/pilot_test.py` | exit 0; timeout output retained; no provider request | P0 | ✅ 25 passed, zero failed; graceful/forced timeout output retained; zero provider requests; `receipts/review/timeout/verification.json` |
| R3 | Offline replay and final truthful measurement | `python3 pilots/jev-assertions/pilot.py replay` and `python3 pilots/jev-assertions/pilot.py summarize` | Replay requests 0; summary retains twenty slots, unavailable attempts and paired-review status | P0 | ✅ 20 complete attempts, replay requests 0, paired status measured; `receipts/summary.json` |
| R4 | Scope and lifecycle | `git diff --name-only origin/main...HEAD` | Only Files Changed paths; completed spec in done with boundary evidence | P0 | ✅ Pilot prefix, completed spec and engine pin only; `receipts/review/boundary-verification.json` |
| S1 | Conform | `make harness-verify` | exit 0 | P0 | ✅ ALL GATES GREEN; 50 source files, zero literal violations; `receipts/conform.txt` |
| S2 | Unit boundary | `make test-unit-all` | exit 0 at PR boundary | P0 | ✅ exit 0: 10614 passed, zero failed; `receipts/review/boundary-verification.json` |
| S3 | Lint boundary | `make lint-all` | exit 0 at PR boundary | P0 | ✅ exit 0: all declared lint checks green; `receipts/review/boundary-verification.json` |
| S4 | Integration boundary | `make test-integration-rustd` | exit 0 at PR boundary in isolated datastore environment | P0 | ✅ exit 0: 899 passed, zero failed; `receipts/review/boundary-verification.json` |
| S5 | Version preservation | `make check-version` | exit 0 | P0 | ✅ all versions match 0.58.0 |
| S6 | No secrets | `gitleaks protect --staged --redact --no-banner` | exit 0 | P0 | ✅ no leaks found; `receipts/secret-scan.txt` |

## Dead Code Sweep

N/A — no production files or symbols deleted or renamed. Temporary proof copies are scoped runtime resources cleaned by their context manager.

## Out of Scope

- Runtime adoption, new questions, calibration changes, production fixes and gate integration.
- Earlier `orly` evaluation fixtures, tuning after freezing, retrying failed requests and extra planning-model calls.
- Claiming independent reviewer improvement from the author's labels or from classification agreement alone.
- Publishing, merging, a product release, docs-repository edits or another agent's worktree.

## Product Clarity (authoring record)

1. **Successful user moment:** A reviewer sees a weak test pass an incorrect copied value, then can assess Jev's actual usefulness with traceable evidence.
2. **Preserved user behaviour:** Every product caller and existing verification command retains its behavior.
3. **Optimal-way check:** Existing advice and source tests supply the smallest relevant measurement; independent reviewer availability limits the paired result.
4. **Rebuild-vs-iterate:** Iterate with isolated artifacts; a general evaluation service adds upkeep without improving this bounded sample.
5. **What we build:** Frozen cases, two manifests, executable counterexamples, bounded orchestration and one results report.
6. **What we do NOT build:** A new reviewer gate or autonomous adoption policy.
7. **Fit with existing features:** Measures the existing assertion question while preserving its advisory authority.
8. **Surface order:** N/A — internal pilot; command-line checking only.
9. **Dashboard restraint:** N/A — no product interface or quality badge.
10. **Confused-user next step:** Run the report's checking commands and inspect the named evidence receipt.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** Offline evidence, authorized measurement and truthful reporting are separate dependency-ordered Sections.
- **Alternatives considered:** Direct custom provider integration duplicates the pinned command and its scanner; manual batch execution cannot enforce a cross-run request ceiling.
- **Patch-vs-refactor verdict:** A pilot addition is sufficient. Quality ceiling: a larger evaluator cannot create independent reviewers or enlarge this approved sample; no product refactor is justified.
- **Surface-area checklist:** OpenAPI no; product command-line interface no; user docs no; engine version yes, product version no; schema/removal no; rule conflict no. The explicit consumer pin update accompanies the pilot.

## Discovery (consult log)

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
