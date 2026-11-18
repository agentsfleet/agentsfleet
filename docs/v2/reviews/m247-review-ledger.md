---
type: reference
audience: contributor
verified: 2026-11-18
product_version: 0.78.0
executable: false
---

# M247 review and verification ledger

This ledger records the baseline, review dispositions, benchmark evidence, and verification still due for platform runners. The implemented architecture lives in the [recovery guide](../../architecture/runner_checkpoint_recovery.md).

Base: `97224ed7432c9dd124ea8797d4324156b7022c07`; pre-refactor implementation: `86f481126c1aa053c34ef3bdaaa6f07baa1c74ab` plus candidate prefix `22ac73ab741b`. The frozen production baseline is `d4823563e80829b97589846a1c7997964fdf0274`.

The optimized comparison used the same fixture against the refactor commit `976883d00`.

## Spec closure and remaining acceptance

Indy requested closure on Nov 18, 2026: "I will do the manual recovery check later, so move the spec to done/".
The [specification](../done/M247_001_P1_API_CLI_DOCS_OBS_UI_FAILED_HOSTS_RESPAWN_FROM_CHECKPOINT.md) now has `Status: DONE` at his request.
Dimension 6.6 remains deferred and unperformed.

This move does not grade the remaining acceptance checks as passed.
R13 benchmarks, R14 greater-than-99% coverage, final automated acceptance and the post-M246 SQL/schema review remain open.
The review closures are pushed at `2a88201a9`, including merge `b4c2d789c` of M246 main.

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

Focused evidence includes 19 real turn-checkpoint integration tests, two real capacity tests, migration checks and the API-only two-process crash/restore drill. Runner unit tests include lost Commit acknowledgements, page limits, capture bounds and parallel authenticated file restore.

Raw local logs retain the failures before their repairs; final CI must verify the committed refactor.

## Second-pass findings and dispositions

The table preserves the 19 findings from the second pass. The verification below records their current evidence.

| # | Severity / source | Location | Finding | Disposition at `976883d00` |
|---|---|---|---|---|
| F1 | CRITICAL / testing | `rustd/crates/runner/afr_supervisor/src/lease_loop/turn_recovery.rs:11` | Missing ordinary Files offer through production supervisor/model entry, warm-hold retirement, thaw, and failure-before-model proof | Closed. Three supervisor and actual-agent-loop checks passed. They restore encrypted files over pinned HTTPS, retire a matching warm hold, prove thaw before model entry, and refuse corrupt bytes or a failed thaw before the model. The daemon boundary is scripted; full process acceptance remains separate. |
| F2 | CRITICAL / testing | `rustd/crates/daemon/afd_fleet/src/lease/turn_checkpoint/guard.rs:60` | Missing connection-bound Begin/Commit/Read/Adopt mutation-time supersession and credential-rotation races | Closed. Both live tests passed across all eight verb/change combinations. Each waits on an observed host-row lock, proves current authority first, then checks `RUN_LEASE_LOST` and unchanged checkpoint, pin and admission state. |
| F3 | CRITICAL / testing | `rustd/crates/daemon/afd_runner/src/heartbeat/cordon.rs:33` | Missing degraded/invalid/fallback and assignment/connection-race automatic-lift proofs | Closed. All 11 live cordon checks passed together. Malformed and degraded observations reset proof; changed assignments reset it after a lock wait; superseded connections and rotated credentials advance neither liveness nor health proof. The fallback, missed cadence and healthy-lift cases passed in the same run. |
| F4 | CRITICAL / testing | `rustd/crates/daemon/afd_fleet/src/lease/deliver/issue.rs:52` | Missing real NATS issued-reclaim versus fresh/refused/reserved publication proof | Closed. Three live NATS checks passed. One hundred concurrent claimers yield one reservation, no provisional restart, one issued replacement and one frame matching its stored event, fence and timestamp. Fresh and refused issues emit no restart. |
| F5 | CRITICAL / state-machines | `rustd/crates/daemon/afd_fleet/src/lease/turn_checkpoint/pins.rs:267` | untested cell: ApiOnly × Abandon → retain the abandoned pending plan, release its writer pin and preserve the ready head | Verified on the real fleet suite for API-only state. |
| F6 | CRITICAL / state-machines | `rustd/crates/daemon/afd_fleet/src/lease/turn_checkpoint/maintenance.rs:98` | untested cell: ApiOnly × CollectTerminal → erase terminal private generations, recovery head and allocation receipts after retention without erasing public source state | Verified on the real fleet suite for API-only state. |
| F7 | CRITICAL / state-machines | `rustd/crates/daemon/afd_fleet/src/task/checkpoint/transfer/signing.rs:81` | untested cell: Incremental × VerifyUpload → authenticate and persist a newly uploaded incremental object and account its unique stored quota | Closed. The live private SDK check uploads a new incremental object, authenticates it twice and proves its physical bytes are charged once. |
| F8 | CRITICAL / state-machines | `rustd/crates/daemon/afd_fleet/src/task/checkpoint/transfer/signing.rs:141` | untested cell: Incremental × Restore → restore the exact committed incremental manifest/object references under reader custody while preserving its base | Closed. The same live check reads the exact committed overlay and retained base pages under reader custody, downloads both exact ciphertexts and repeats the read after orphan cleanup. |
| F9 | CRITICAL / state-machines | `rustd/crates/daemon/afd_fleet/src/task/checkpoint/maintenance/mod.rs:75` | untested cell: Incremental × CollectOrphan → erase expired incremental private keys/references and release its physical quota once without retiring a live base | Closed. The same live check collects an expired uploaded incremental orphan, proves wrapped-key removal and acknowledged purge custody, retains both live generations and the base, and proves one quota release across repeated collection. |
| F10 | CRITICAL / state-machines | `rustd/crates/daemon/afd_fleet/src/lease/turn_checkpoint/pins.rs:287` | untested cell: writer × PinRelease → release the exact writer pin while preserving its pending plan and head, and refuse later protected operations | Verified on the real fleet suite for a pending writer. |
| F11 | CRITICAL / state-machines | `rustd/crates/runner/afr_supervisor/src/recovery/turn/publication.rs:47` | untested cell: Commit × ReconcileRefusal → clear indeterminate publication only after exact abandonment, or stop when abandonment cannot settle it | Verified in the runner unit suite. A refused replay cannot start a new capture until its exact pending plan is abandoned. |
| F12 | CRITICAL / native | `rustd/crates/daemon/afd_fleet/src/lease/sql/commit_report.sql:181` | A report can decide a hold before the audit insert's foreign-key check takes a host lock. Exercise expiry and competing issue in that interval, then serialize hold-producing reports from before their decision through commit | Fixed. The real delayed-report regression failed at the baseline and passed after the lock change. |
| F13 | INFORMATIONAL / data-migration | `schema/20261117154627_runner_turn_checkpoints.sql:6` | Missing fleet_id-leading index for fleet purge | Implemented in the checkpoint schema. The focused migrations and retention suites passed. |
| F14 | INFORMATIONAL / parent | `schema/20261117154626_runner_turn_recovery.sql:4` | Missing head_turn_checkpoint_id-leading index for referenced checkpoint deletion | Implemented in the recovery schema. The focused migrations and retention suites passed. |
| F15 | INFORMATIONAL / api-contract | `rustd/crates/daemon/afd_api_operator/src/handler/operator/events.rs:31` | Allowed event_type query description omits runner_uncordoned; regenerate OpenAPI | Fixed and regenerated. The operator filter documents runner_uncordoned. |
| F16 | INFORMATIONAL / design | `ui/packages/app/tests/e2e/fixtures/fleet-stream-browser-probe.ts:21` | New reply status adds a second output; transport-status assertions need a dedicated selector | Fixed. Three real Chromium transport and restart tests passed. |
| F17 | INFORMATIONAL / performance | `rustd/crates/runner/afr_supervisor/src/recovery/turn/capture.rs:149` | Ordinary upload serializes singleton native calls and physical transfers while frozen | Fixed. A four-object HTTPS test proved one authorization and one verification request. |
| F18 | INFORMATIONAL / performance | `rustd/crates/daemon/afd_fleet/src/lease/turn_checkpoint/maintenance.rs:35` | First owner lock timeout aborts later maintenance and repeats oldest candidate without cursor | Fixed. The real locked-owner regression failed at the baseline and passed after the cursor change. |
| F19 | INFORMATIONAL / performance | `rustd/crates/daemon/afd_fleet/src/lease/turn_checkpoint/sql/maintenance_candidates.sql:2` | LIMIT bounds returned owners but not the joined candidate scan; empty heads are created for uncaptured offers | Closed. Indexed owner pagination seeks before inspecting each owner's work; the locked-owner regression proves later owners progress. Quantitative maximum-manifest results remain required by R13. |

The state-machine record has nine machines, 90 cells, 83 mapped to tests and seven uncovered.
The installed matrix checker passed its completeness/path checks; mapping to a test does not prove that test ran or satisfies every other specification dimension.

## Review closure checks

Indy requested that the recorded findings close before final acceptance and
the implementation PR. The following checks now fill the seven critical
proof gaps.

Their checks of remaining state passed in the reset focused run.
Final acceptance, burst measurement and coverage retain their separate
rubric entries.

| Finding | Added check |
| --- | --- |
| F1 | `afr_supervisor/src/recovery/supervisor_tests` drives an ordinary Files offer through the supervisor and agent loop. It retires an existing warm hold, checks the restored root and bytes at model entry, and refuses corrupt ciphertext or a failed thaw before any model call. |
| F2 | `afd_fleet/tests/integration_turn_checkpoints/authority.rs` proves Begin, Commit, Read and Adopt succeed with current authority, then refuse replay after connection replacement or credential rotation during an observed PostgreSQL host-lock wait. Checkpoint, pin and admission state must stay unchanged. |
| F3 | `afd_runner/tests/integration_runner_plane/cordon/return_health.rs` adds malformed and degraded observations plus assignment, connection and credential changes after policy read. The existing fallback check remains in the same cordon group. |
| F4 | `afd_fleet/tests/integration_lease_gates/restart.rs` observes real NATS traffic through a same-connection publication barrier. Fresh issue, reservation and refused issue emit no restart. One hundred competing claimers must create one replacement row and one matching restart frame. |
| F7–F9 | `afd_fleet/tests/integration_tasks/decisions/checkpoint_remote/incremental` verifies newly uploaded overlay objects twice, checks unique quota, reads the exact overlay and retained base, and collects an expired uploaded orphan without erasing either live generation or releasing quota twice. |

The M246 integration merge is committed at `b4c2d789c`. The first subsequent
`make test-unit-all` run passed its runner and daemon stages, then stopped on
a misspelled object-status variant in the new incremental test. That test
now uses the actual `Missing` variant.

The first workspace-wide all-feature test build then caught a temporary heartbeat value borrowed across
`tokio::join!` in the new race test. Naming that value fixes its lifetime.
The second workspace build passed all 197 test targets.

The first focused run caught the canned supervisor fixture's invalid workspace ID before
restore. Its replacement now uses a valid ID, a real-loop context budget and
an exactly matching warm sandbox policy. All three file-restore checks then
passed in 0.94 seconds.

The reset live follow-up passed both authority tests across eight verb/change
combinations in 15.01 seconds and all 11 cordon checks together in 13.97
seconds. The fresh/reserved and refused-issue NATS cases passed.

The remaining NATS test incorrectly replayed one reservation through issue
100 times. Its setup now races 100 actual claimers before issuing the sole winner, matching
Dimension 3.1. All three NATS cases passed on the corrected setup.

The completed reset closure run passed 32 checks across eight groups.

| Group | Passed | Runtime |
| --- | --- | --- |
| Supervisor file restore and model entry | 3 | 1.14 s |
| Checkpoint authority, eight verb/change combinations | 2 | 12.67 s |
| Automatic cordon and healthy return | 11 | 9.44 s |
| NATS restart publication | 3 | 1.44 s |
| Incremental upload, restore and orphan collection | 1 | 3.29 s |
| Direct-claim delivery fixtures | 4 | 3.88 s |
| Parked-head deadline fixtures | 2 | 0.71 s |
| Configured task rollout fixtures | 6 | 6.13 s |

The workspace all-feature build produced the binaries used by the focused
run. Its driver checks that each named group selects tests before running
it. These are focused results, not a repository-wide verification pass.

The fixture corrections above changed test inputs. They did not weaken
expected production outcomes or change the production statements. Manual
Cargo commands and Make now use the same local commit stamp to avoid
rebuilding the API and its dependants when switching invocation paths.

Three older direct-claim fixtures also needed the new resume boolean before
their platform-model parameter. Production statements are unchanged by that
fixture repair.

## Added burst and quality requirements

| ID | Area | Required work |
|---|---|---|
| S1 | Bounded recovery drain | Use recovery-only bounded round-robin. Keep ordinary ring budgets and avoid retrying a locked prefix of 64 candidates while fleet 65 can run |
| S2 | Shared resource ownership | Use runner-wide byte/I/O/blocking budgets and reusable transport, with family-specific authority |
| S3 | Cross-file batches | Batch physical transfers across file boundaries and remove duplicate plaintext buffers |
| S4 | Short custody and bulk deadlines | Hold a shared host lock through a hold-producing report, conflict with exclusive issue, and preserve fresh-clock/connection checks. Remove empty quota writes and bound verification deadlines |
| S5 | Measurement and coverage | 1/10/100/500 interrupted fleets; dist comparisons; >99% per production file and patch lines |

The Astra direction review identified two benchmark fixture limitations. Direct `Session::offer` bypasses scheduler registration and the replica wake path.

The default seed gives each fleet a separate workspace and hides contention on shared quota. R13 requires registered, already-idle replacement hosts and both workspace distributions.

The measured baseline uses those registered sessions. The refactor checks below exercise F12 and S1 on PostgreSQL 19rc2.

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

The follow-up standards review reproduced a skipped-page starvation case. A live regression kept newer admissions ready while releasing 64 older locks.

The prior cursor starved the oldest admission. The bounded cursor passed both page tests plus five retention cases. [Recovery page tests](../../../rustd/crates/daemon/afd_fleet/tests/integration_ledger_queue/recovery_page.rs)

The gateway and supervisor follow-up passed 544 unit tests with one ignored. The benchmark source also compiled.

An earlier unit attempt failed the old Busy classification assertion, which was corrected to the intended typed unavailable error. A later attempt failed the existing expired-deadline HTTP/3 transport test; the full retry passed, and the initial red log remains.

No claim of an unrelated baseline defect is made without equivalent reproduction.

## Performance and coverage acceptance

Compare the frozen pre-refactor revision with the final revision on the same owned local infrastructure. Use 1, 10, 100 and 500 interrupted fleets, declare compatible source/replacement worker counts, and use multiple failed hosts where the 64-worker limit requires them.

Separate API-only roots, four small files, large multi-chunk files and metadata-heavy manifests.

**Benchmark correction from the Astra review.** The existing lease bench uses `afd_gateway::test_util::Session::offer`, which calls `services.offer()` directly. Its session constructor does not register the host with the scheduler, and its window listens on a separate wake instance.

Starting `Gateway::run` alongside that fixture would still miss the recovery path. Use real registered native sessions with the production scheduler, shared wake and normal credit reporting.

Keep replacement hosts connected and settled at idle before killing the source hosts. Count ordinary rings, Ready credit increases and fallback probes so their causes can be distinguished.

Capture the eligibility deadline for each lease. Hold resumed model responses at a barrier for the dispatch-only scenario so settlement does not create fresh credits.

Also run the scenario through settlement to measure the user-visible result.

The existing lease seed assigns a distinct workspace and tenant per fleet. Add a shared-workspace case to expose quota contention and a distributed-workspace case for comparison.

Fit file sizes to the 50-GiB workspace quota and available disk on both revisions. Exercise large multi-chunk and high-entry-count cases with explicitly stated populations.

Never extrapolate those timings to a larger population. Record the number of interrupted, eligible, issued, restored, settled, failed and outstanding fleets at the same fixed observation horizon.

Report a cohort percentile only when that rank was observed. Retain failures and right-censored results.

Repeat matched runs and record per-run spread. Include ordinary single-ring and idle query costs as regression controls.

Baseline production commit `d4823563e80829b97589846a1c7997964fdf0274`. The verified installed PostgreSQL 19rc2 Alpine image index is `sha256:7e3904251ae6f1e80c756b0ea12fbac512d8b9512de0047cdcbe4c6a69dff60b` on linux/amd64.

Pin the image in the measurement record along with the benchmark harness hash, runner/daemon binary hashes, fixture seed and concurrency. Run the baseline before any production code edits.

Reuse the same harness for the final comparison.

Record cold/warm storage, unique/reused chunks, concurrency limits, data size, injected storage delays and the actual build profile. Published production-performance comparisons use `dist`; debug/instrumented tests establish behavior and must be labelled accordingly.

Use controlled model transport for repeatability and identify it in every result.

### Optimized fixture smoke, 2026-11-18

The first successful `make bench-lease PROFILE=rig BENCH_SHAPE=recovery BENCH_FLEETS=1 BENCH_WORKSPACES=shared` used `dist` at `d4823563e8` with the fixture fingerprint `13f3b7288385869205e5d37bb41d9cddfc81fe8cac9c901367679208849034d7`. The API-only source committed one root before its process was killed.

The replacement issued fence 2 at **4,759 ms** and called the model with the recovered prefix at **5,364 ms**. It settled at **5,444 ms**. Each time starts at the old lease's expiry.

The first 10-fleet attempt issued all 10 source leases and timed out before creating checkpoints. The fake model shared the runner transport's four-stream connection bound.

The comparison requires a fresh one-fleet run alongside the larger cohorts on the regular TLS model fixture. The single-fleet numbers above record the earlier fixture smoke.

### Measured API-only baseline

`make bench-lease PROFILE=rig BENCH_SHAPE=recovery` ran with the regular TLS fixture at `40a579688` against the unchanged production revision `d4823563e8`. Each successful cohort committed its source roots before the failed hosts were killed.

The observation window ended 60 seconds after the latest source lease expired. All times below start at each fleet's own eligibility deadline.

[Raw results and fingerprints](../../../bench/baselines/lease-recovery-api.rig.json)

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

For the shared one-fleet case at `d4823563e8`, the observed P50 was **5.479 s**, or **about 5.5 s**, from one sample. The source committed an API-only root containing its accepted model answer and a folded follow-up.

The fixture killed the source process after the idle replacement connected.

The timer starts at the source lease's eligibility deadline, the later of the lease and slot expiries. The replacement received fence 2 at **+4.923 s**.

Its first model request contained the saved answer and follow-up exactly once at **+5.479 s**. The fixture returned the next answer, and the replacement settled at **+5.545 s**.

The 100- and 500-fleet trials with 12 completions leave 88 and 488 fleets outstanding respectively. Their cohort P50, P95 and P99 remain unobserved.

The distributed 500-fleet source preparation timed out twice. Both attempts issued a source lease for each of 500 fleets and produced zero ready checkpoint heads before timing out.

The raw record preserves those two attempts and their counts.


### Measured API-only comparison

Both builds used the same optimized profile, model fixture and owned PostgreSQL and NATS rig. Times start at each source lease's expiry and end at the first model request containing the saved conversation and folded follow-up.

The observation window ended 60 seconds after the final old lease expired. [Refactor samples and fingerprints](../../../bench/baselines/lease-recovery-api.refactor.rig.json)

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

The shared one-fleet issue, model request and settlement went from **4.923 / 5.479 / 5.545 s** to **4.532 / 5.267 / 5.344 s**. Both medians represent one sample.

The shared ten-fleet P50 fell by **19.480 s**.

The four larger refactor attempts timed out during source preparation, before the failure and restore measurement. At the end of the distributed 500-fleet attempt the rig had grants for all 500 fleets and zero ready heads.

The completed cases establish a ten-fleet improvement and leave the larger recovery cohorts unmeasured.

A read-only sample of the refactor's distributed one-fleet source checkpoint measured **1,138 bytes** of encrypted history and **88 bytes** of encryption metadata. The encrypted fields totaled **1,226 bytes** before database row and index overhead. That sample contained zero workspace file bytes and zero manifest pages.

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

Review receipts, source fingerprints, exact commands and raw evidence remain separate. This repo has no reviewed Orly hermetic reuse policy, so standalone Make passes are not reusable Orly completion records.

Batch coherent edits, rerun affected proof, and run the required final gate on the pushed tree; never manufacture an override or a current binding.

Tenant-runner work is parked by the owner; the human check remains unperformed.
Thirteen identical full-history secret-scan findings were observed at the exact base and implementation history; no suppression, history rewrite or passing scan is claimed.

## Proposed agentsfleetd deployment configuration

The reference checkout is `openai/codex` at
`ca21fe65d18a9b6326759fb4263c65dfbb645aa5`. Indy asked to follow its settings
loading approach.

Its `ConfigBuilder` collects layers, merges TOML values
before typed deserialization, validates the resulting configuration and passes
it to service owners. The app server shares the result through `Arc<Config>`.

For this daemon, the existing `BootConfig` is the resolved startup value.
Copyable lease clocks and scheduler limits pass through construction, while
environment and vault credentials keep their existing injectable sources.
[Codex builder](https://github.com/openai/codex/blob/ca21fe65d18a9b6326759fb4263c65dfbb645aa5/codex-rs/core/src/config/mod.rs#L1554)
· [Codex layer loading](https://github.com/openai/codex/blob/ca21fe65d18a9b6326759fb4263c65dfbb645aa5/codex-rs/config/src/loader/README.md)
· [Codex configuration injection](https://github.com/openai/codex/blob/ca21fe65d18a9b6326759fb4263c65dfbb645aa5/codex-rs/app-server/src/lib.rs#L1039)

```text
Git  -- application TOML for local, dev or prod --+
CI   -- tested daemon image by digest ------------+--> agentsfleetd boot
Fly  -- app secrets and mounted TLS files --------+          |
                                                              +--> validate once
                                                              +--> serve, then /readyz
```

The shared loader uses the upstream `toml` parser with Serde.
The daemon owns `ConfigToml` and validates it before opening services.
Select the file with `--config` or `AGENTSFLEET_CONFIG_FILE`.

Precedence is explicit CLI input, a present TOML value, its environment
variable, then the current default. The no-subcommand preflight uses the
same resolution as `serve`.

An unreadable file, unknown field or invalid value reports a safe path and
field error. The existing `EnvSource` supplies environment values and secret
lookups. [CLI dispatch](../../../rustd/crates/daemon/main/src/cli.rs) · [Boot validation](../../../rustd/crates/daemon/main/src/preflight/mod.rs) · [Environment seam](../../../rustd/crates/foundation/af_core/src/env.rs)

`env_toml` 0.2.5+upstream-1.1.6 was published on 29 September 2026.
The registry showed 228 downloads and zero published dependents on
18 November 2026. [Release history](https://docs.rs/crate/env_toml/latest) · [Registry](https://crates.io/crates/env_toml) · [Reverse dependencies](https://crates.io/crates/env_toml/reverse_dependencies)

A standalone Rust probe verified literal numbers and booleans, quoted
environment references, numeric defaults and a failure for an unset required
variable. A table reference requires application-side resolution.
The daemon uses its existing named environment readers for credentials.

```toml
# deploy/fly/agentsfleetd-dev/agentsfleetd.toml
[server]
port = 3000
api_url = "https://api-dev.agentsfleet.net"
app_url = "https://app-dev.agentsfleet.net"
max_in_flight_requests = 256
max_sse_streams = 256

[database.api]
max_connections = 8

[runner.listener]
address = "[::]:4443"
tls_cert_file = "/etc/agentsfleet/runner-stream/server-chain.pem"
tls_key_file = "/etc/agentsfleet/runner-stream/server.key"

[runner.lease]
ttl_seconds = 35

[runner.recovery]
resume_from_checkpoint = false
offer_fallback_seconds = 5
max_concurrent_claims = 8
```

### Inputs to move into the application file

| Section | Current input | Proposed ownership |
| --- | --- | --- |
| `server` | `PORT`, `API_URL`, `APP_URL`, `SSE_MAX_STREAMS`, and fixed API admission of 256 per replica | Keep HTTP port 3000 on Fly to match the readiness and tunnel targets. Let deployments set API and SSE budgets separately. [Server](../../../rustd/crates/daemon/main/src/serve/mod.rs) · [Current knobs](../../../rustd/crates/daemon/main/src/preflight/knobs.rs) |
| `database.api` | `DATABASE_POOL_SIZE[_API]`, `DATABASE_MIN_POOL_SIZE[_API]`, `DATABASE_ACQUIRE_TIMEOUT_MS[_API]` | Retain the pool defaults and the dev override of 8 and prod override of 5 per replica. Keep database URLs and URL-embedded TLS or authentication settings in the secret source. [Pool resolution](../../../rustd/crates/daemon/afd_db/src/config/mod.rs) · [Dev Fly config](../../../deploy/fly/agentsfleetd-dev/fly.toml) · [Prod Fly config](../../../deploy/fly/agentsfleetd-prod/fly.toml) |
| `runner.listener`, `runner.stream` | `AGENTSFLEET_RUNNER_STREAM_LISTEN_ADDR`, `AGENTSFLEET_RUNNER_STREAM_TLS_CERT_FILE`, `AGENTSFLEET_RUNNER_STREAM_TLS_KEY_FILE`, `AGENTSFLEET_RUNNER_CONTROL_MEMORY_MIB`, `AGENTSFLEET_RUNNER_DATA_MEMORY_MIB` | Put the private bind address, file paths and bounded memory settings in TOML. The certificate and key bytes come from Fly file secrets. Reuse `MemorySettings` and its 16/48 MiB defaults. [Listener](../../../rustd/crates/daemon/main/src/preflight/runner_listener/mod.rs) · [Memory limits](../../../rustd/crates/protocol/af_stream/src/framing/memory.rs) |
| `bus` | `AGENTSFLEET_DEV_NAMESPACE`, `AGENTSFLEET_RUNNER_WAKE_QUEUE_CAPACITY` | The namespace belongs to local or dev deployments. The wake queue currently defaults to 64. Resolve credential-bearing `NATS_URL` from the secret source. [NATS configuration](../../../rustd/crates/daemon/afd_bus/src/nats/config.rs) · [Wake queue](../../../rustd/crates/daemon/afd_bus/src/wake/config.rs) |
| `identity`, `connectors` | `OIDC_ISSUER`, `OIDC_AUDIENCE`, `OIDC_JWKS_URL`, `CLERK_API_BASE`, `PLATFORM_ADMIN_WORKSPACE_ID`, `SLACK_API_URL` | Use TOML for public endpoints and the workspace ID. Resolve Clerk credentials and an authenticated `IDENTITY_HTTP_PROXY_URL` through the existing environment. [Identity and endpoints](../../../rustd/crates/daemon/main/src/preflight/read.rs) |
| `telemetry` | `AGENTSFLEET_LOG_LEVEL`, `POSTHOG_HOST`, `OTEL_EXPORTER_OTLP_ENDPOINT`, `OTEL_EXPORTER_OTLP_PROTOCOL`, `OTEL_EXPORTER_OTLP_TIMEOUT` | Use TOML for the level, credential-free destinations, protocol and timeout. Keep project keys, authorization headers, and the collector service's own credentials in their existing sources. [Log bootstrap](../../../rustd/crates/daemon/main/src/main.rs) · [Exporter settings](../../../rustd/crates/protocol/af_otlp/src/config/mod.rs) |
| `checkpoint.storage` | `R2_CHECKPOINT_STORAGE` JSON with endpoint, private bucket and `credential_name` | Accept the same fields as a typed TOML table. Look up the credential by name in the platform-admin vault. A file-backed deployment needs both the location and that vault row. [Boot reader](../../../rustd/crates/daemon/main/src/preflight/checkpoint.rs) · [Storage settings](../../../rustd/crates/daemon/afd_fleet/src/task/storage/mod.rs) |

Keep `ENCRYPTION_MASTER_KEY`, `AUTH_SESSION_CODE_PEPPER`, `DATABASE_URL_API`, `DATABASE_URL_MIGRATOR`, `NATS_URL`, `CLERK_SECRET_KEY`, `CLERK_WEBHOOK_SECRET`, `POSTHOG_API_KEY` and `OTEL_EXPORTER_OTLP_HEADERS` in the deployment secret source. The Fly migration command resolves its own role and environment before starting serving Machines. [Migrate entry point](../../../rustd/crates/daemon/main/src/migrate.rs) · [Current Fly deploy](../../../.github/workflows/deploy-dev-fly.yml)

### Code policies and live runner capacity

| Existing setting | Current value | Proposed action |
| --- | --- | --- |
| Lease and offline clocks | TTL 35 s, offline at three TTLs (105 s), 10 s runner heartbeat, 30 s of healthy heartbeats to lift quarantine | Pass the validated TTL and derived offline interval to all daemon consumers. Preserve the heartbeat and uncordon relationship. [Clock contract](../../../rustd/crates/foundation/af_core/src/timing/mod.rs) · [Liveness pass](../../../rustd/crates/daemon/afd_runner/src/sweep/liveness/mod.rs) |
| Renewal and transport budgets | 5 s renewal tick, 15 s renewal window, 5 s answer, 7 s reconnect made of 2 + 2 + 3 s | Validate TTL against the wire and runner's existing clocks. Keep the renewal and reconnect budgets as one protocol contract. [Clock assertions](../../../rustd/crates/foundation/af_core/src/timing/mod.rs) · [Runner cleanup margin](../../../rustd/crates/runner/afr_supervisor/src/renew/mod.rs) |
| Recovery backstop and dispatch | 5 s probe per replica, 8 concurrent claims per drain and one per eligible session | Expose the probe interval and concurrent-claim cap with their current defaults. Prove the settings against the per-replica database pool and measure idle probes and burst time. Keep the SQL page size of 64 and the revisit rule together. [Probe](../../../rustd/crates/daemon/afd_gateway/src/scheduler/recovery/mod.rs) · [Drain](../../../rustd/crates/daemon/afd_gateway/src/scheduler/recovery/drain.rs) · [Cursor](../../../rustd/crates/daemon/afd_fleet/src/lease/assign/ledger.rs) |
| Runner workers | Per-host `assigned_policy.worker_count`, shared bounds 1–64 | Set A and B to 16 in each runner's policy if the hosts can support it. `agentsfleet platform runners update <RUNNER_ID> --policy <FILE>` replaces the supplied policy, so use the complete policy. Fly CPU and replica count size the daemon's compute capacity. [Worker bounds](../../../rustd/crates/foundation/af_core/src/limits.rs) · [Runner update CLI](../../../cli/src/program/tree/platform-runners.command.ts) |
| Other sweep and lifetime controls | 2 s expiry, 10 s host liveness, 30 s census, 5 min checkpoint cleanup, 12 h lease runtime, 10 min sandbox hold, 7 d terminal turn retention, 30 d settled runner history | Treat cadence and retention as separate follow-up settings with their own effect on database load, custody and cleanup. [Expiry](../../../rustd/crates/daemon/main/src/sweepers/expiry.rs) · [Checkpoint cleanup](../../../rustd/crates/daemon/main/src/sweepers/checkpoints.rs) · [Terminal retention](../../../rustd/crates/daemon/afd_fleet/src/lease/turn_checkpoint/maintenance.rs) · [Lease retention](../../../rustd/crates/daemon/afd_runner/src/sweep/retention/mod.rs) |
| Storage and scheduler ceilings | 2 verify operations per session and 8 per daemon, 2 file recovery operations per runner, 4 chunks of up to 8 MiB each per window, 50 GiB workspace quota | Treat semaphore counts as performance parameters backed by measured memory and contention. Keep wire sizes, quota accounting, fence increments and retry caps in their owning rules. [Verification admission](../../../rustd/crates/daemon/afd_gateway/src/session/dispatch/turn_checkpoint.rs) · [Runner admission](../../../rustd/crates/runner/afr_supervisor/src/recovery/admission.rs) · [Storage and wire limits](../../architecture/runner_checkpoint_recovery.md) |
| Other daemon capacities | Database idle timeout 10 min and connection lifetime 30 min, admission backlog 100,000, SSE keepalive 15 s, daemon drain 15 s | Review with the owning database, API and deployment budgets. Authentication and token lifetimes require an identity policy decision. [Database pool](../../../rustd/crates/daemon/afd_db/src/pool/mod.rs) · [Backlog budget](../../../rustd/crates/daemon/afd_admission/src/budget/mod.rs) · [SSE keepalive](../../../rustd/crates/daemon/afd_sse/src/lib.rs) · [Daemon drain](../../../rustd/crates/daemon/main/src/serve/drain/mod.rs) |

`ttl_seconds` passes through construction as one validated clock.
Grants and renewals write absolute expiry timestamps.
An existing lease keeps its saved deadline until renewal succeeds.

Fleet claim, metering, authoring, wallet checks, affinity, runner status and
liveness use that clock. Validation derives the minimum from renewal and
reconnect timing. The default requires more than 32 seconds and less than
12 hours.

The fallback interval must be positive and shorter than the lease lifetime.
Runner shutdown has its own named 35-second grace.
[Fleet claim](../../../rustd/crates/daemon/afd_fleet/src/lease/assign/ledger.rs) · [Fleet renewal](../../../rustd/crates/daemon/afd_fleet/src/lease/renew/batch/columns.rs) · [Metering](../../../rustd/crates/daemon/afd_fleet/src/lease/sql/meter.rs) · [Authoring lease](../../../rustd/crates/daemon/afd_authoring/src/lease/claim/issue/mod.rs) · [Runner shutdown](../../../rustd/crates/runner/afr_supervisor/src/shutdown.rs)

With `resume_from_checkpoint = false`, captures may still commit and live
leases may reconnect. An expired ordinary admission with a committed
checkpoint waits at its fleet's head.

Ordinary admissions without a committed checkpoint and task admissions keep
their existing retry behavior. Both candidate selection and the locked claim
check the ordinary head. A refusal preserves its slot, fence and admission.

Enabling resume permits a higher-fence claim and exact-generation restore.
The recovery-head primary key supplies the lookup.
[Candidate SQL](../../../rustd/crates/daemon/afd_fleet/src/lease/sql/lease/select_ledger_candidates.sql) · [Locked claim SQL](../../../rustd/crates/daemon/afd_fleet/src/lease/sql/lease/claim_admission_head.sql) · [Head schema](../../../schema/20261117154626_runner_turn_recovery.sql)

| Production entry point to edit | Proposed change |
| --- | --- |
| `foundation/af_config/src/lib.rs`, `merge.rs`, `error.rs` (new) | Generic `ConfigBuilder::build<T>` reads through `FileSource`, merges invocation overlays and returns `garde::Valid<T>`. Each application supplies its own schema. Dependencies stay in foundation and parsing libraries. |
| `daemon/main/src/config/mod.rs`, `config/settings.rs`, `config/values.rs` (new), `cli.rs::run`, `preflight/mod.rs::preflight`, `preflight/config.rs::BootConfig` | The daemon owns `ConfigToml`, file selection and mapping to existing startup readers. It uses the shared loader and injects the resulting typed policy through construction. |
| `foundation/af_core/src/timing/lease.rs` (new) | `LeaseSettings` validates the deployment lifetime. `LeaseClock` exposes the valid lifetime and its derived offline threshold to stores. |
| `daemon/main/src/serve/mod.rs`, `plane/construction.rs::NativeInputs::build`, `sweepers/mod.rs` | Pass the request ceiling to `Admission::new` and the resolved policy to the stores, gateway and liveness pass. |
| `afd_fleet/src/lease/assign/ledger.rs`, `lease/renew/batch/columns.rs`, `lease/sql/meter.rs`, `lease/affinity.rs` | Bind one lease clock and the recovery policy to the existing queries. |
| `afd_authoring/src/lease/claim/issue/mod.rs`, `lease/renew.rs`, `review/wallet.rs`, `afd_runner/src/sweep/liveness/mod.rs`, `heartbeat/cordon.rs`, `view/decode.rs` | Reuse the derived clock in issuance, accounting, quarantine and status. |
| `afd_gateway/src/scheduler/recovery/mod.rs`, `scheduler/recovery/drain.rs`, `scheduler/subscriptions.rs` | Use the configured fallback cadence and concurrency in the scheduler and its retry backoff. |
| `select_ledger_candidates.sql`, `claim_admission_head.sql`, and the coarse `afd_fleet/src/lease/sql/lease/select_waiting_work.sql` | Check the feature before admitting a checkpointed ordinary retry. Ensure parked heads do not repeatedly generate useless wake work. The existing tables hold the checkpoint, event and fence. |

### Release and rollback

1. Point `AGENTSFLEET_CONFIG_FILE` at the local application file, or mount
   the file read-only into the daemon container.
   Run the no-subcommand preflight before `serve`.
   [Development workflow](../../development.md)

2. Keep `agentsfleetd.toml` beside the Fly config and set its path under Fly `[env]`.
   Run the existing deploy commands from the repository root.

   Fly reads `local_path` from that working directory.
   [Fly file loading](https://github.com/superfly/flyctl/blob/master/internal/appconfig/config.go#L144)

   ```toml
   [[files]]
     local_path = "deploy/fly/agentsfleetd-dev/agentsfleetd.toml"
     guest_path = "/etc/agentsfleet/agentsfleetd.toml"
     processes = ["app"]
   ```

3. Verify each environment file against the candidate binary and record its
   revision with the image digest. The pipeline stages credentials from
   1Password and applies the schema with `agentsfleetd migrate`.

   Serving Machines then start and `/readyz` must pass.
   Confirm the effective recovery settings in each replica's startup event.
   [Dev deploy](../../../.github/workflows/deploy-dev-fly.yml) · [Readiness checks](../../../.github/workflows/deploy-dev-verify.yml) · [Production workflow](../../../.github/workflows/release.yml) · [Fly secrets](https://fly.io/docs/apps/secrets/)

4. Verify both test hosts have the compatible runner build.
   Dev starts with the flag false.
   Commit a disposable checkpoint on A, then stop A.

   Wait past A's saved lease expiry and confirm the admission waits while B
   is idle. A different fleet must remain eligible.
   Roll the same image with the dev flag true.

   Verify one higher-fence claim, the exact generation and one settlement.
   Repeat with workspace files, a private bucket and the platform-admin vault
   credential.

   Measure from saved expiry to the replacement model request.
   [Dev runner deploy](../../../.github/workflows/deploy-dev.yml) · [Production runner rollout](../../../.github/workflows/release.yml)

5. Promote the qualified image digest with the production application file.
   Enable or disable resume by redeploying that digest with the desired file.
   Confirm the flag on every replica after the rolling update.

   Resumed leases already in flight can settle.
   Roll file and image together when the settings schema changes.

The loader and policy wiring are under verification.
The shared crate manifest has no daemon or runner dependencies.
Static checks confirmed file delivery, matching readiness ports and the
initial false value in both Fly environments.

Environment-only startup defaults to resume enabled. The deployment files
explicitly disable it for the staged rollout. Every replica must load the
selected value before rollout verification.

Focused verification recorded seven shared-loader tests and 94 daemon
library tests passing, with one ignored. Three live recovery checks and four
query-plan checks passed on the owned PostgreSQL rig.

The follow-up live run passed all three rollout checks, including compatible
other-fleet progress, and 26 runner-plane tests together. Eight CLI checks
passed, including spawned daemon startup from the selected file and explicit
port precedence.

The fold-barrier regression and populated lease-row check passed after their
fixtures were updated for the immutable generation and setup witnesses.
Global cordon fault tests now use isolated databases so their sweeps cannot
encounter another test's intentional trigger refusal.

Further checks cover the latest clock-edge unit cases, per-file coverage and
the full repository gates. Deployment drills and a matched burst benchmark
with the real dev connection budget remain due.

Lint fixes to the benchmark harness followed the recorded measurements.
Those results remain tied to `976883d00`; this configuration batch has no new
matched burst result yet.

### Integration with the merged M246 base

M246 merged as `813ccba56dd1dbfe795992744a8348e2ff700c3c` after the focused
configuration checks. The pre-merge `make lint-all` passed. The following
`make test-unit-all` was stopped during its runner build so the branch could
incorporate the updated base.

The merge preserves tenant ownership and model-key placement checks alongside
M247 hold accounting, quarantine, checkpoint custody and bounded recovery.
It also retains the shared waiting probe's idle-tenant filtering. M247's
unpublished checkpoint error codes move after the three codes already on main.

The merged workspace passed `cargo check --locked --workspace --all-features
--all-targets`. Eighteen scheduler unit checks passed, including the bounded
500-claim case and main's idle-tenant probe tests.

The repository pins Orly 0.60.1. Updating the local installation from 0.50.0
restored a green `orly doctor` with the new burst configuration keys.

Post-merge `make lint-all` passed. The focused live run passed three rollout
checks, eight tenant-placement checks, four query-plan checks, 28 runner-plane
checks and eight CLI checks.

The rollout check also proves the coarse waiting
probe leaves a checkpointed head parked while resume is disabled. The query
checks include the 20,000-idle-fleet case.

Seven shared-loader tests and 95 daemon library tests passed, with one ignored.
Full repository tests, final acceptance and coverage remain due. Earlier
results describe their recorded snapshots.

### Deployment job environment inventory

Indy's deploy-job environment list was checked against
`.github/workflows/deploy-dev-fly.yml`. The application file covers daemon
policy, while the job also configures the collector, NATS and the image rollout.

| Job input | Delivery or configuration owner |
| --- | --- |
| `OTLP_COLLECTOR_APP`, `OTLP_COLLECTOR_DIR`, `OTLP_COLLECTOR_MACHINES` | Deployment workflow selects and sizes the collector app. |
| `NATS_APP`, `NATS_BUILD_CONTEXT`, `NATS_MACHINES` | Deployment workflow selects, builds and sizes NATS. |
| `OTLP_COLLECTOR_ENDPOINT` | `telemetry.otlp_endpoint` supplies the daemon destination. The current workflow also stages `OTEL_EXPORTER_OTLP_ENDPOINT`. |
| `FLY_API_TOKEN` | Deployment-job credential for Fly operations. |
| `DATABASE_URL_API` | Daemon secret environment input. |
| `DATABASE_URL_MIGRATOR` | Release-command secret environment input; migration uses its existing environment reader. |
| `GRAFANA_OTLP_ENDPOINT` | Collector upstream secret input. |
| `GRAFANA_OTLP_INSTANCE_ID`, `GRAFANA_OTLP_API_KEY` | Collector authentication inputs. The workflow also derives the daemon's masked `OTEL_EXPORTER_OTLP_HEADERS` value. |
| `POSTHOG_API_KEY` | Daemon secret environment input; `telemetry.posthog_host` may supply its public endpoint. |
| `OIDC_ISSUER` | Existing environment fallback when `identity.issuer` is absent from TOML. |
| `CLERK_SECRET_KEY`, `CLERK_WEBHOOK_SECRET` | Daemon secret environment inputs. |
| `RUNNER_STREAM_CERTIFICATE_CHAIN`, `RUNNER_STREAM_PRIVATE_KEY` | Workflow derives the two `RUNNER_STREAM_TLS_*_BASE64` Fly file secrets. TOML supplies their mounted file paths. |
| `NATS_URL` | Credential-bearing input staged for both NATS and the daemon. |
| `ENCRYPTION_MASTER_KEY`, `AUTH_SESSION_CODE_PEPPER` | Daemon secret environment inputs. |
| `OP_MANAGED_VARIABLES` | Secret-loading action metadata within the job. |
| `CANDIDATE_IMAGE` | Qualified immutable image digest selected by the deployment workflow. The image must include the loader for its application file schema. |

TOML currently overrides the public log level, identity audience and telemetry
endpoint that the workflow also stages as Fly secrets. Removing duplicate
staging remains a deployment cleanup item. Live secret removal or deployment
has not run.

## Manual two-runner drill

Use a disposable fleet and two enrolled platform runners on the same development daemon. Both runners need compatible policies and funded model access.

A file-backed check also needs a verified sandbox and private object storage. [Development setup](../../development.md#shared-development-worktrees)

1. Use `agentsfleet platform runners list` and `agentsfleet --json platform runners show <RUNNER_ID>` to check that both intended runners are healthy. Confirm which disposable container belongs to runner A.
2. Send the prepared recovery task through `agentsfleet fleet steer <FLEET_ID> "<fixture prompt>"`. Keep its controlled second action waiting until the `respawn_checkpoint_committed` receipt proves a usable generation.
3. Read A's expiry with `agentsfleet --json platform runners leases <RUNNER_A_ID> --fleet <FLEET_ID>`. Pause the owned source container with `docker pause <RUNNER_A_CONTAINER>`. Keep the original reply open.
4. Read B's leases. Require the same event, a greater fence issued at or after expiry, and `respawn_checkpoint_restored` for the exact generation before the next model or tool call. Check the marker file for the file-backed case.
5. After the offline sweep, inspect A with `agentsfleet platform runners show <RUNNER_A_ID>` and `agentsfleet platform runners events <RUNNER_A_ID>`. Use `docker unpause <RUNNER_A_CONTAINER>` to restore the lab. Watch for one automatic lift after continuous healthy heartbeats.

The operator drill still needs a recorded human run. The automated owned-rig command covers controlled API-only process failure.

[Rig command](../../architecture/runner_checkpoint_recovery.md#operator-and-lab-checks)

## Historical drafts

Commit `976883d00` preserves the original review walkthrough, full initial file inventory, and detailed manual fixture instructions. Use Git to inspect the draft alongside the implementation and measurement evidence.

```bash
git show 976883d00:docs/v2/reviews/m247-implementation-and-manual-check.md
git show 976883d00:docs/v2/reviews/m247-files-and-symbols.md
git show 976883d00:docs/v2/reviews/m247-scalability-and-refactor.md
git show 976883d00:docs/v2/reviews/m247-two-host-walkthrough.md
```
