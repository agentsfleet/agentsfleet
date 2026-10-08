# Jev assertion pilot results

**Status: IN_PROGRESS.** Offline controls demonstrate measurable assertion
weaknesses; Jev usefulness and independent reviewer benefit are unmeasured.
Keep advice experimental until the approved measurements exist.

Source revision: `dd917b7ef42dcb883b5192fafe894060aab5845d`.
Frozen input digest: `7c1117c91f17918519b550dd1b27c5e04980bafe53313adac726f175bf015caa`.
Approved runner digest to propose: `4d0644fd93a46bac785f5282392310f24634092bd4843a9189acd1b02875f9e8`.
The [spec](../../docs/v2/active/M216_001_P2_DOCS_INFRA_JEV_ASSERTION_REVIEW_PILOT.md),
[case ledger](cases.json) and [freeze inventory](freeze.json) bind scope,
requirements, original source hashes, evidence references and expected labels.
The [protocol](README.md) specifies checking and separate blinded review sessions.

## Frozen cases and executable controls

Each behavior contributes one case of every existing class. Original production
tests informed the requirement and exact control; pilot tests are adaptations,
with full implementations and necessary copied helpers. Listed import and named-constant adaptations preserve the copied behavior. Expected labels and
justifications were frozen before advice. Earlier `orly` evaluation fixtures
were not consulted; this is a fresh source-derived sample.

| Behavior | exact | weak | wrong_target | missing | insufficient | Correct → deliberate fault |
|---|---|---|---|---|---|---|
| Clipboard value | p12 | p01 | p09 | p05 | p16 | Supplied value → button label |
| Semantic class merging | p06 | p17 | p14 | p10 | p03 | Size plus color → color only |
| Relative-time boundary | p02 | p11 | p19 | p15 | p08 | `5 seconds ago` → `just now` |
| Elapsed-time rounding | p13 | p07 | p04 | p18 | p20 | `1m 0s` → `59s` |

Original implementation/test paths and selected test names are recorded per
family in [cases.json](cases.json). Source-derived exact controls account for
four cases; sixteen are seeded controls. Twelve assertions deliberately fail
to reject their family fault. Four insufficient inputs omit their exact
assertion helper while local execution still includes it. None is a newly
confirmed production defect.

`python3 pilots/jev-assertions/pilot.py check` produced **20 correct passes,
8 fault rejections and 12 faulty passes**, with **0 provider requests**:
[structured proof](receipts/checks.json), [correct run](receipts/correct.txt),
[faulty run](receipts/faulty.txt). Four rejections are visible exact assertions;
the other four come from the hidden insufficient-case helpers. These omissions
affect classification evidence, not local test quality.

CopyButton's exact assertion rejects copying `Copy workspace ID` instead of the
supplied value. Identifier (ID) remains abbreviated in this literal source
label. The p01 weak assertion checks only that `writeText` was called once, so
the wrong copied value passes. The original selected test checks the exact
value; this is a seeded demonstration, not a production defect finding.

## Advice and review measurements

Both manifests contain ten items, with two of each expected class. The exact
offline commands returned native exit 2 and ten unavailable replay results
each, requests 0: [batch a](receipts/offline-a.json),
[batch b](receipts/offline-b.json). The runner's exit 0 means receipt collection,
not available advice. No refresh or provider call has occurred.

| Measurement | Observed so far | Evidence / limit |
|---|---|---|
| Native answers; confidence withholding | Unmeasured | No live advice; offline replay absent |
| Confirmed findings, misses, false alarms | Unmeasured for Jev | Keep twelve seeded weaknesses in the eventual denominator |
| Independent unaided versus assisted review | Unmeasured | Separate blinded sessions unavailable |
| Reviewer time | Unmeasured | Author inspection untimed and knows labels |
| Provider requests / reservations | 0 / 0 | Offline receipts and [summary](receipts/summary.json) |
| Input/output tokens and cost | No live usage; zero known spending | Pending slots are not failed requests or zero-token replies |
| Seeded versus real defects | Four exact controls, sixteen seeded controls; no confirmed real defect | Preregistered ledger and executable proof |

The [frozen author inspection](reviews/unaided-author.json) explicitly records
`independent: false`, `blinded: false`, `advice_seen: false`, and unknown review
time. It cannot demonstrate reviewer improvement. If independent sessions
remain unavailable, the final report must leave their comparison unmeasured.
The [summary](receipts/summary.json) retains twenty pending slots; its zero
native counters express absent data, not accuracy or absence of false alarms.

For actual attempts, report exact-label agreement separately from inadequate
assertion detection and from concerns withheld by confidence. Confirmed finding
recall uses all twelve seeded inadequate cases; unavailable results remain in
that denominator. Inspect native `insufficient` separately from low strength.
Pair reviewer findings and active review seconds by case, and retain failure,
request count, model elapsed time, input/output tokens and unknown costs.
Reviewer finding decisions are counted independently of their classifications;
their classification agreement cannot silently replace their finding decision.

## Approval checkpoint

Only the manifest requirements, the existing fixed question, and selected
whole-file implementation/test/helper text and reference metadata are uploaded.
The exact per-case paths, line bounds and hashes are in
[checks.json → uploads](receipts/checks.json). There are **45 distinct source
files**, **120,882 selected source bytes including repetitions** and **141,724
state bytes across twenty items**; the largest item is **15,010 bytes against
the 24,576-byte state cap**. These counts come from the check receipt's upload
inventory; they are bytes, not token estimates. Manifest sizes are 10,728 and
10,324 bytes against the 65,536-byte cap.

The scope is forty fixture files plus `helpers/setup.ts`, `Button.tsx`,
`utils.ts`, `use-resettable-timeout.ts`, and `app-utils.ts`, selected by case.
Four opaque `check-*.ts` definitions are deliberately omitted. The ledger,
expected labels, justifications, reviews, reports, spec and runner are outside
model inputs. Copy helpers remain complete; ordinary dependency packages are
named by imports and use the installed source workspace stack.

Proposed approval: **at most 20 provider requests total, no retries or extra
planning-model calls, and United States dollars (USD) 0.06**. The official
[Jev 1.13.0 model table](https://docs.typesafe.ai/models), checked for this
preparation, lists USD 0.042 per million input tokens, free output and a 64k
context limit. Reserving 65,536 input tokens for each request bounds twenty
requests at USD 0.05505024 under that tariff. Verify the current tariff and
context limit again before admission.

The [runner](pilot_measure.py) locks and persists ten-request reservations
before each batch, checks frozen bytes and its own approved digest, refuses
duplicate launches, and keeps failed/crashed reservations spent. Failed or
unavailable token usage remains unknown. The request ceiling is locally
enforced; the dollar bound depends on the confirmed provider tariff/context
limit. A provider account dollar cap has not been verified. No approval record
or live reservation exists yet.

## Verification and limitations

`python3 pilots/jev-assertions/pilot_test.py` ran **25 checks, all passing**:
[raw output](receipts/runner-tests.txt). The checks exercise changed evidence,
label leakage, path escape, missing approval, stale pricing, changed engine,
concurrent/duplicate reservation, corrupt accounting, failed attempts, native
classification versus withholding, mismatched receipts, review contamination
and owned-process timeout cleanup. Synthetic model replies are unit-test
inputs only and never enter actual attempt receipts.

`ruff check pilots/jev-assertions/*.py` returned `All checks passed!`.
Executable proofs used Node v26.9.0, Vitest 5.0.3 and jsdom 30.1.2. Initial Bun
runs failed jsdom EventTarget worker setup and executed no tests; the checker
refused those runs. Using the framework's Node runtime corrected setup without
changing frozen case bytes or production files.

`make harness-verify` returned `ALL GATES GREEN`: 50 source files had zero
literal violations and the milestone/interface check had zero hits
([raw output](receipts/conform.txt)). `make check-version` returned
`all versions match 0.58.0`. `gitleaks protect --staged --redact --no-banner`
reported `no leaks found` ([scan output](receipts/secret-scan.txt)). An earlier
conformance attempt exceeded 120 seconds; the subsequent completed audit is
recorded in the same receipt. Native code review found four pilot-runner issues:
malformed replies prevented receipts, error handling discarded native evidence,
paired metrics ignored finding decisions, and summaries accepted corrupt
reservation totals. Repairs retain raw output and validated native answers,
count findings separately, and validate ledgers in both admission and summary.
The corresponding checks are `test_malformed_output_retains_attempt_receipt`,
`test_unexpected_exit_retains_valid_native_answers`,
`test_review_findings_are_separate_from_classifications`, and
`test_summary_rejects_corrupt_reservation_totals` in [pilot_test.py](pilot_test.py).
This code audit is separate from independent blinded pilot reviews.
Outside review-model calls were skipped under the explicit spending checkpoint.
The declared full
unit, lint and isolated datastore integration lanes, comparison baselines and
test delta remain due before the Pull Request. Focused pilot checks do not
establish a repository-wide test result. No architecture document changes are
needed: the pilot adds local contributor artifacts and defines no product
service, storage namespace or production path. No product release or changelog
claim is made.

This sample has only four correlated families, deliberately balanced labels,
seeded assertion variants and omitted-context controls. It cannot estimate
production defect prevalence. Model agreement alone cannot prove reviewer
usefulness. Unknown costs, unavailable advice and missing independent sessions
must remain visible. The provisional freeze preceded conformance; its
digest `fd9042b935f01efe5aeff155c8efbedf656177bf80bda060094615b08e62ee5f`
was superseded solely for named-constant repairs, recorded provenance and an
import-order correction in p16,
before any Jev answer. Requirements, labels and justifications did not change.
The final inventory above binds the proposed upload. The spec stays IN_PROGRESS until actual measurements and
an evidence-supported recommendation are recorded; adoption remains Kishore's
separate decision.

## Exploratory quality assurance (QA) and verification results

The functional runner probes have verdict **pass**, with no open checks:
[derived evidence](receipts/review/evidence.json),
[scope and expected behavior](receipts/review/charter.md).
They ran in the owned worktree at opening commit `5172e8579`, before the focused
pilot commit. The recorder uses Bun 1.4.2; executable fixture checks use Node.
The first 21-check probe and earlier missing-approval probe are retained as
superseded. Current evidence proves 25 runner checks, the 20/8/12 controls,
zero-request offline replay, twenty pending summary slots and refusal of live
execution without approval. Synthetic provider replies stay in temporary unit
tests. Required repair checks used finite deadlines after the original
five-minute exploration budget expired; that original budget was not reset.

The recorded progression is [checkpoint 002](receipts/review/exploration-002.json),
[003](receipts/review/exploration-003.json),
[004](receipts/review/exploration-004.json),
[005](receipts/review/exploration-005.json),
[006](receipts/review/exploration-006.json), and
[007](receipts/review/exploration-007.json).
Coverage excludes provider availability, independent reviewer benefit and
repository-wide boundary checks. No browser or production service was changed.
