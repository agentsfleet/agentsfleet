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

Refactor recovery scheduling and the shared file pipeline before publishing the implementation PR.
Per-session limits are insufficient for a burst, and the fallback scheduler currently budgets one issued fleet per five-second tick per daemon.
Retain Postgres as durable authority, core NATS as a disposable wake mechanism, encrypted roots, immutable file objects, exact attempt fences, and restore before model execution.

Use a shared runner recovery runtime with separate task and ordinary-turn authority adapters.
Share batching, transport, resource accounting and safe file application; keep each family's ownership, compatibility, input adoption and publication rules explicit.
Additional traits alone do not supply capacity, fairness or throughput.

The design below is the selected implementation direction; it is not a claim that the refactor or burst benchmark has already passed.
The source review found no basis for a successful-recovery latency guarantee without spare compatible workers, storage bandwidth and healthy dependencies.

## What the source review found

All figures in this section are calculations from the reviewed source, not measurements.
The implementation base is `97224ed7432c9dd124ea8797d4324156b7022c07`; source paths are repository-relative.

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

## Proposed ownership and execution

```text
Postgres admissions + leases + checkpoint heads
                    |
   coarse wake / five-second lost-wake probe
                    v
 RecoveryDrain(epoch, bounded round, fair host rotation)
                    |
       fenced issue under real capacity
                    v
 Runner lease owner starts renewal immediately
                    |
       acquire shared restore admission
                    v
 RecoveryRuntime (one per runner process)
  + RecoveryBudget: bytes / transfers / blocking jobs
  + RecoveryJobs: owned work, cancellation and drain
  + shared authenticated HTTP transport
                    |
       RecoverySession<family control>
          /                       \
 Task authority                Turn authority
 base + overlay                complete files + root
 task/effect cursors            exact fold membership
          \                       /
    bounded transfer stream + safe file application
                    |
 validate -> adopt -> release pin -> thaw -> model
```

### Fair recovery drain

Treat a wake as evidence that a bounded drain may help, rather than as a count of work items.
Carry a recovery epoch through the drain so the first provisional claim does not consume the ordering witness.
Keep the one coarse probe per tick distinct from actual claim/issue operations.

Rotate among compatible sessions, issue one offer at a time per session, and allow bounded concurrency across different hosts.
Stop or yield when no eligible work or credits remain, cancellation arrives, or a round reaches its work/time budget.
Continue a productive bounded drain without waiting another five seconds per lease, while preserving admission gates and authoritative Postgres fences.

### Shared recovery kernel

`RecoveryRuntime` owns the reusable transfer client, process-wide budgets and tracked blocking work.
`RecoverySession<C>` owns one family control adapter, exact pin state, cancellation and bounded progress.
Use a `RecoveryControl` trait for the two real implementations; a trait object is useful only where runtime engine selection needs type erasure.

The control adapter provides native registration, upload authorization, read authorization and custody renewal.
The common engine performs byte-bounded batches and concurrent physical I/O, keyed by immutable object identity.
It restores order at the boundary and shares repeated chunk bodies through bounded references instead of copying them.

Batch transfers across files while retaining path, executable-mode, ownership, link and whole-manifest validation.
A blocking job receives owned, bounded work and releases its permits only when the job actually completes.
No blocking thread waits for a network body, and no caller cancellation silently detaches a live file writer.

Acquire continuation-capture admission before freezing the sandbox.
A busy capture can retain the old committed checkpoint through the existing explicit skip result.
Required restore waits with lease renewal active, and obtains its reader pin and short-lived transfer URLs close to use.

### Short metadata and bulk verification

Keep metadata operations short, and classify bulk verification separately from settlement and custody maintenance.
Admission must bound active verification work before it occupies the shared native request capacity.
An action-specific bounded verification deadline is a smaller first implementation than a new durable verification service; benchmark it before adding another lifecycle.

Use shared host authority locks for checkpoint operations that do not mutate physical capacity, with current-connection and fresh-clock checks retained after waits.
Capacity-changing issue and hold-producing reports require the exclusive host lock before slot/lease locks through commit.
Pure read authorization must not update quota when it has no accounting receipts.

Retain family-specific SQL guards and publication transactions.
Compact repeated authority reads only with controlled races proving that session, fence, plan and pin expiry cannot cross the boundary.
Bulk page validation must not hold an exclusive host lock for dozens of page round trips and large JSON hashing.

### Independent cleanup

Use indexed, bounded keyset progress and advance the cursor before a contended owner is attempted.
Record per-owner deferrals and continue unrelated maintenance stages.
Avoid creating recovery metadata for every offer that has no checkpoint, and preserve task/turn references, active reader pins and exactly-once physical quota release.

## Concrete implementation map

Paths are relative to `rustd/crates/` unless prefixed with `schema/` or `docs/`.
Names below are proposed interfaces, not symbols claimed to exist already.

| Files / modules | Methods and types | Purpose |
|---|---|---|
| `daemon/afd_gateway/src/scheduler/{mod.rs,recovery/mod.rs}` | `RecoveryDrain`, `DrainRound`, bounded async pass | Fill available recovery capacity from one coarse wake |
| `daemon/afd_bus/src/wake/{mod.rs,queue.rs}` | recovery epoch lifecycle | Keep recovery ordering until a round completes |
| `daemon/afd_gateway/src/session/{supervisor.rs,events.rs}` | offer cause and per-session claim ownership | Concurrent hosts, one offer per physical session |
| `daemon/afd_fleet/src/lease/assign/{mod.rs,ledger.rs}` | explicit selection context and continuation | Separate wake hints from authoritative claims |
| `runner/afr_supervisor/src/recovery/{mod.rs,runtime.rs,budget.rs,jobs.rs}` | `RecoveryRuntime`, `RecoveryBudget`, `RecoveryJobs` | Shared clients, limits and structured cleanup |
| `runner/afr_supervisor/src/recovery/{control.rs,session.rs}` | `RecoveryControl`, `RecoverySession<C>`; task/turn adapters | One bounded file pipeline with distinct owners |
| `runner/afr_supervisor/src/recovery/{capture.rs,restore.rs,transfer.rs}` and `turn/` | batch upload/download, keyed chunk ownership | Remove singleton transfers and plaintext copies |
| `runner/afr_supervisor/src/{lease_loop,worker_pool,shutdown}/` | recovery admission and runtime drain | Preserve renewal, restore barrier and safe teardown |
| `daemon/afd_runner/src/capacity/{mod.rs,sql/}` | separate shared authority / exclusive capacity acquisition | Reduce host lock convoys without weakening fences |
| `daemon/afd_fleet/src/lease/{commit,turn_checkpoint}/` | report host serialization; bounded custody/transfer sections | Fix future-hold race and shorten locked work |
| `daemon/afd_fleet/src/{task/checkpoint,lease/turn_checkpoint}/` | shared physical transfer service, family custody adapters | Share verification and accounting, retain identity rules |
| `daemon/afd_fleet/src/lease/turn_checkpoint/maintenance.rs` and SQL | keyset scan, cursor, deferred outcomes | Bound scan work and isolate hot owners |
| `schema/20261117154626_runner_turn_recovery.sql`, `schema/20261117154627_runner_turn_checkpoints.sql` | supporting head/fleet foreign-key indexes | Avoid unrelated-head scans during purge |
| Existing `daemon/afd_bench/` lease lane and recovery integration fixtures | burst shape and measured stage results | Compare dispatch, API-only and file recovery workloads |
| `docs/v2/reviews/m247-files-and-symbols.md` | regenerate actual declaration inventory | Replace planned names with the final diff inventory |

All raw configuration and wire bounds use Garde; materialization consumes validated inputs.
Semantic constructors still prove exact object membership, immutable ciphertext, whole-tree link safety and lifecycle transitions.
Task/turn ownership must not be represented by fake IDs or a generic repository escape hatch.

## Performance and coverage acceptance

Compare the frozen pre-refactor revision with the final revision on the same owned local infrastructure.
Use 1, 10, 100 and 500 interrupted fleets, declare compatible source/replacement worker counts, and use multiple failed hosts where the 64-worker limit requires them.
Separate API-only roots, four small files, large multi-chunk files and metadata-heavy manifests.

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
