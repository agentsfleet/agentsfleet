---
type: reference
audience: contributor
verified: 2026-11-18
product_version: 0.78.0
executable: false
---

# M247 review and verification ledger

This ledger records the baseline, review dispositions, benchmark evidence, and verification still due for platform runners. The implemented architecture lives in the [recovery guide](../../architecture/runner_checkpoint_recovery.md).

Base: `97224ed7432c9dd124ea8797d4324156b7022c07`; pre-refactor implementation: `86f481126c1aa053c34ef3bdaaa6f07baa1c74ab` plus candidate prefix `22ac73ab741b`.
The frozen production baseline is `d4823563e80829b97589846a1c7997964fdf0274`. The optimized comparison used the same fixture against the refactor commit `976883d00`.

## Completed first-pass repairs

| Finding | Evidence / disposition |
|---|---|
| page_limit | FIXED before the full pre-refactor unit run; Catalog uses 1 MiB task pages for 512 KiB turn wire |
| restore_download_pin | FIXED before the full pre-refactor unit run; Four sequential GETs can exceed 120s pin |
| restore_directory_pin | FIXED before the full pre-refactor unit run; Directory pass omits renewals |
| local_capture_classes | FIXED before the full pre-refactor unit run; Local inventory bounds and Busy become fatal Config |
| commit_ack | FIXED before the full pre-refactor unit run; Exhausted acknowledgement retries leave head stale forever |
| page_ordinal | FIXED before the full pre-refactor unit run; Wrong page_number can publish unrestorable head |
| identity_retention | FIXED before the full pre-refactor unit run; One permanent wrapped content key per file event |
| restart_empty_row | FIXED before the full pre-refactor unit run; System restart without output loses assistant row marker |
| restart_a11y | FIXED before the full pre-refactor unit run; Restart not announced to screen reader |
| fleet_issue_hold_tests | FIXED before the full pre-refactor unit run; No real fleet issue hold census/rollback tests |
| turn_maintenance_tests | FIXED before the full pre-refactor unit run; No turn refs/pins/retention/quota maintenance proof |
| checkpoint_lock_tests | FIXED before the full pre-refactor unit run; No expiry during lock wait or Commit/Read/Adopt authority negatives |
| stale_frame_test | FIXED before the full pre-refactor unit run; Malformed stale payloads bypass reducers regardless of fencing |
| commit_lock_expiry | FIXED before the full pre-refactor unit run; Fresh plan and pin custody must be rechecked after manifest or fold lock waits |
| server_link_graph | FIXED before the full pre-refactor unit run; Server accepted cyclic or escaping cross-page link graphs |
| immutable_cleanup_permissions | FIXED before the full pre-refactor unit run; Cleanup used FOR UPDATE on immutable operation receipts without UPDATE privilege |

Focused evidence includes 19 real turn-checkpoint integration tests, two real capacity tests, migration checks and the API-only two-process crash/restore drill.
Runner unit tests include lost Commit acknowledgements, page limits, capture bounds and parallel authenticated file restore.
Raw local logs retain the failures before their repairs; final CI must verify the committed refactor.

## Second-pass findings and dispositions

The table preserves the 19 findings from the second pass. The verification below records their current evidence.

| # | Severity / source | Location | Finding | Disposition at `976883d00` |
|---|---|---|---|---|
| F1 | CRITICAL / testing | `rustd/crates/runner/afr_supervisor/src/lease_loop/turn_recovery.rs:11` | Missing ordinary Files offer through production supervisor/model entry, warm-hold retirement, thaw, and failure-before-model proof | Open. The cross-file unit and native database proofs passed. The complete supervisor and model-entry drill remains due. |
| F2 | CRITICAL / testing | `rustd/crates/daemon/afd_fleet/src/lease/turn_checkpoint/guard.rs:60` | Missing connection-bound Begin/Commit/Read/Adopt mutation-time supersession and credential-rotation races | Open. The refactor implements shared host authority. Mutation-time supersession and rotation races still need dedicated proof. |
| F3 | CRITICAL / testing | `rustd/crates/daemon/afd_runner/src/heartbeat/cordon.rs:33` | Missing degraded/invalid/fallback and assignment/connection-race automatic-lift proofs | Open. Healthy-return and legacy-heartbeat proofs exist. The fallback case was added for the next runner integration run. |
| F4 | CRITICAL / testing | `rustd/crates/daemon/afd_fleet/src/lease/deliver/issue.rs:52` | Missing real NATS issued-reclaim versus fresh/refused/reserved publication proof | Open. The process fixture sees restart and recovery. The four-way publication proof still needs a dedicated NATS subscriber assertion. |
| F5 | CRITICAL / state-machines | `rustd/crates/daemon/afd_fleet/src/lease/turn_checkpoint/pins.rs:267` | untested cell: ApiOnly × Abandon → retain the abandoned pending plan, release its writer pin and preserve the ready head | Verified on the real fleet suite for API-only state. |
| F6 | CRITICAL / state-machines | `rustd/crates/daemon/afd_fleet/src/lease/turn_checkpoint/maintenance.rs:98` | untested cell: ApiOnly × CollectTerminal → erase terminal private generations, recovery head and allocation receipts after retention without erasing public source state | Verified on the real fleet suite for API-only state. |
| F7 | CRITICAL / state-machines | `rustd/crates/daemon/afd_fleet/src/task/checkpoint/transfer/signing.rs:81` | untested cell: Incremental × VerifyUpload → authenticate and persist a newly uploaded incremental object and account its unique stored quota | Open. The existing SDK test verifies a base upload. It needs a newly uploaded object on an incremental plan. |
| F8 | CRITICAL / state-machines | `rustd/crates/daemon/afd_fleet/src/task/checkpoint/transfer/signing.rs:141` | untested cell: Incremental × Restore → restore the exact committed incremental manifest/object references under reader custody while preserving its base | Open. The test needs an exact incremental restore while retaining its base. |
| F9 | CRITICAL / state-machines | `rustd/crates/daemon/afd_fleet/src/task/checkpoint/maintenance/mod.rs:75` | untested cell: Incremental × CollectOrphan → erase expired incremental private keys/references and release its physical quota once without retiring a live base | Open. The test needs to collect an incremental orphan while retaining a live base and proving quota correction. |
| F10 | CRITICAL / state-machines | `rustd/crates/daemon/afd_fleet/src/lease/turn_checkpoint/pins.rs:287` | untested cell: writer × PinRelease → release the exact writer pin while preserving its pending plan and head, and refuse later protected operations | Verified on the real fleet suite for a pending writer. |
| F11 | CRITICAL / state-machines | `rustd/crates/runner/afr_supervisor/src/recovery/turn/publication.rs:47` | untested cell: Commit × ReconcileRefusal → clear indeterminate publication only after exact abandonment, or stop when abandonment cannot settle it | Verified in the runner unit suite. A refused replay cannot start a new capture until its exact pending plan is abandoned. |
| F12 | CRITICAL / native | `rustd/crates/daemon/afd_fleet/src/lease/sql/commit_report.sql:181` | A report can decide a hold before the audit insert's foreign-key check takes a host lock. Exercise expiry and competing issue in that interval, then serialize hold-producing reports from before their decision through commit | Fixed. The real delayed-report regression failed at the baseline and passed after the lock change. |
| F13 | INFORMATIONAL / data-migration | `schema/20261117154627_runner_turn_checkpoints.sql:6` | Missing fleet_id-leading index for fleet purge | Implemented in the checkpoint schema. The focused migrations and retention suites passed. |
| F14 | INFORMATIONAL / parent | `schema/20261117154626_runner_turn_recovery.sql:4` | Missing head_turn_checkpoint_id-leading index for referenced checkpoint deletion | Implemented in the recovery schema. The focused migrations and retention suites passed. |
| F15 | INFORMATIONAL / api-contract | `rustd/crates/daemon/afd_api_operator/src/handler/operator/events.rs:31` | Allowed event_type query description omits runner_uncordoned; regenerate OpenAPI | Fixed and regenerated. The operator filter documents runner_uncordoned. |
| F16 | INFORMATIONAL / design | `ui/packages/app/tests/e2e/fixtures/fleet-stream-browser-probe.ts:21` | New reply status adds a second output; transport-status assertions need a dedicated selector | Fixed. Three real Chromium transport and restart tests passed. |
| F17 | INFORMATIONAL / performance | `rustd/crates/runner/afr_supervisor/src/recovery/turn/capture.rs:149` | Ordinary upload serializes singleton native calls and physical transfers while frozen | Fixed. A four-object HTTPS test proved one authorization and one verification request. |
| F18 | INFORMATIONAL / performance | `rustd/crates/daemon/afd_fleet/src/lease/turn_checkpoint/maintenance.rs:35` | First owner lock timeout aborts later maintenance and repeats oldest candidate without cursor | Fixed. The real locked-owner regression failed at the baseline and passed after the cursor change. |
| F19 | INFORMATIONAL / performance | `rustd/crates/daemon/afd_fleet/src/lease/turn_checkpoint/sql/maintenance_candidates.sql:2` | LIMIT bounds returned owners but not the joined candidate scan; empty heads are created for uncaptured offers | Implemented. The owner page seeks before inspecting per-owner work. Quantitative maximum-manifest results remain due. |

The state-machine artifact has nine machines, 90 cells, 83 mapped to tests and seven uncovered.
The installed matrix checker passed its completeness/path checks; mapping to a test does not prove that test ran or satisfies every other specification dimension.

## Added burst and quality requirements

| ID | Area | Required work |
|---|---|---|
| S1 | Bounded recovery drain | Use recovery-only bounded round-robin. Keep ordinary ring budgets and avoid retrying a locked prefix of 64 candidates while fleet 65 can run |
| S2 | Shared resource ownership | Use runner-wide byte/I/O/blocking budgets and reusable transport, with family-specific authority |
| S3 | Cross-file batches | Batch physical transfers across file boundaries and remove duplicate plaintext buffers |
| S4 | Short custody and bulk deadlines | Hold a shared host lock through a hold-producing report, conflict with exclusive issue, and preserve fresh-clock/connection checks. Remove empty quota writes and bound verification deadlines |
| S5 | Measurement and coverage | 1/10/100/500 interrupted fleets; dist comparisons; >99% per production file and patch lines |

The Astra direction review identified two benchmark fixture limitations. Direct `Session::offer` bypasses scheduler registration and the replica wake path. The default seed gives each fleet a separate workspace and hides contention on shared quota. R13 requires registered, already-idle replacement hosts and both workspace distributions. The measured baseline uses those registered sessions. The refactor checks below exercise F12 and S1 on PostgreSQL 19rc2.

See [platform runner checkpoint recovery](../../architecture/runner_checkpoint_recovery.md) for the implemented design, limits, operator walkthrough and source map.

## Pre-refactor verification

| Command / scope | Actual outcome |
|---|---|
| `make lint-all` | PASS; Rust formatting, Clippy, unused-dependency checks and full repository lint |
| `make test-unit-all` | PASS; Rust 1,819 runner + 172 daemon + 3,711 daemon-library tests |
| App unit/coverage | 4,659 tests / 491 files; 100% statements, branches, functions and lines across 529 measured source files |
| CLI unit/coverage | 2,377 passed, 22 skipped; 100% function and line coverage |
| Website / design-system unit/coverage | 133 / 516 tests passed; reported coverage gates passed |
| Controlled process crash drill | PASS; 111.04 seconds for the entire process/liveness scenario, not checkpoint latency |
| Rust per-file instrumented coverage | NOT RUN on the refactor |
| Full final integration and PR gate | NOT RUN on the refactor |
| Optimized API-only baseline | Seven measured cases across 1, 10, 100 and 500 fleets. The shared 500-fleet run reached the model for 12 fleets by the cutoff. The distributed 500-fleet source preparation timed out twice. [Raw results](../../../bench/baselines/lease-recovery-api.rig.json) |
| File-backed performance | Pending a matched capture and restore benchmark |
| Human smoke / paid acceptance | NOT RUN |

## Focused refactor verification

| Scope | Verified outcome |
|---|---|
| Real focused lease and checkpoint integration | 27 passed on PostgreSQL 19rc2 and NATS. A hold-producing report serializes with competing issue. Restore authorization proceeds with a locked quota row. A read of a fleet with no ready checkpoint writes no recovery metadata. Retention progresses past a locked owner. The four cases reproduced their failures on the baseline. |
| Recovery page | One real recovery pass proceeds past 64 locked slots, leases fleet 65 and retries the older page after wraparound. |
| Agent, supervisor, gateway and wake-queue unit tests | 833 passed and one ignored. The scheduler tests cover 1, 10, 100 and 500 claims, a slow session and a concurrent ordinary ring. Runner tests cover required admission and optional skip, shared four-object uploads, cross-file restore, cancellation and exact pending abandonment. |
| Chromium SSE and attempt display | 3 passed. Real browser transport covers heartbeat renewal after quiet EOF, replacement of a silent connection, and one announcement per attempt. |
| Generated operator API | Regenerated from the daemon. The runner event filter adds `runner_uncordoned` to its documented accepted values. |

An earlier unit attempt failed the old Busy classification assertion, which was corrected to the intended typed unavailable error.
A later attempt failed the existing expired-deadline HTTP/3 transport test; the full retry passed, and the initial red log remains.
No claim of an unrelated baseline defect is made without equivalent reproduction.

## Performance and coverage acceptance

Compare the frozen pre-refactor revision with the final revision on the same owned local infrastructure.
Use 1, 10, 100 and 500 interrupted fleets, declare compatible source/replacement worker counts, and use multiple failed hosts where the 64-worker limit requires them.
Separate API-only roots, four small files, large multi-chunk files and metadata-heavy manifests.

**Benchmark correction from the Astra review.** The existing lease bench uses `afd_gateway::test_util::Session::offer`, which calls `services.offer()` directly. Its session constructor does not register the host with the scheduler, and its window listens on a separate wake instance. Starting `Gateway::run` alongside that fixture would still miss the recovery path. Use real registered native sessions with the production scheduler, shared wake and normal credit reporting. Keep replacement hosts connected and settled at idle before killing the source hosts. Count ordinary rings, Ready credit increases and fallback probes so their causes can be distinguished. Capture the eligibility deadline for each lease. Hold resumed model responses at a barrier for the dispatch-only scenario so settlement does not create fresh credits. Also run the scenario through settlement to measure the user-visible result.

The existing lease seed assigns a distinct workspace and tenant per fleet. Add a shared-workspace case to expose quota contention and a distributed-workspace case for comparison. Fit file sizes to the 50-GiB workspace quota and available disk on both revisions. Exercise large multi-chunk and high-entry-count cases with explicitly stated populations. Never extrapolate those timings to a larger population. Record the number of interrupted, eligible, issued, restored, settled, failed and outstanding fleets at the same fixed observation horizon. Report a cohort percentile only when that rank was observed. Retain failures and right-censored results. Repeat matched runs and record per-run spread. Include ordinary single-ring and idle query costs as regression controls.

Baseline production commit `d4823563e80829b97589846a1c7997964fdf0274`. The verified installed PostgreSQL 19rc2 Alpine image index is `sha256:7e3904251ae6f1e80c756b0ea12fbac512d8b9512de0047cdcbe4c6a69dff60b` on linux/amd64. Pin the image in the measurement record along with the benchmark harness hash, runner/daemon binary hashes, fixture seed and concurrency. Run the baseline before any production code edits. Reuse the same harness for the final comparison.

Record cold/warm storage, unique/reused chunks, concurrency limits, data size, injected storage delays and the actual build profile.
Published production-performance comparisons use `dist`; debug/instrumented tests establish behavior and must be labelled accordingly.
Use controlled model transport for repeatability and identify it in every result.

### Optimized fixture smoke, 2026-11-18

The first successful `make bench-lease PROFILE=rig BENCH_SHAPE=recovery BENCH_FLEETS=1 BENCH_WORKSPACES=shared` used `dist` at `d4823563e8` with the fixture fingerprint `13f3b7288385869205e5d37bb41d9cddfc81fe8cac9c901367679208849034d7`. The API-only source committed one root before its process was killed. The replacement issued fence 2 at **4,759 ms**, called the model with the recovered prefix at **5,364 ms**, and settled at **5,444 ms**, all measured from the old lease's expiry.

The first 10-fleet attempt issued all 10 source leases and timed out before creating checkpoints. The fake model shared the runner transport's four-stream connection bound. The comparison requires a fresh one-fleet run alongside the larger cohorts on the regular TLS model fixture. The single-fleet numbers above record the earlier fixture smoke.

### Measured API-only baseline

`make bench-lease PROFILE=rig BENCH_SHAPE=recovery` ran with the regular TLS fixture at `40a579688` against the unchanged production revision `d4823563e8`. Each successful cohort committed its source roots before the failed hosts were killed. The observation window ended 60 seconds after the latest source lease expired. All times below start at each fleet's own eligibility deadline. [Raw results and fingerprints](../../../bench/baselines/lease-recovery-api.rig.json)

| Workspaces | Fleets | Issued by cutoff | Reached model by cutoff | Settled at collection | First observed model | Last observed model |
|---|---:|---:|---:|---:|---:|---:|
| Shared | 1 | 1 | 1 | 1 | 5.479 s | 5.479 s |
| Shared | 10 | 10 | 10 | 10 | 5.330 s | 49.816 s |
| Shared | 100 | 12 | 12 | 12 | 4.481 s | 58.983 s |
| Shared | 500 | 12 | 12 | 12 | 1.901 s | 56.022 s |
| Distributed | 1 | 1 | 1 | 1 | 6.132 s | 6.132 s |
| Distributed | 10 | 10 | 10 | 10 | 5.186 s | 49.653 s |
| Distributed | 100 | 12 | 12 | 12 | 3.952 s | 57.813 s |
| Distributed | 500 | — | — | — | — | — |

For the shared one-fleet case at `d4823563e8`, the observed P50 was **5.479 s**, or **about 5.5 s**, from one sample. The source committed an API-only root containing its accepted model answer and a folded follow-up. The fixture killed the source process after the idle replacement connected.

The timer starts at the source lease's eligibility deadline, the later of the lease and slot expiries. The replacement received fence 2 at **+4.923 s**. Its first model request contained the saved answer and follow-up exactly once at **+5.479 s**. The fixture returned the next answer, and the replacement settled at **+5.545 s**.

The 100- and 500-fleet trials with 12 completions leave 88 and 488 fleets outstanding respectively. Their cohort P50, P95 and P99 remain unobserved. The distributed 500-fleet source preparation timed out twice. Both attempts issued a source lease for each of 500 fleets and produced zero ready checkpoint heads before timing out. The raw record preserves those two attempts and their counts.


### Measured API-only comparison

Both builds used the same optimized profile, model fixture and owned PostgreSQL and NATS rig. Times start at each source lease's expiry and end at the first model request containing the saved conversation and folded follow-up. The observation window ended 60 seconds after the final old lease expired. [Refactor samples and fingerprints](../../../bench/baselines/lease-recovery-api.refactor.rig.json)

| Workspace layout | Fleet count | Baseline resumed | Refactor resumed | Baseline P50 | Refactor P50 | Baseline last observed | Refactor last observed |
|---|---:|---:|---:|---:|---:|---:|---:|
| Shared | 1 | 1/1 | 1/1 | 5.479 s | 5.267 s | 5.479 s | 5.267 s |
| Shared | 10 | 10/10 | 10/10 | 24.765 s | 5.285 s | 49.816 s | 5.391 s |
| Shared | 100 | 12/100 | Source timeout | — | — | 58.983 s | — |
| Shared | 500 | 12/500 | Source timeout | — | — | 56.022 s | — |
| Distributed | 1 | 1/1 | 1/1 | 6.132 s | 5.551 s | 6.132 s | 5.551 s |
| Distributed | 10 | 10/10 | 10/10 | 24.700 s | 5.167 s | 49.653 s | 5.354 s |
| Distributed | 100 | 12/100 | Source timeout | — | — | 57.813 s | — |
| Distributed | 500 | Source timeout | Source timeout | — | — | — | — |

The shared one-fleet issue, model request and settlement went from **4.923 / 5.479 / 5.545 s** to **4.532 / 5.267 / 5.344 s**. Both medians represent one sample. The shared ten-fleet P50 fell by **19.480 s**.

The four larger refactor attempts timed out during source preparation, before the failure and restore measurement. At the end of the distributed 500-fleet attempt the rig had grants for all 500 fleets and zero ready heads. The completed cases establish a ten-fleet improvement and leave the larger recovery cohorts unmeasured.

A read-only length sample of the live distributed one-fleet source checkpoint in the refactor rig measured **1,138 bytes** of encrypted history plus **88 bytes** of encryption metadata, for **1,226 bytes** total before database row and index overhead. That sample contained zero workspace file bytes and zero manifest pages.

| Measurement | Required evidence |
|---|---|
| Eligibility to issue and first resumed model request | Per-fleet timestamps; P50/P95/P99; first/last and total drain time |
| Capture and restore stages | Queue, scan/hash, authorization, I/O, validation, adoption and thaw timings |
| Control-plane load | Native RPCs, SQL statements/transactions, acquire/lock waits and timeouts |
| Resource bounds | Peak RSS, admitted payload bytes, active HTTP operations and blocking jobs |
| Storage work | PUT/GET counts, verified/deduplicated bytes and local read/write bytes |
| Correctness under the burst | No early takeover, stale writes, mixed attempts, partial workspace, duplicate settlement or renewal-induced attempt storm |
| Fault variants | Lost Commit ACK, slow object, unavailable backend, one contended owner, reconnect and cancellation |
| Coverage | More than 99% per changed production source file and changed lines; list denominator and missed lines |

## Fingerprint policy

Review receipts, source fingerprints, exact commands and raw evidence remain separate.
This repo has no reviewed Orly hermetic reuse policy, so standalone Make passes are not reusable Orly completion records.
Batch coherent edits, rerun affected proof, and execute the required final gate on the pushed tree; never manufacture an override or a current binding.

Tenant-runner work is parked by the owner; the human check remains unperformed.
Thirteen identical full-history secret-scan findings were observed at the exact base and implementation history; no suppression, history rewrite or passing scan is claimed.

## Manual two-runner drill

Use a disposable fleet and two enrolled platform runners on the same development daemon. Both runners need compatible policies and funded model access. A file-backed check also needs a verified sandbox and private object storage. [Development setup](../../development.md#shared-development-worktrees)

1. Use `agentsfleet platform runners list` and `agentsfleet --json platform runners show <RUNNER_ID>` to check that both intended runners are healthy. Confirm which disposable container belongs to runner A.
2. Send the prepared recovery task through `agentsfleet fleet steer <FLEET_ID> "<fixture prompt>"`. Keep its controlled second action waiting until the `respawn_checkpoint_committed` receipt proves a usable generation.
3. Read A's expiry with `agentsfleet --json platform runners leases <RUNNER_A_ID> --fleet <FLEET_ID>`. Pause the owned source container with `docker pause <RUNNER_A_CONTAINER>`. Keep the original reply open.
4. Read B's leases. Require the same event, a greater fence issued at or after expiry, and `respawn_checkpoint_restored` for the exact generation before the next model or tool call. Check the marker file for the file-backed case.
5. After the offline sweep, inspect A with `agentsfleet platform runners show <RUNNER_A_ID>` and `agentsfleet platform runners events <RUNNER_A_ID>`. Use `docker unpause <RUNNER_A_CONTAINER>` to restore the lab. Watch for one automatic lift after continuous healthy heartbeats.

The operator drill still needs a recorded human run. The automated owned-rig command covers controlled API-only process failure. [Rig command](../../architecture/runner_checkpoint_recovery.md#operator-and-lab-checks)

## Historical drafts

Commit `976883d00` preserves the original review walkthrough, full initial file inventory, and detailed manual fixture instructions. Use Git to inspect the draft alongside the implementation and measurement evidence.

```bash
git show 976883d00:docs/v2/reviews/m247-implementation-and-manual-check.md
git show 976883d00:docs/v2/reviews/m247-files-and-symbols.md
git show 976883d00:docs/v2/reviews/m247-scalability-and-refactor.md
git show 976883d00:docs/v2/reviews/m247-two-host-walkthrough.md
```
