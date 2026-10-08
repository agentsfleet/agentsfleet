# Jev assertion pilot results

**Recommendation: keep Jev experimental and advisory.** Its native answers
identified every seeded inadequate assertion, while the existing confidence
handling surfaced seven of twelve. Independent reviewer benefit and review
time remain **unmeasured**; this evidence does not justify adoption or a gate.
Adoption remains Kishore's separate decision.

**Overall status: IN_PROGRESS.** The approved model measurement is complete.
Separate blinded reviewer sessions are unavailable, and repository-wide checks
and comparison baselines remain due before a Pull Request.

Source revision: `dd917b7ef42dcb883b5192fafe894060aab5845d`.
Frozen input digest: `7c1117c91f17918519b550dd1b27c5e04980bafe53313adac726f175bf015caa`.
Approved runner digest: `4d0644fd93a46bac785f5282392310f24634092bd4843a9189acd1b02875f9e8`.
The [spec](../../docs/v2/active/M216_001_P2_DOCS_INFRA_JEV_ASSERTION_REVIEW_PILOT.md),
[case ledger](cases.json) and [freeze inventory](freeze.json) bind scope,
requirements, original source hashes, evidence references and expected labels.
The [protocol](README.md) specifies checking and separate blinded review sessions.

## Frozen cases and executable controls

Each behavior contributes one case of every existing class. The
[ledger](cases.json) preregisters requirements, expected labels, justifications,
source hashes and adaptations before advice. Selected implementations and
helpers are complete except deliberate insufficient-case omissions. Earlier
`orly` evaluation fixtures were not consulted.

| Behavior | exact | weak | wrong_target | missing | insufficient | Correct → deliberate fault |
|---|---|---|---|---|---|---|
| Clipboard value | p12 | p01 | p09 | p05 | p16 | Supplied value → button label |
| Semantic class merging | p06 | p17 | p14 | p10 | p03 | Size plus color → color only |
| Relative-time boundary | p02 | p11 | p19 | p15 | p08 | `5 seconds ago` → `just now` |
| Elapsed-time rounding | p13 | p07 | p04 | p18 | p20 | `1m 0s` → `59s` |

The ledger records original implementation/test paths and selected test names.
There are four source-derived exact controls and sixteen seeded controls:
twelve inadequate assertions and four omitted-helper inputs. Local execution
still includes those strong helpers. None is a newly confirmed production defect.

`python3 pilots/jev-assertions/pilot.py check` produced **20 correct passes,
8 fault rejections and 12 faulty passes**, with **0 provider requests**:
[structured proof](receipts/checks.json), [correct run](receipts/correct.txt),
[faulty run](receipts/faulty.txt). Four rejections are visible exact assertions;
the other four come from the hidden insufficient-case helpers. These omissions
affect classification evidence, not local test quality.

CopyButton's exact test rejects copying the button label instead of the supplied
value. The p01 weak assertion checks only that `writeText` was called once, so
that incorrect value passes. The original selected test checks the exact value.

## Advice and review measurements

`pilot.py summarize` produced the following totals from the retained
[live batch a](receipts/live-a.json), [live batch b](receipts/live-b.json) and
[summary](receipts/summary.json). Both native commands exited 0 with ten requests
and ten complete answers. All twenty attempts remain in the totals; failed and
unavailable attempts were zero. The two original offline cache misses remain
in [offline-a](receipts/offline-a.json) and [offline-b](receipts/offline-b.json).
Subsequent exact [replay-a](receipts/replay-a.json) and
[replay-b](receipts/replay-b.json) each returned ten complete answers and zero
requests; replay usage is never added to live spending.

| Measurement | Native Jev answer | Existing confidence handling |
|---|---|---|
| Exact classification agreement | 14/20 | Three matching answers withheld for low strength |
| Confirmed seeded weaknesses | 12/12 | 7/12 actionable concerns; five withheld |
| Missed seeded weaknesses | 0/12 | Five produce inspection rather than a repair suggestion |
| False alarms | 3/8 adequate or omitted-context controls | All three withheld; zero actionable false alarms |
| Missing-context classification | 0/4 `insufficient` answers | All four low strength and require inspection |
| Withholding | Nine answers below 0.8 | Native class retained separately |
| Independent reviewer findings and time | Unmeasured | Separate blinded sessions unavailable |

Confirmed findings mean a native `weak`, `wrong_target` or `missing` answer on
one of the twelve known inadequate assertions. Subclass mistakes still confirm
inadequacy. False alarms use that same positive rule on exact or insufficient
inputs. This distinguishes label agreement from finding detection. Strength is
the minimum of answer confidence and chosen-class probability; the existing
0.8 threshold and question were unchanged. No native answer was `insufficient`.

Per-class agreement was exact 4/4, weak 4/4, wrong_target 2/4, missing 4/4 and
insufficient 0/4. Jev called p14 and p19 `missing` instead of `wrong_target`.
It called omitted-helper cases p03, p08 and p20 `weak`, and p16 `exact`.
All six classification errors had low strength. CopyButton's weak p01 was
correctly classified `weak` at strength 0.33 and withheld; exact p12 was `exact`
at strength 1. All four wrong-target controls were withheld.

| Origin | Cases | Matching labels | Native findings | Native false alarms | Actionable findings |
|---|---|---|---|---|---|
| Source-derived exact controls | 4 | 4 | 0 | 0 | 0 |
| Seeded controls | 16 | 10 | 12 | 3 | 7 |
| Newly confirmed production defects | 0 | N/A | 0 | N/A | 0 |

The [frozen author inspection](reviews/unaided-author.json) predates advice and
records no independence, blinding or timing. The
[protocol](README.md#review-protocol) specifies separate blinded sessions and
paired decisions/time. Those sessions are absent; author expectations and the
code-audit agent cannot establish reviewer findings, misses, false alarms or
time savings. Their comparison remains unmeasured.

## Requests, usage and cost

The live receipts and `pilot.py summarize` report **20/20 requests and
reservations**, zero unreconciled reservations, **51,813 input tokens** and
**1,122 output tokens**, with zero unknown-usage slots. Calculated spending is
**United States dollars (USD) 0.002176146**, using the confirmed tariff; it is
not an invoice reconciliation. No retries or extra planning-model calls ran.
The conservative reserved bound was USD 0.05505024 against the USD 0.06 ceiling.

Summing live `elapsedMs` fields gives **6.471 seconds**; their median is
**311.104 milliseconds**, with a 283.909–442.053 millisecond range. These are
provider-attempt timings, not reviewer time or complete command runtime.

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

Kishore approved the frozen scope and budget with **"okay approved"**, recorded
in [approval.json](approval.json). The official
[Jev 1.13.0 model table](https://docs.typesafe.ai/models), reconfirmed at
`2026-10-08T13:21:11.785254+00:00`, lists USD 0.042 per million input tokens,
free output and a 64k context limit. Reserving 65,536 input tokens per request
bounded twenty requests at USD 0.05505024. Approval allowed at most twenty
requests including failures, USD 0.06, no retries and no extra planning calls.

The [runner](pilot_measure.py) locks and persists ten-request reservations
before each batch, checks frozen bytes and its own approved digest, refuses
duplicate launches, and keeps failed/crashed reservations spent. Failed or
unavailable token usage remains unknown. The request ceiling is locally
enforced; the dollar bound depends on the confirmed provider tariff/context
limit. A provider account dollar cap has not been verified. Both admitted
batches are retained in [reservations.json](receipts/reservations.json); the
approved request allowance is exhausted. Direct refresh or deleting the ledger
would bypass local enforcement and is outside this approval.

## Verification and limitations

`python3 pilots/jev-assertions/pilot_test.py` ran **25 checks, all passing**
([output](receipts/runner-tests.txt)), covering admission, frozen evidence,
accounting, failures, process cleanup and contaminated reviews. Synthetic replies
exist only in isolated unit tests. Runner bytes and frozen proof inputs remain
unchanged since the offline commit; no redundant proof run was needed.

`ruff check pilots/jev-assertions/*.py` returned `All checks passed!`.
Executable proofs used Node v26.9.0, Vitest 5.0.3 and jsdom 30.1.2. Initial Bun
worker setup failed before tests ran; the checker refused those runs. Node
corrected setup without a production change.

`make harness-verify` returned `ALL GATES GREEN`: 50 source files had zero
literal violations and the milestone/interface check had zero hits
([raw output](receipts/conform.txt)). `make check-version` returned
`all versions match 0.58.0`. `gitleaks protect --staged --redact --no-banner`
reported `no leaks found` ([scan output](receipts/secret-scan.txt)). The receipt
retains an initial conformance timeout and completed audit. Pre-advice code
review repaired four accounting/evidence issues with regression checks in
[pilot_test.py](pilot_test.py). Outside review-model calls were skipped under
the spending restriction; native code audits are separate from blinded reviews.
Full unit/lint/datastore integration lanes, baselines and test delta remain due
before a Pull Request. These Section checks establish no repository-wide result.
The diff adds only contributor pilot artifacts; product behavior, architecture,
version, question catalog, threshold, hooks and gates are preserved.

Four correlated families and deliberately balanced seeded variants cannot
estimate production defect prevalence or calibrate the confidence threshold.
The provisional freeze was corrected for named constants and p16 import order
before advice; the spec records that history. Requirements, labels and
justifications stayed fixed. `verify_freeze()` still returns the registered
digest; cases and runner bytes never changed after advice. Missing-context
errors and absent independent reviews prevent a claim of reviewer improvement.

Quality assurance (QA) uses the repository-required functional review route.

## Exploratory QA and Verification Results

| Field | Current evidence |
|---|---|
| Revision / inputs | `908d3cde491` plus actual approval and live receipts; registered input and runner digests unchanged |
| Scope / authority | Local accounting and replay; repository-required review, pilot-only writes |
| Runtime / tools | Python 3, `orly` 0.13.0, Bun 1.4.2 evidence recorder |
| Isolation / bound | Owned worktree and private cache; five-minute window, three probes, 635 milliseconds command time |
| Outcome | **pass**, no open checks: [derived evidence](receipts/review/live/evidence.json), [charter](receipts/review/live/charter.md) |

| Check | Expected → observed | Outcome |
|---|---|---|
| `pilot.py summarize` | Twenty actual attempts; findings/withholding distinct; paired review unmeasured → matched | pass |
| `pilot.py replay` | Twenty exact cached answers; zero requests → matched live answers, assessments and usage | pass |
| Resummarize after replay | No double-counted usage → totals unchanged | pass |

[Checkpoint 002](receipts/review/live/exploration-002.json) schedules replay;
[checkpoint 003](receipts/review/live/exploration-003.json) checks its accounting
effect. Earlier [offline evidence](receipts/review/evidence.json) retains
[002](receipts/review/exploration-002.json),
[003](receipts/review/exploration-003.json),
[004](receipts/review/exploration-004.json),
[005](receipts/review/exploration-005.json),
[006](receipts/review/exploration-006.json) and
[007](receipts/review/exploration-007.json) as history; pre-live pending summaries
are not current advice evidence. Independent reviews and repository-wide suites
remain unmeasured/pending. No new tests or operating-rule learnings arose from
these receipt checks; owned subprocesses exited and private replay state remains local.
