---
type: explanation
audience: contributor
verified: 2026-10-22
product_version: 0.65.0
executable: false
---

# Admissions queue verification

## What it is

This record covers the change that makes `fleet.admissions` the sole work queue in `agentsfleet`.
The comparison revision is `0fa9e55356571d57df69cd3a13fb3b90930b0e49`.

## Why it exists

The removed inbound stream and readiness index copied work already saved in Postgres.
The replacement must preserve ordering, lease fencing, report billing, gate recovery and retry identity.

## How it behaves

### Required behavior and tests

| Changed behavior | Proof |
| --- | --- |
| Admission remains accepted when its notification fails | `afd_admission/tests/receipts/confirmation.rs` |
| Retrying an admission retains its event identity | `afd_admission/tests/receipts/confirmation.rs` and `receipts/staging.rs` |
| Save and run retains one fleet and initial event | `afd_authoring/tests/draft/creation/contention.rs` |
| Missing, mismatched or unavailable confirmation retains the saved initial event for retry | `afd_authoring/tests/draft/creation/confirmation.rs` |
| A stopped or failed authoring message permits the next message | `afd_authoring/tests/draft/admission/recovery.rs` and `admission/failure.rs`, retained from Pull Request 779 |
| Competing replicas claim one admission once | `afd_fleet/tests/integration_ledger_queue/contention.rs` |
| A locked or parked oldest admission holds later work | `afd_fleet/tests/integration_ledger_queue.rs` |
| A superseded report writes no billing, history or settlement | `afd_fleet/tests/integration_ledger_queue/settlement.rs` |
| Report failure rolls back admission settlement and permits retry | `afd_fleet/tests/integration_ledger_queue/settlement.rs` |
| An expired slot requalifies its admission without a repair job | `afd_fleet/tests/integration_ledger_queue/reclaim.rs` |
| A gate answer during a park lock wait prevents a stale park | `afd_fleet/tests/integration_ledger_queue/parking.rs` |
| A superseded park cannot release or park the current holder | `afd_fleet/tests/integration_ledger_queue/parking.rs` |
| Failed approval parking releases the claim, preserves the original error and retries the same event with a newer fence | `afd_fleet/tests/integration_lease_gates/faults/parking.rs` |
| Stale terminal settlement leaves the current admission claim intact | `afd_fleet/tests/integration_terminal_redelivery.rs` |
| An unreadable redelivery preserves an existing terminal ending | `afd_fleet/tests/integration_lease_gates/faults.rs` |
| An answer or expiry waits for an uncommitted park, then clears it | `afd_fleet/tests/integration_ledger_queue/parking_commit.rs` |
| First delivery commits while an admission holds the fleet budget lock | `afd_fleet/tests/integration_ledger_queue/delivery.rs` |
| Delivery-stamp failure rolls back history and counters; retry delivers once | `afd_fleet/tests/integration_ledger_queue/delivery.rs` |
| An optional counter-read failure leaves delivery committed and retry adds no event | `afd_fleet/tests/integration_ledger_queue/delivery.rs` |
| Claim uses both prior-event index keys for fresh and reclaimed heads | `afd_fleet/tests/integration_lease_plan/claim.rs` |
| An empty ledger skips scans across 20,001 active fleets | `afd_fleet/tests/integration_lease_plan/idle.rs` |
| Resolved and expired gates clear the durable park | `afd_fleet/tests/integration_lease_gates/stops.rs` and `afd_approval/tests/integration_inbox.rs` |
| A full admission, lease and report cycle creates no inbound queue keys | `test_cycle_leaves_no_dragonfly_queue_keys_and_populates_rows` |
| Queued steers appear once in ordered history | `afd_events/tests/integration_admission_history.rs` |
| Public history uses one statement snapshot per page | `afd_events/tests/queued/snapshot.rs` |
| Runtime roles can use the migrated queue columns | `afd_db/tests/integration_admissions_queue.rs` |
| Notification lag and reconnect gaps are counted without losing the next valid hint | `afd_dragonfly/src/doorbell/tests.rs` |

### Failure boundaries

Each row names the operation, failure location, residual state and retry proof.
Database faults use private migrated databases; they cannot change another test's tables.

| Operation and boundary | Injection | Required residual state | Retry |
| --- | --- | --- | --- |
| Admit: database acquisition | Unreachable database fixture | No admission accepted; database error preserved | Caller can resubmit |
| Admit: notification after database commit | Unreachable Dragonfly | One accepted admission; no inbound stream required | Same key returns the same event |
| Claim: existing slot lock | Held transaction on the slot | No changed admission fence; poll returns without waiting | Claim succeeds after release |
| Claim: oldest admission lock | Held transaction on the oldest row | No later admission exposed; provisional slot rolled back | Original head leases after release |
| Claim: row decoding after updates | Infinite timestamp accepted by Postgres but refused by the application | Neither slot nor admission fence commits | Repaired row claims with fence one |
| Report: admission settlement inside the transaction | Trigger raises during `settled_at` update | No billing, history, lease ending or admission settlement commits | One settlement; repeat returns the stored outcome |
| Report: admission fence changed | Change the admission fence before reporting | No report side effects; admission remains unsettled | Current fence can settle |
| Park: answer commits while fleet slot lock is held | Observe the blocked backend before answering | Delayed park sees the committed answer | Same admission leases on the next poll |
| Park: admission stamp or slot release fails | Private trigger raises at either write | Original database error survives; transaction rolls back and fenced cleanup frees the slot | Approval permits the same event with a newer fence |
| Gate answer or expiry: uncommitted park write | Private trigger pauses the park after its write | Resolver waits for the admission and clears its park | Same head leases on the next poll |
| Delivery: narrative written before delivery stamp fails | Private trigger raises on `delivered_at` update | No narrative, stamp or counter change commits | Retry delivers once; repeat adds nothing |
| Delivery: optional counter read fails | Revoke the private database runtime role's counter read grant | Narrative, stamp and triggered counter increment stay committed; returned figures are absent | Retry is a repeat with no extra event |
| Gate answer: notification after durable unpark | Unreachable Dragonfly | Gate answer and cleared park survive | Postgres discovery finds the work |

### Concurrency

The claim race releases 100 application clients from one barrier across two independent database pools.
The pools retain their production bounds; this does not claim 100 database connections.

A private trigger holds claims at an advisory lock before inserting a fleet slot.
The observer requires at least four blocked claim statements before releasing them.
Exactly one caller must receive the event, and both stored fences must equal one.

This counter catches a lock that serializes every claim inside the application.
The claim test passed with this counter in `/tmp/m226-ledger-races-1.log`.
Three consecutive runs passed with the parallel counter and one winner in `/tmp/m226-concurrency-repeat.log`.

### Repository checks

Only the repository's declared `make` targets establish repository results.
Package commands establish the narrower behavior named above.

| Check | Current evidence |
| --- | --- |
| Comparison revision, full unit suite | Failed: 10,668 passed, one failed, 1,390 skipped/ignored; `/tmp/m226-baseline-unit-4.log` |
| Comparison revision, full integration suite | Failed after 608 passed: one cron deadline assertion failed; `/tmp/m226-baseline-integration-2.log` |
| Implementation before the failed-park repair, full unit suite | `0.65.0`: 11,114 passed, 1,349 skipped/ignored; `/tmp/m226-unit-all-65-2.log`. All four TypeScript package coverage gates passed at 100% |
| Implementation before the failed-park repair, full integration suite | `0.65.0`: 1,328 passed, zero failures (1,325 normal and three exclusive); `/tmp/m226-integration-all-65.log` |
| Focused review regressions | Ledger 12 and lease plans 4 passed after the metering repair; `/tmp/m226-ledger-green-4.log`, `/tmp/m226-plans-green-4.log`. Queued history 6 passed before that unrelated repair; `/tmp/m226-history-green-1.log` |
| Lint | Full `make lint-all` passed; `/tmp/m226-lint-all-65-3.log`, including both shipping binaries without test features and the production-statement test import |
| Conformance | `make harness-verify` passed for the staged release documentation; `/tmp/m226-harness-65-closure.log`. Runtime source commits also passed their conformance hooks |
| Rust coverage | After the failed-park repair: 5,779 tests passed, 99.5522% overall, 401/401 changed lines, all 72 measured changed files ≥99%; `/tmp/m226-coverage-park-fix-retry.log` |
| Version | `make check-version` passed: all versions match `0.65.0` |
| OpenAPI | Regenerated from the all-feature `0.65.0` daemon and byte-matched `public/openapi.json`; only the fleet-create description changes |
| Secrets | Full-history `gitleaks detect --redact --no-banner` passed: 7,430 commits, no leaks found; `/tmp/m226-gitleaks-full-65.log` |
| Review | Five initial issues, the optional-counter transaction failure and the metering identity cast are fixed. Current native and specialist source reads found no runtime defect. Archived exploratory checks remain inconclusive; the final clean-candidate gate is recorded in Pull Request Session Notes |

The baseline uses an archive of the exact comparison revision and an independent Cargo target directory.
Its integration services use a separate Compose project and ports.
The baseline unit command stopped at `cli/test/dev-dashboard.test.ts:108`: its 100 ms child-process deadline elapsed before the fixture wrote its startup trace.

The design-system target was run separately afterward: 498 tests passed in 62 files (`/tmp/m226-baseline-design-system.log`).
Those supplemental results are not added to the canonical baseline total.

The baseline integration command stopped at `afd_cron/tests/integration_expiry_lock.rs:101`, whose 500 ms lease expired before the guard queued.
Both historical runs remain failed; they do not permit failures in the implementation's final lanes.

The checks include `main` at `124c2bce303c95533745312c1ec725092bb604cb`, retaining Pull Request 780's authoring claim and renewal fixes.
Pull Request Session Notes retain the final uncached `orly gate pr` output for the clean, pushed candidate.

The latest Rust coverage run measures `589a2eaefc9e88e0274c3f4f1ca47454bbcf0881` against that integrated base.
It covers 68,475 of 68,783 instrumented lines and every one of the 401 measured changed lines.
The lowest whole-file result is 99.2308% across the 72 measured changed files.

The [hosted coverage check](https://github.com/agentsfleet/agentsfleet/actions/runs/39707230551/job/119593318416) covers the same source tree, including Linux-only paths.
It reports 70,629 of 70,924 lines covered, 401/401 changed lines, and all 72 measured changed files above 99%.
The workflow retains the four raw coverage reports as downloadable files.

The run adds no exclusions and keeps the repository's existing test-helper and benchmark exclusions.
Test targets and uninstrumented lines do not enter those source-coverage denominators.
Linux-only runner checks use the hosted Continuous Integration lane.

The failed-park regression tests first failed both cases, then passed both after fenced cleanup was added.
Related tests passed 30 cases; a later affected-suite recheck passed 69.
The repair preserves the original typed database error and adds no query to successful claim or park paths.

The first full coverage attempt failed 12 tests with recorded differences between host and database timestamps.
After those clocks agreed, the unchanged affected tests and full coverage run passed.
The cause of the clock disagreement remains unproven; no timeout, assertion or coverage floor changed.

The source review read 190 source paths and summarized 240 earlier test and fixture paths.
The bounded failed-park review also read the two-case regression fixture in full.
One specialist reader assessed performance, migrations and failure isolation; another completed the native adversarial read.
At `589a2ea`, the gate helper emitted 43 of 554 eligible hunks; all 43 were inspected, leaving 511 outside that bounded review.

The archived exploratory report remains inconclusive because several recorded commands name superseded binaries.
Later native tests do not rewrite that historical result.
After those limits were explained, the owner instructed: “Well run the test and send the PR”.

The baseline Rust unit selection passed 4,669 tests: runner 1,314, daemon 137 and libraries 3,218.
Python passed 25; app passed 3,889; website passed 133; command-line interface passed 1,952 with one failure and 16 skips.
Rust reported 1,374 ignored tests, reserved for integration.

Compare package selections individually: the baseline canonical command never reached the design-system package.
Do not calculate an integration delta from a baseline that stopped before later packages.

The abort fixture previously allowed five seconds for each failed database acquire.
That prevented ten failures from reaching the monitor within its 30-second run.
The existing 50 ms unreachable-database fixture preserves the monitor assertions and passes in 1.06 seconds.

The red and green logs are `/tmp/m226-unit-all-6.log` and `/tmp/m226-abort-fixture.log`.
Ten consecutive abort-fixture runs passed in `/tmp/m226-abort-repeat.log`.

### Measurements

Earlier stream and ledger prototypes each drained 200 events with no failures.
Those runs preceded the production settlement change; their timings do not describe the final queue.

The final `0.65.0` queue path completed all 15 canonical samples: three each at 20, 40, 80, 160 and 200 events.
Each had zero failures, zero unsettled admissions, and equal created/swept fixture counts.
Together they completed 1,500 drain events and swept 9,240 fixture records.

At 200 events the medians were 52.725 statements, 22.535 commits and 3.045 Dragonfly calls per completed event.
The median completed-event rate was 142.37/s; lease and report p95 were 43.679 ms and 33.695 ms.
Idle discovery cost one statement per poll.

The tenfold population increase required 8.28 times the drain statements, a client-work measurement rather than a server CPU bound.
The [scaling table](../architecture/scaling.md#admissions-ledger-measurements) retains all five population medians and the benchmark command.
The measured source was `ab66bb2ec992c87f0158470672a33bab30a58f38`, including the integrated base and coverage repairs.

Runtime sources stayed unchanged throughout; documentation edits were pending.
The later failed-park repair changes error cleanup; no new latency benchmark was run for that repair.

The empty-ledger discovery probe seeded 20,001 active fleets in a private migrated database.
Before the precheck, discovery visited all fleets and made 20,001 admission probes; afterward, no fleet scan ran.
The single query-plan samples took 19.702 ms and 0.079 ms respectively.

The probe command was `fleet_suite --ignored integration_lease_plan --test-threads=1 --nocapture`.
This measures an empty queue, with no claim about chat latency or nonempty discovery cost.

The prepared metering plan compares native admission identifiers after casting the event identifier to a Universally Unique Identifier (UUID).
Before the repair, the settlement scan at 1,250 admissions rejected 1,249 rows (`/tmp/m226-meter-red-3.log`).
The existing lease-plan test now also runs renewal and settlement at 1,250, 2,500, 5,000 and 10,000 admissions.

Each tested admission lookup must use the primary key and return at most one row; settlement state is checked before rollback.
The isolated fixture uses its owner connection to collect planner statistics; API-role behavior remains covered separately by the ledger suite.
This proves bounded lookup work in the fixture, without a chat-latency or general throughput claim.

The fresh canonical stream comparison completed no drain events and recorded 17 failures while exiting successfully.
Its zero ratios are invalid for a completed-work comparison; no new throughput improvement is claimed.
The new benchmark refuses incomplete drains.

`test_ledger_claim_lane_reports_round_trips` passed its real 200-event drain and injected event-write refusal in 35.45 seconds (`/tmp/m226-production-bench-proof.log`).
The refusal occurs after staging; prefixed fixtures are swept on both outcomes.

Two independent hubs received the same 24 published fleet identifiers across the sharded channels and ignored malformed payloads.
Every channel returned to zero subscribers after its owners were dropped (`/tmp/m226-doorbell-live-3.log`).

### HTTP resource ownership

The real HTTP saving tests passed all 24 cases under macOS `leaks --noContent --atExit`.
The tool reported zero leaks and zero leaked bytes; `/tmp/m226-http-resources-1.log` retains its output.
This run preceded the base integration and used the full router, middleware, Postgres and Dragonfly.

The checks cover retries, lost notifications, admission refusal and revision cleanup.
They do not prove allocator-failure recovery or long-running reachable-heap bounds.

### Mutation and cutover proofs

The notification module's mutation audit caught all ten compilable mutants; one could not compile because `Uuid7` has no `Default` implementation.
No mutant survived or timed out.
The audit removed ring, take, listen, answer and publish bodies, changed the capacity comparison and shard arithmetic, and replaced the channel name.

The audit used `cargo-mutants 27.1.0` on `afd_dragonfly/src/doorbell/mod.rs`; `/tmp/m226-mutations-3.log` retains its output.
The generated-mutant build capped lints so newly unused names could not substitute for behavioral failures; shipping lint settings were unchanged.
This proves the notification tests reject those defects, without claiming SQL mutation coverage.

The owned development reset was exercised with a seeded draft and fleet plus an inbound stream and readiness key.
The canonical `make test-integration-rustd` reset removed both rows and both keys before its test readers started.
All three queue columns existed afterward, and all four approved obsolete columns were absent.

The setup and verification logs are `/tmp/m226-cutover-seed.log` and `/tmp/m226-cutover-verify.log`.
This checks the disposable local cutover, not a deployed upgrade or populated-history backfill.

## Limits

Each idle poll now makes one Postgres statement.
Lost notifications cannot strand work, but an idle deployment spends that query per poll.

An empty ledger skips fleet scans.
When unsettled admissions exist, discovery probes the oldest one of each eligible active fleet.
The partial index bounds each fleet's head lookup; nonempty discovery cost still grows with eligible active fleet count.

The per-fleet admission budget is serialized by the fleet row.
The deployment budget is a sampled soft ceiling, so independent replicas can temporarily exceed it.

The approved development cutover stops old readers and resets disposable drafts and fleets.
Mixed stream and ledger readers are unsupported.
No history backfill is included.

## Related pages

- [Queue design](../architecture/datastore_scaling.md#the-admissions-ledger-is-the-queue)
- [Lease flow](../architecture/lease_flow.md)
- [Concurrency rules](../architecture/concurrency.md)
