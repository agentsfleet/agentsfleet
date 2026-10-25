# PR #784 coverage repair

## Observed failure

The [hosted aggregate](https://github.com/agentsfleet/agentsfleet/actions/runs/40211187049/job/121226327069) failed at `07672e528` after all four Rust test shards passed.
Global line coverage was 74,210 / 74,603, or 99.4732 percent; the required floor is 99.5.
Changed-line coverage was 3,045 / 3,150, or 96.6667 percent; the required floor is 99.

Thirty-six touched files also missed their 99 percent floor.

This is missing behavioral coverage, separate from the earlier intermittent kernel admission failure.
The fresh hosted kernel suite passed all 61 tests.
The source-chain diagnostic preserves the original cause for another occurrence; it does not establish that intermittent cause.

## Added behavior checks

The table maps the repair to observable outcomes.
Focused package results do not replace repository verification or a fresh hosted coverage grade.

| Changed unit | Required proof | Test file | Current evidence |
|---|---|---|---|
| Shared compaction and history paging | Cancellation invokes no model or store; empty, oversized, stalled or missing source refuses publication | [refusals.rs](../../rustd/crates/afd_context/tests/refusals.rs) | Seven focused tests passed |
| Summary validation and window bounds | Whitespace goals refuse; zero operating allowance retains the physical window | [refusals.rs](../../rustd/crates/afd_context/tests/refusals.rs) | Same seven-test result |
| Private diagnostics and wire documents | Debug excludes private values while JSON retains the complete checkpoint | [redaction.rs](../../rustd/crates/afd_wire/tests/redaction.rs) | 74 focused wire tests passed |
| Runtime history and progress | Truncation invalidates the pending group; source reads emit no visible summary progress | [index_tests.rs](../../rustd/crates/afr_agent/src/loop/context/index_tests.rs), [progress.rs](../../rustd/crates/afr_agent/src/loop/context/effects/progress.rs) | Agent library passed, 142 tests |
| Runtime checkpoint memory | Poisoned storage refuses load and commit without replacing the existing notes | [testing.rs](../../rustd/crates/afr_agent/src/testing.rs) | Same agent result |
| Runtime summary settlement | Invalid and truncated summaries retain known usage without publishing notes | [compaction_tests.rs](../../rustd/crates/afr_agent/src/loop/compaction_tests.rs) | Same agent result |
| Authoring request admission and errors | An irreducible request refuses before output; shared errors preserve typed causes and safe codes | [prepare_tests.rs](../../rustd/crates/afd_authoring/src/model/prepare_tests.rs), [authoring errors](../../rustd/crates/afd_authoring/src/error/tests.rs), [fleet errors](../../rustd/crates/afd_fleet/src/error/tests.rs) | Authoring 69 and fleet 131 library tests passed |
| One-attempt transport | A 503 returns after exactly one physical HTTP request | [transport tests](../../rustd/crates/afr_providers/src/transport/tests.rs) | Provider library passed, 44 tests |
| Supervisor continuation and native client | A summary receipt continues the same turn, accounts for both calls and carries fenced checkpoint requests | [authoring context](../../rustd/crates/afr_supervisor/src/authoring/tests/context.rs), [client tests](../../rustd/crates/afr_supervisor/src/test_util_tests.rs) | Supervisor library passed, 347 tests |
| Settled Billing reports | Empty totals, stable tie ordering, complete scoped detail pagination and unreadable-row refusal leave the ledger unchanged | [billing paging](../../rustd/crates/afd_api/tests/integration_billing_report_paging.rs) | All four cases passed in the canonical integration run |
| Enrolment and connector callbacks | Storage failure returns 503 without a token; missing platform setup refuses callback completion | [authorization.rs](../../rustd/crates/afd_api/tests/runner_plane/authorization.rs), [callback tests](../../rustd/crates/afd_api/tests/connector_callback_route.rs) | Both cases passed in the full unit rerun |
| Revoked runner retirement | Delete returns 204, removes events and makes subsequent reads/deletes return 404 | [runner retirement](../../rustd/crates/afd_api/tests/integration_operator_runner_retirement.rs) | Canonical integration passed |
| Late authoring notes | Changed or removed history withholds notes while known provider usage settles | [compaction progress](../../rustd/crates/afd_authoring/tests/draft/model_calls/progress/compaction.rs) | Canonical integration passed |
| Concurrent or failed checkpoint writes | A competing writer retains its notes and the usage settles; a storage fault rolls back receipt updates and leaves the dispatch pending | [publication checks](../../rustd/crates/afd_authoring/tests/draft/model_calls/progress/compaction/publication.rs) | Both real-storage cases passed |
| Summary response content | Reasoning remains private; a tool call mixed with valid notes refuses publication while retaining known usage | [content checks](../../rustd/crates/afd_authoring/tests/draft/model_calls/progress/compaction/content.rs) | Both real-storage cases passed |
| Lease renewal and input pricing | Unknown leases or runners cannot meter usage; overflowing input prices refuse before a reservation or physical send | [renewal checks](../../rustd/crates/afd_authoring/tests/draft/model_calls/renewal/authorization.rs), [funding checks](../../rustd/crates/afd_authoring/tests/draft/admission/platform/funding.rs) | Canonical integration passed |
| Checkpoint freshness during counting | A changed checkpoint invalidates a native count before paid intent and retains the competing notes | [counting checks](../../rustd/crates/afd_authoring/tests/draft/model_calls/counting.rs) | Canonical integration passed |
| Lease checkpoint publication | Invalid summary refuses without writing notes; repository diagnostics exclude connection data | [runner context](../../rustd/crates/agentsfleetd/tests/integration_runner_context.rs) | Canonical integration passed |
| Native continuation prefix | Changed authoritative history quotes the prior reply without replaying its private reasoning or executable tool items | [reasoning checks](../../rustd/crates/afd_authoring/tests/draft/model_calls/reasoning_context.rs) | Passed in the instrumented run |
| Measured-request admission | A changed body retains a typed provider refusal and performs no physical send | [admission check](../../rustd/crates/afd_authoring/src/model/provider/tests/admission.rs) | Authoring library passed, 70 tests |
| Context publication deadline | A saturated real pool bounds the hint and releases its cancelled waiter | [deadline check](../../rustd/crates/afd_authoring/src/model/progress/tests/context.rs) | Passed in the instrumented run |

The five focused library suites passed 733 tests in total, with zero failures.
Their log is `/private/tmp/pr784-coverage-repair-unit-packages.log`.

## Local verification

`make test-unit-all` passed 11,686 tests with zero failures and 16 command-line skips.
Its log is `/private/tmp/pr784-coverage-repair-unit-all-2.log`.

| Unit selection | Passed |
|---|---:|
| Rust runner | 1,327 |
| Rust daemon | 137 |
| Rust libraries | 3,312 |
| Local development launcher | 25 |
| Dashboard | 4,283 |
| Website | 133 |
| Command line | 1,950 |
| Design system | 519 |

All four configured package coverage gates passed.

`make test-integration-rustd` passed 1,405 regular tests and three exclusive tests, with zero failures.
Its log is `/private/tmp/pr784-coverage-repair-integration-3.log`.
The later native-prefix and context-deadline cases passed under instrumentation in `/private/tmp/pr784-coverage-repair-local-full-3.log`.

`make test-coverage-rustd` then passed all 6,019 instrumented unit and real-dependency tests, including the three exclusive tests.
Its log is `/private/tmp/pr784-coverage-repair-local-full-3.log`.

| Local macOS coverage | Measured result | Required floor |
|---|---|---|
| Global lines | 72,188 / 72,491 = 99.5820 percent | 99.5 percent |
| Changed lines | 3,138 / 3,146 = 99.7457 percent | 99 percent |
| Touched files | All 188 measured files meet their floor | 99 percent each |

The hosted Linux aggregate, including its kernel suite, must still grade the pushed revision.

`orly gate verify` passed, including full lint, Rust formatting, Clippy, feature checks and version agreement at 0.65.0.
Its log is `/private/tmp/pr784-coverage-repair-verify-6.log`.
Normal publication hooks and a fresh hosted coverage grade remain required.
Coverage thresholds, exclusions, acceptance assertions and retry policies are unchanged.

Two redundant runtime paths were removed: fallible serialization of a JSON value, and duplicate detail-kind validation before the handler's exhaustive selection.
The served detail route still refuses missing or unknown kinds before any reporting query.

A local `make test-coverage-rustd RUSTD_SHARD=substrate` attempt failed while linking because the disk filled.
It produced no usable coverage result.
`cargo clean --target-dir target/llvm-cov-target` removed that attempt's six GiB of generated files; source and ordinary build files were retained.

The first full unit attempt stopped when macOS rejected the generated `integration_admin` executable's signature, before its assertions ran.
The system log records an unrecoverable signature issue and a security-policy refusal.
Evidence is `/private/tmp/pr784-unit-macos-signing-evidence.log`.
The normal test command then rebuilt the executable and passed the full unit run.

The next instrumented attempt reached the callback test and received the expected 503 response.
Its assertion used a helper restricted to client errors, so that helper refused the server error.
The test now reads the JSON error code directly and passed in the full unit rerun.

## Owner-directed workflow and documentation state

Ten-fleet acceptance is available only through manual dispatch for Indy after merge and deployment verification.
The deployment summary reports its explicit deferral.
The original ten-fleet test, concurrency and zero-retry settings are unchanged.

The spec moved to `done/` at Indy's request and retains its incomplete evidence rows.

Docs PR [#228](https://github.com/agentsfleet/docs/pull/228) merged at `d12ed662ddeecc18a954f2a91b7cd2a0288e2d60`.
Commit `a1cd28e` corrected both login exit-code references; the [Greptile thread](https://github.com/agentsfleet/docs/pull/228#discussion_r4366806166) has a fix reply and is resolved.
The docs checkout is clean on `main` and matches `origin/main`; its four merged local branches were pruned.

Four unmerged Billing/context documentation edits remain preserved in stash `bbcb780238b852fc62d6450e7aff4736b68ea4d3`, named `pr228-pending-companion-docs-before-main`.

PR #784 remains open for Indy; merge and deployment are not authorized.
The exhausted implementation and documentation reviews, manual login check and live-provider accuracy limitations remain recorded in the spec.
