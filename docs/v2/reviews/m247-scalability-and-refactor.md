---
type: explanation
audience: contributor
verified: 2026-11-18
product_version: 0.78.0
executable: false
---

# M247: recovery bursts, architecture and performance

This review extends the [active specification](../active/M247_001_P1_API_CLI_DOCS_OBS_UI_FAILED_HOSTS_RESPAWN_FROM_CHECKPOINT.md) to correlated failures affecting hundreds of platform fleets.
Indy requested a fresh review after selecting 6.2 Sol Max and explicitly allowed a different, larger implementation.
Tenant runners remain parked by his instruction: "I think complete M247 for platform runners, park tenant runners", followed by "go".

Indy also requires "99% above coverage on all the patch files you touched" and performance evidence.
The delivery criterion is more than 99% line coverage for every changed production source file, plus changed-line coverage, with explicit accounting for files that contain no instrumentable code.
SQL changes require real database lifecycle and contention tests.

## Decision

Indy requested an adversarial review on 6.2 Astra XHigh before implementation. This decision supersedes the proposed interfaces saved at the frozen production baseline `d4823563e80829b97589846a1c7997964fdf0274`. The burst performance baseline is pending.

Use bounded round-robin among eligible hosts for recovery and oldest eligible admission first in Postgres. Preserve ordinary admission scheduling and its locality and ring budget. Keep checkpoint ownership and adoption in the existing task and turn implementations. Share the physical transfer code and its resource limits. The changes below specify what to implement and how to distinguish a speedup from issuing more work than the runner can restore.

## What the source review found

All figures in this section are calculations from the reviewed source, not measurements.
The implementation reviewed is `d4823563e80829b97589846a1c7997964fdf0274`, based on `97224ed7432c9dd124ea8797d4324156b7022c07`. Source paths are repository-relative.

| Finding | Effect during a recovery burst | Source |
|---|---|---|
| Generation delta becomes an issue budget | An otherwise quiet replica spends one successful fallback claim per five-second tick, even with idle compatible hosts | `afd_gateway/src/scheduler/mod.rs:38,54`; `scheduler/recovery/mod.rs:24`; `session/supervisor.rs:107` |
| Recovery ordering is consumed after a provisional claim | Increasing that budget alone loses recovery's oldest-first ordering before the whole drain completes | `afd_fleet/src/lease/assign/ledger.rs:82,109` |
| Ordinary missing chunks upload serially | Every chunk pays singleton authorization and authenticated readback; sandbox freeze includes the entire process | `afr_supervisor/src/recovery/turn/capture.rs:137`; `turn/session.rs:156` |
| Restore batches stop at file boundaries | Many small files never use the four-transfer window and repeatedly pay native/SQL/vault overhead | `afr_supervisor/src/recovery/turn/files.rs:53,144,159`; `recovery/session.rs:232` |
| Restore ordering duplicates plaintext | Four distinct eight-MiB chunks can temporarily retain about 64 MiB of plaintext rather than one 32-MiB window | `turn/files.rs:181`; `recovery/session.rs:323` |
| Transport clients and limits belong to individual sessions | Concurrent sessions multiply buffers, TLS pools, disk jobs and requests without a runner-wide recovery budget | `recovery/transfer.rs:20`; `turn/session.rs:49` |
| Every checkpoint transaction takes an exclusive host lock | Independent fleets on one host queue behind each other, and can delay heartbeat/renewal work | `afd_fleet/src/lease/turn_checkpoint/guard.rs:60`; `afd_runner/src/capacity/sql/lock_host.sql:6` |
| Pure restore authorization repeats quota work | Hosts in the same workspace contend on a quota row even when no storage accounting changes | `afd_fleet/src/lease/turn_checkpoint/transfer.rs:139` |
| Physical verification shares a five-second native deadline | A four-chunk upload readback can require authenticating about 32 MiB within five seconds, despite a longer inner storage timeout | `afr_supervisor/src/client/stream.rs:31`; `afd_gateway/src/session/dispatch/turn_checkpoint.rs:20`; `turn_checkpoint/signing.rs:40` |
| An owner failure aborts storage maintenance | A contended oldest head can prevent later owners, identities, orphan collection and purges from progressing | `turn_checkpoint/maintenance.rs:35`; `task/checkpoint/maintenance/mod.rs:22` |
| Expiring reports do not serialize future holds with claims | A report delayed after its statement can commit a hold after another fleet has consumed the expired lease's capacity | `afd_fleet/src/lease/sql/commit_report.sql:181`; `lease/commit/mod.rs:243` |

Paths beginning `afd_` are under `rustd/crates/daemon/`; `afr_` paths are under `rustd/crates/runner/`.
These findings describe code and interleavings, not measured deadlocks or production incidents.

### Dispatch calculation

Assume leases have expired, compatible spare credits already exist, claims succeed, and there are no new admission or Ready wakes.
The last fallback offer on one replica occurs roughly `5 * (N - 1)` to `5 * N` seconds after eligibility, depending on tick alignment.
Lease expiry, SQL, storage and model latency add to that interval.

| Interrupted fleets | One replica: last fallback offer | Four replicas: ideal division |
|---:|---:|---:|
| 100 | 495–500 seconds | 120–125 seconds |
| 500 | 2,495–2,500 seconds | 620–625 seconds |

Four replicas is an illustration, not an observed deployment.
Each runner supports at most 64 assigned workers, so 100 or 500 simultaneously active restores need at least two or eight compatible hosts at that maximum assignment.
Actual spare workers, memory and storage determine admissible concurrency.

### Transfer calculation

For 128 distinct, missing eight-MiB chunks and one manifest page, current ordinary capture requires 260 native commands.
The existing task-style byte-bounded upload batches need 68 for the same work: 192 fewer commands, about 73.8%.
Physical PUTs and authenticated readback remain necessary.

For one 1-GiB file, current restore requires 36 checkpoint RPCs and 68 guarded transactions before retries, renewal, folds and normal issue work.
One hundred such restores therefore require 3,600 RPCs and 6,800 transactions; five hundred require 18,000 and 34,000.
These are manual successful-path counts that the benchmark must verify with traces.

Four concurrent eight-MiB chunks across 500 active restores represent 15.625 GiB of payload across the runner pool.
The current map-to-result clone can double that payload temporarily, before manifests, roots, TLS and allocator overhead.
The existing native frame budget does not account for these object bodies.

Small-file work must be measured separately from bytes.
For 200,000 one-byte files, dedup allows at most 256 distinct objects, but the current per-file restore can still repeat approximately 200,000 authorizations, GETs and file syncs.
A byte quota alone does not bound that metadata and operation cost.

## Adversarial review and implementation decisions

### Schedule recovery with bounded round-robin

An ordinary ring carries a fleet hint. M243 §4.4 limits a pass covering K rings to K issued leases and prefers a host already holding that fleet. Keep the existing ordinary pass and its unknown-outcome behavior. A positive recovery probe or a lost-notification signal has no reliable count. Only that case starts a recovery drain. Account for hints arriving during the drain and consume only its own recovery generation.

Each replica takes the current eligible hosts and their advertised credits. Start in registry order. Give each host one claim at a time, cap the number of claims in flight across hosts, and put a successful host with remaining credit at the back. A completion opens a place immediately. A slow host consumes one place and independent completions can keep making progress. Use a local queue and a bounded unordered set of futures inside the existing scheduler. The session supervisor already owns each offer, and Postgres still decides eligibility and capacity. An equal quantum of one fits the decision being scheduled, one lease. Weighted or deficit round-robin would require a reliable cost or service weight per lease. Actual file sizes and throughput are handled at the runner.

Limit a drain to its starting credits per host, including in-flight claims. End a host's turn on empty, cancellation, supersession or uncertain issue. A timeout can follow a committed grant. Defer that host to a fresh scheduling signal and let its session and durable lease reconcile. Limit attempts and candidate page work as well as successes. Poll cancellation and yield between bounded batches. Retain newer wake generations. When all attempts return empty, wait for the next wake or probe. Keep the five-second negative probe independent of host count. Test a slow host both inside and outside the first concurrency window. All occupied places being slow can delay a queued host up to a claim deadline.

```text
ordinary ring -> existing locality pass -> at most one issue per ring
recovery hint -> A, B, C ready -> one claim per host, up to the cap
                   A waits        B issues -> B to tail if credit remains
                                  C issues -> C to tail if credit remains
                                      |
                     Postgres checks eligible head, fence and host capacity
                                      |
                           lease owner renews while restore waits
                                      |
                      verified root + files -> adopt -> thaw -> model
```

**A fair host queue alone cannot guarantee a claim.** Recovery selects at most 64 candidates in a fixed oldest-first order. A separate statement then tries slot and admission locks with `SKIP LOCKED`. With the oldest 64 slots held, every concurrent host can exhaust the same page while fleet 65 is free. An empty page of attempts means only that those candidates failed. Add a bounded recovery-only keyset scan using the admission's existing sort key (creation time, admission ID). Advance past an exhausted page, preserve progress when the scan yields and wrap to retry old rows. Retain the first unsettled admission within each fleet and each host's compatibility filters. Never skip a locked head to execute that fleet's next message. Keep the scan position disposable. Measure the candidate plan with idle fleets in the database as well as pending fleets. A LIMIT after materialization and sorting bounds returned rows, not database work.

**Evidence.** Two paused-clock scheduler regressions failed on the baseline. One wake left 9 of 10 fleets waiting, and a blocked host delayed a healthy host. A separate 65-row experiment on PostgreSQL 19rc2 locked rows 1–64. Skipping locked rows within the selected page returned zero, while the next page returned row 65. This tested the database mechanism on a disposable table. The product regression must hold 64 real fleet slots, offer fleet 65, release the locks, and show the old work is retried. Also exercise two competing replicas, an ordinary ring during recovery, and a negative probe with many idle hosts.

### Reuse existing owners for transfer and cleanup

The worker pool already shares one `Arc<Lessee>` across workers. Put a reusable `Transfers` client and admission limits there. Keep the existing task and turn sessions. Extract the byte- and count-bounded batch builder and authenticated download/application helpers at their two call sites. A generic async function taking a typed callback suffices where the only difference is obtaining authorization. Introduce a new trait only if those call sites demonstrate a larger stable contract. Task base plus overlay and effect watermarks differ from a turn's full tree and fold adoption. Each family continues to own its pins, scope and publication.

Start with a cap on admitted file operations and a fixed maximum window per operation. Bound the window by unique ciphertext and plaintext bytes, entries and concurrent transfers. Their product gives the per-runner payload ceiling. Account separately for manifest memory, directory entries, buffers held during decryption and queued snapshots. Avoid holding one resource permit while waiting indefinitely for another. Pass shared zeroizing chunk owners through a window, including repeated references, and bound open file writers. Use the existing capture boundary owner and wait for started blocking writers before thaw or teardown. A permit moved into the writer stays charged until the writer exits. Reuse the existing maintenance cursor pattern in `task/decision/maintenance.rs` for bounded scans, fair wraparound and per-owner errors.

Try admission for optional capture before cloning history or freezing. Without resources the boundary records a typed skip, retaining the last committed generation. With no prior generation the original input and unsettled folds are still the recovery source. Required restore and required task initialization need cancellable admission while renewal progresses. Acquire pin and expiring URLs close to use. Keep a queued restore eligible for the next available place even under a stream of optional captures. Limit waiting workers, measure time to the first resumed model request, and verify that losing authority while queued permits no adoption or model call.

### Serialize only conflicting database changes

Both fleet issue and authoring issue acquire the host row exclusively and then read occupancy in a fresh statement. A report converting its own live lease into at most one future hold needs to conflict with **issue**. Reports on distinct slots can share the host lock. Use the existing batched renewal's `FOR SHARE` pattern for the hold-producing path and current-connection checkpoint authority. Preserve parent → host → slot → lease order, recheck timing after locks, and hold the guard through commit. Prove credential rotation and supersession still fence these calls. Preserve existing acknowledgement, late settlement and billing behavior if a hold cannot be accepted.

In the PostgreSQL 19rc2 experiment, two transactions obtained shared host locks concurrently. Exclusive issue and a non-key host update both received lock error `55P03` while a simulated hold was uncommitted. After commit, an exclusive claimant's fresh capacity statement saw the hold. Repeat both orderings with the actual report and competing fleet/authoring issue, including the case where expiry occurs before the report commits. Row-lock compatibility alone does not verify application capacity accounting.

For read authorization with zero verification receipts, omit quota locking and accounting writes. Keep the second custody check after signing or remote verification. For upload readback, bound admission before slow work fills all eight Reports slots. Propagate an action-specific absolute deadline to both ends, within custody expiry. Reserve time for a typed reply and test with a concurrent terminal report and renewal. A client timeout currently cancels the whole physical session. Keep metadata deadlines short. Measure Commit with the maximum 64 pages. If validation delays renewal or blocks a host update, validate immutable page bytes outside the long transaction and recheck the exact ordered page IDs and digests, pin, fence, folds and expected head under the publication lock. A concurrent abandon, head change or expired pin must prevent publication. Defer broader guard consolidation until measurements identify remaining cost.

## Implementation map

| Existing files under `rustd/crates/` | Intended change |
|---|---|
| `daemon/afd_gateway/src/scheduler/mod.rs`, `session/offers.rs`, `session/supervisor.rs`, `daemon/afd_bus/src/wake/` | Keep ordinary `pass`. Add recovery-only round-robin and scoped epoch completion using the existing session owner. |
| `daemon/afd_fleet/src/lease/assign/ledger.rs`, `lease/sql/lease/select_ledger_candidates.sql` | Keep recovery order across a productive drain. Return the oldest-first sort key and advance a bounded scan after an unclaimable prefix. |
| `runner/afr_supervisor/src/lease_loop/`, `runner/afr_agent/src/loop/recovery_boundary.rs` | Acquire admission at the boundary before allocating the snapshot. Carry it through existing freeze, restore, cleanup and renewal ownership. |
| `runner/afr_supervisor/src/recovery/{transfer,capture,session,restore}.rs` and `recovery/turn/` | Reuse transport, batch missing uploads and window reads across files. Retain separate `Session` and `TurnSession` lifecycles. |
| `daemon/afd_runner/src/capacity/`, `daemon/afd_fleet/src/lease/{commit,turn_checkpoint}/`, `lease/sql/commit_report.sql` | Shared authority guard before child locks for holds/checkpoints, fresh exclusive issue census, receipt-only quota writes, validation under current custody. |
| `runner/afr_supervisor/src/client/`, `daemon/afd_gateway/src/session/dispatch/turn_checkpoint.rs` | Bound bulk verification concurrency and answer deadlines while progress, settlement and renewal remain available. |
| `daemon/afd_fleet/src/{lease/turn_checkpoint,task/checkpoint}/` and the two new recovery/head migrations | Create heads on demand, add FK indexes, use bounded indexed maintenance progress and isolate hot owners. |
| Existing lease benchmark, production native stream and process crash fixtures | Drive the normal scheduler, measure restore and settlement, record both revisions and keep the workload identical. |

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

The completed pre-refactor unit run has 5,702 passing Rust tests, 4,659 app tests, 2,377 CLI tests plus 22 skips, 133 website tests and 516 design-system tests.
App coverage reports 100% statements, branches, functions and lines across 529 measured files; the changed app source files are each 100%.
CLI function and line coverage are 100%; Rust per-file instrumented coverage and burst timings have not yet been measured on the refactor.

The real one-runner process crash drill passed before this refactor, using a controlled model transport and API-only work.
It is not proof of file-backed supervisor restoration or hundreds of simultaneous recoveries.
The missing production-entry, connection-race, restart-publication and lifecycle proofs remain required.

## Fingerprints and avoiding repeated work

Retain revision and source/test/fixture/command/toolchain fingerprints with review, coverage and benchmark results.
A result can be reused only for the inputs it actually covered; changed work invalidates its affected evidence.
Keep unresolved findings and completed actions in one ledger rather than rediscovering or silently discarding them.

Gstack review bindings already distinguish current source from changed source.
This repository currently declares no reviewed Orly `reuse` policies, and standalone Make logs are not Orly completion records.
`orly gate pr --reuse-verification` can reuse only explicit hermetic policies with valid engine evidence; the final gate must state which commands actually ran or were reused.

No gate, scanner suppression, threshold or verification policy is weakened by this review.
The public documentation companion is [docs PR 254](https://github.com/agentsfleet/docs/pull/254); the implementation PR remains unpublished while these changes and proofs are completed.
