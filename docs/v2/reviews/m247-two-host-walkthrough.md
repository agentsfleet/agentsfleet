---
type: explanation
audience: contributor
verified: 2026-11-18
product_version: 0.78.0
executable: false
---

# Sixteen fleets lose their host

Assume two platform hosts, one runner per host, 16 workers per runner, and one daemon replica. A owns one active turn for each of fleets F01 through F16. B is healthy, compatible, already connected, and has advertised 16 free workers. Each fleet has a committed checkpoint. For the file examples they run in the same workspace. Labels in the diagrams mean **KEEP** for an existing path, **EDIT** for an existing implementation that will change, and **NEW** for a proposed file or method. The diagrams describe the frozen baseline `d4823563e8` and the proposed fixes. The timing examples are calculations, pending the benchmark.

```text
                       Postgres                         private object store
              admission, lease + fence,                encrypted file chunks
              committed root + manifest                  (file runs only)
                          ^                                    ^
                          |       safe boundary on A           |
 A [01][02][03][04][05][06][07][08][09][10][11][12][13][14][15][16]
 B [..][..][..][..][..][..][..][..][..][..][..][..][..][..][..][..]

 A crashes -> its workers and local disks can all disappear
              |
              +-> old lease still live? Wait until its own durable deadline.
              |   Each lease normally gets 35 s from its last grant or renewal.
              |
              +-> old lease expires -> compatible B can claim a higher fence
              |   -> start renewal -> restore exact committed generation
              |   -> verify root and files -> adopt covered inputs -> thaw
              |   -> first resumed model call -> one fenced settlement
              |
              +-> host silent for more than 105 s -> liveness sweep cordons A
```

**Why 35 seconds?** M243 raised the lease from 30 to 35 s in `2bfdc2190b` after a paused-clock test showed a healthy runner could enter cleanup before its renewal reply. The original calculation allowed 5 s to the next tick, 5 s for the failed answer, 2 s of jitter and 7 s to reconnect. That left 16 s, enough for the 15 s renewal window. Within that window, a second answer has 5 s and physical cleanup has 9 s. Today's compile-time checks use `5 + 5 + 7 + 15 = 32 < 35` and `5 + 9 = 14 < 15`. The historical 2 s jitter allowance is no longer a separate term in the first check. [M243 decision](../done/M243_001_P1_API_DOCS_INFRA_OBS_RUNNERS_HOLD_ONE_PUSH_STREAM.md#renewal-recovery-budget-correction) · [Lease timing](../../../rustd/crates/foundation/af_core/src/timing.rs) · [Runner cleanup margin](../../../rustd/crates/runner/afr_supervisor/src/renew/mod.rs)

**Why 105 seconds?** The existing host-health policy tolerates silence for three lease lifetimes, or `3 × 35 = 105 s`. An idle runner beats every 10 s. The longer threshold gives a host time to reconnect before the daemon cordons it. For a host that stays down, the liveness sweep runs every 10 s and rechecks the last-seen time under lock. [Liveness sweep](../../../rustd/crates/daemon/afd_runner/src/sweep/liveness/mod.rs) · [Cordon statement](../../../rustd/crates/daemon/afd_runner/src/sql/sweep/cordon_silent_runner.sql)

```text
t=0     A heartbeats and F01's lease is renewed
t=3     A loses power
t=35    F01's lease reaches expiry. B may now compete for F01 (32 s after the crash).
t=110   A sweep at this instant sees 110 s of silence and cordons A.
```

The example assumes sweep ticks at multiples of 10 s. Claim, restore and first resumed model call each take additional time. B can start recovery while A still appears uncordoned.

Expiry can differ across the 16 leases. With no committed generation, recovery starts from the admitted input and outstanding folds. An advertised committed generation that fails validation stops before model/tools. If B already has 12 occupied workers and no other reservations, at most four interrupted turns can start immediately. A better scheduler cannot create spare workers.

## 1. A recovery wake only buys one successful offer

```text
BEFORE   positive probe -> wake generation +1 -> issue budget 1
         B workers       [F01][..][..] ... [..]       15 free
         next 5 s probe  [F01][F02][..] ... [..]       14 free
         ...
         probe 16        [F01][F02][F03] ... [F16]

AFTER    positive probe -> queue B -> offer F01 -> B has credit -> queue B
                                  -> offer F02 -> B has credit -> queue B
                                  -> ... -> last credit used -> stop
```

With all 16 simultaneously eligible, no new wake or credit increases, successful claims, and no completions, the last baseline offer waits **75–80 seconds after eligibility** for probe opportunities alone. In the proposal B still issues one offer at a time. It can immediately schedule the next one. With two eligible survivors, each gets one claim per turn and a slow claim occupies only one concurrency slot.

**Can an event wake the scheduler?** Ordinary admissions already publish a Core NATS wake. A runner can also announce new capacity with `Ready`. On a quiet replica, A can vanish while B already has 16 free workers. Crossing a lease deadline changes no row and publishes no event. A daemon can also die between committing an admission and publishing its wake. The existing backstop costs one coarse Postgres query per daemon replica every 5 s, independent of worker count. A positive result starts the proposed drain. [Recovery query](../../../rustd/crates/daemon/afd_gateway/src/sql/select_waiting_admission.sql)

```text
M247       admission wake / new capacity -> offer immediately
           silent expiry / lost wake     -> five-second probe -> drain free credits
Alternative for a scheduler without a periodic probe
           issue or renew -> record the deadline -> arm timer for earliest deadline
           timer fires -> reread Postgres -> claim only if the current lease expired
           commit work and its wake record together -> deliver and retry from Postgres
           restart or listener gap -> rebuild timers and catch up on pending work
```

The timer alternative also needs a durable way to wake a sleeping scheduler for an earlier deadline or newly ready capacity. A stale timer is harmless if the claim checks the current deadline and fence. A lost timer or admission can otherwise strand F01 indefinitely. M247 keeps the existing backstop and fixes the one-success-per-tick limit. [M243 probe decision](../done/M243_001_P1_API_DOCS_INFRA_OBS_RUNNERS_HOLD_ONE_PUSH_STREAM.md#nov-09-recovery-probe-clarification)

**KEEP** `afd_gateway/src/scheduler/recovery/mod.rs::probe_ticks` as the lost-wake backstop. **EDIT** `afd_gateway/src/scheduler/mod.rs::notifications` to distinguish ordinary rings from recovery and keep the ordinary `pass` budget. **NEW** `afd_gateway/src/scheduler/recovery/drain.rs::drain` for bounded round-robin. **EDIT** `session/supervisor.rs::SessionSupervisor::offer` to preserve the trigger through the existing per-session owner.

## 2. Choosing a candidate consumes the oldest-first signal too early

```text
BEFORE   oldest F01 -> provisional slot claimed -> recovery flag cleared
                     -> later gate can still refuse F01
         next claim  -> ordinary hint/locality order can choose F12 before F02

AFTER    recovery epoch E -> choose oldest eligible F01 -> prove capacity/issue
                          -> next claim still uses E -> try oldest eligible
         end of drain -> acknowledge E only -> a newer epoch stays pending
```

The provisional slot can be committed before a lease is issued. Grants on different hosts can finish out of order. The ordering rule applies to **selection**. With a larger backlog, keep a cursor so 64 locked candidates cannot hide eligible candidate 65. On wrap, retry the older rows and keep each fleet's own admission order.

**EDIT** `afd_fleet/src/lease/assign/ledger.rs::Leases::ledger_counted` and `lease/sql/lease/select_ledger_candidates.sql` for recovery ordering and keyset progress. **EDIT** `afd_bus/src/wake/mod.rs` and scheduler `notifications` for generation ownership.

## 3. Uploading missing chunks keeps a fleet frozen

```text
F01 on A has committed generation G7. Four file chunks changed since G7.
Other fleets are already using A's file-transfer capacity.

BEFORE   F01 reaches a safe boundary, its own executor is idle
         -> clone state -> freeze F01's sandbox -> scan and register [a b c d]
         -> authorize a, PUT a, verify a -> repeat for b, c, d -> commit G8 -> thaw
AFTER    F01 tries to reserve a runner-wide file-operation slot before cloning state.
         full -> skip this optional capture -> F01 continues immediately, still at G7
         free -> reserve -> clone -> freeze F01 -> scan and register [a b c d]
              -> one authorization -> up to four PUTs -> one verify request
              -> successful verification and publication of G8 -> thaw -> release
```

Only the full-slot branch avoids freezing F01 in this example. If A crashes before the next successful capture, B restores G7 and repeats eligible work since G7. If G8 commits first, B restores G8. On the admitted branch the freeze keeps file bytes stable during capture. Four distinct missing chunks fitting one batch take eight native upload/verify commands before and two after. Completing the capture still transfers and authenticates four objects. Registration, manifest pages, pin renewals and commit are outside that count. Sandbox freeze duration requires a file-backed measurement. [Boundary owner](../../../rustd/crates/runner/afr_supervisor/src/lease_loop/recovery_capture.rs) · [Baseline upload loop](../../../rustd/crates/runner/afr_supervisor/src/recovery/turn/capture.rs)

**EDIT** `afr_supervisor/src/recovery/turn/capture.rs::TurnSession::capture_files` and `turn/session.rs::TurnSession::upload`. **NEW** bounded grouping helpers in `recovery/window.rs`. **EDIT** `recovery/session.rs::Session::upload_batch` to share transfer mechanics with the task path.

## 4. One small file uses only one transfer slot

```text
BEFORE   B:F01   file a -> authorize [a] -> GET a -> finish a
                 file b -> authorize [b] -> GET b -> finish b ... c, d
AFTER    B:F01   validate entries -> window spans files a, b, c, d
                 authorize unique objects together -> bounded parallel GETs
                 -> write each file in manifest order -> verify/finish files
```

A single large file already uses up to four concurrent chunk reads. Many one-chunk files pay authorization latency individually. Cross-file batching retains path, link, mode and parent checks before model entry.

**EDIT** `afr_supervisor/src/recovery/turn/files.rs::TurnSession::{restore_files,file,download}` and `recovery/restore.rs::Session::restore_files`. **NEW** the shared file-window helper in `recovery/window.rs`.

## 5. Reordering chunks makes a second plaintext copy

```text
After A crashes, B:w01 restores F01's artifact. Its manifest requests [3 1 2 0].
BEFORE   by object ID     [0][1][2][3]    four authenticated 8-MiB buffers
         copy for writer  [3][1][2][0]    four more 8-MiB buffers
         writer appends    3, 1, 2, 0     peak at reorder ~64 MiB
AFTER    by object ID     [0][1][2][3]    four authenticated 8-MiB buffers
         writer handles   [3][1][2][0]    small references to the same buffers
         writer appends    3, 1, 2, 0     payload at reorder 32 MiB
```

The existing restore test serves four encrypted parts over HTTP and checks the bytes of `dir/script` in exactly that order. The test file is 28 bytes. The diagram scales its ordering to four full chunks. At baseline, `TurnSession::download` copies each byte with `to_vec()` while the lookup map still owns the originals. With 16 simultaneous workers in this phase, the plaintext alone can approach 1 GiB. In the proposal each authorized window owns its own buffers. Two references to one chunk can use the same backing bytes. The last consumer releases the memory and its budget. These are byte counts for reordering. Ciphertext, HTTP buffers, manifests and allocator overhead add to process memory. [Restore code](../../../rustd/crates/runner/afr_supervisor/src/recovery/turn/files.rs) · [File-order test](../../../rustd/crates/runner/afr_supervisor/src/recovery/turn/files_tests.rs)

**EDIT** `recovery/turn/files.rs::TurnSession::download` and `recovery/session.rs::Session::download_batch`. **NEW** window-scoped shared owners in `recovery/window.rs`. Reuse stays within the authorized scope.

## 6. Sixteen workers can independently exhaust B's resources

```text
BEFORE   B:w01..w16 each create a client and can each issue four GETs (up to 64).
AFTER    all workers share B's Lessee, one HTTP client and file-operation permits.
         Example with two permits, one window per operation
           permit 1   F01 -> up to four GETs -> ordered write -> release
           permit 2   F02 -> up to four GETs -> ordered write -> release
           waiting    F03..F16 renew their leases. F03 gets the next free permit.
           optional   capture arriving while full skips before freeze
```

**Concrete limits for a benchmark run.** Try two file-operation permits and one window per permit. In this example a window stops at 32 manifest entries, four distinct chunks or 32 MiB of plaintext, whichever comes first. An 8-MiB chunk adds a 16-byte authentication tag when encrypted. Each operation allows up to four simultaneous GETs and one blocking writer. Across B that gives **8 active GETs, 2 writers and 64 MiB of unique plaintext in the two windows**. Each operation finishes writing one window before starting its next. Ciphertext, decryption scratch, HTTP buffers and manifests need separate memory accounting. The permit and entry counts are illustrative. The production settings depend on measured time to first model call, peak memory and renewal delay. [Current chunk size](../../../rustd/crates/runner/afr_supervisor/src/recovery/files.rs) · [Current physical upload ceiling](../../../rustd/crates/runner/afr_supervisor/src/recovery/mod.rs)

Use an owned Tokio semaphore permit on the shared `Lessee`. Required file restores wait in order, alongside lease renewal, and acquire the reader pin and signed URLs close to use. Optional captures try once and yield to waiting restores. A cancelled waiter exits before touching files or entering the model. Once disk work starts, its owner holds the permit through the final blocking write and cleanup. A dropped request cannot give another restore that slot while its writer is still running.

**EDIT** `afr_supervisor/src/lib.rs` startup, `lease_loop/lessee.rs::Lessee`, `recovery/transfer.rs::Transfers::new`, and `recovery/turn/session.rs::TurnSession::new`. **EDIT** `afr_agent/src/loop/recovery_boundary.rs::Harness::save_boundary` for admission before cloning, and `lease_loop/turn_recovery.rs::LeaseRun::prepare_turn_recovery` for admission before freeze.

## 7. Independent fleets queue at B's host row

```text
BEFORE   F01 checkpoint -- exclusive B lock -- release
         F02 checkpoint ........wait........ -- exclusive B lock
         F03 renewal    ........wait........
AFTER    F01 checkpoint -- shared B proof -- locks F01 slot/lease
         F02 checkpoint -- shared B proof -- locks F02 slot/lease
         fleet issue / connection rotation -> conflicts with shared proof
```

Shared checks on distinct fleets may overlap. Mutating the host, including heartbeat updates, can still wait. Timing the locked section and measuring renewal under load are necessary even with shared locks.

**EDIT** `afd_fleet/src/lease/turn_checkpoint/guard.rs::TurnLeaseGuard::lock`. **NEW** shared-authority helper in `afd_runner/src/capacity/mod.rs` and `capacity/sql/share_host.sql`. **KEEP** the exclusive issue lock and the post-wait clock/fence checks.

## 8. Restoring existing files queues on the workspace quota row

```text
F01 and F02 belong to workspace W. W has 10 GiB stored.
B:w01 needs F01's existing object X. B:w02 needs F02's existing object Y.

BEFORE   w01 -> sign GET X -> recheck F01 -> lock W's quota -> move 0 bytes -> commit
         w02 -> sign GET Y -> recheck F02 -> wait for w01 on W -> move 0 bytes -> commit
AFTER    w01 -> sign GET X -> recheck F01's lease, pin and object X -> return URL
         w02 -> sign GET Y -> recheck F02's lease, pin and object Y -> return URL
         W still has 10 GiB stored. The two reads can pass this step together.
```

The quota row counts physical storage for the whole workspace. At baseline, even an empty verification receipt list takes that row and runs an update that can change its timestamp while moving zero bytes. Sixteen fleets in W all hit the same row. In the proposal the daemon still checks custody again after signing. A lease may have expired while the signer ran. Only the accounting step is skipped when there are no receipts. If F03 uploads new data, physical verification produces a receipt. In that transaction the newly verified object's accounted bytes move from reserved to stored once, including on retry. The runner uses a returned URL for the actual GET and authenticates the downloaded bytes. [Transfer confirmation](../../../rustd/crates/daemon/afd_fleet/src/lease/turn_checkpoint/transfer.rs) · [Quota update](../../../rustd/crates/daemon/afd_fleet/src/task/checkpoint/sql/verify.sql)

**Observed on PostgreSQL 19rc2.** Two sessions ran the baseline quota SQL against disposable tables. While F01 held its empty-receipt transaction open, F02's same-workspace quota operation hit the experiment's 250 ms lock timeout (`55P03`). Reading F02's verified object under a shared lock and confirming a different workspace both succeeded during the hold. W still held 10 GiB. A separate upload receipt moved 8,388,624 bytes from reserved to stored. Replaying it moved zero additional bytes. [Quota lock](../../../rustd/crates/daemon/afd_fleet/src/task/checkpoint/sql/quota.sql) · [Receipt accounting](../../../rustd/crates/daemon/afd_fleet/src/task/checkpoint/sql/verify.sql)

**EDIT** `afd_fleet/src/lease/turn_checkpoint/transfer.rs::{execute,confirm}`. Preserve post-I/O custody validation while omitting quota writes on the empty-receipt path.

## 9. Slow storage can disconnect otherwise healthy workers

```text
BEFORE   B:w01 uploads -> daemon verifies bytes remotely -> 5 s answer expires
                         runner times out -> shared native session reconnects
                         w02..w16 also lose that connection during recovery
AFTER    verify admission -> bounded bulk deadline on runner and daemon
                           -> verify under pin deadline -> correlated answer
         control renewal and terminal reports retain capacity
```

This concerns **upload verification**. Ordinary restore GETs run on the runner outside the authorization RPC. Four full chunks represent about 32 MiB of verification. Finishing that in five seconds requires over 6.4 MiB/s before database work and network latency. A longer timeout alone can occupy the eight report slots longer.

**NEW** action classification in `af_wire/src/lease/turn_checkpoint/command.rs` and a `Turn::timeout` override in `afr_supervisor/src/client/turn_checkpoint.rs`. **EDIT** `afd_gateway/src/session/dispatch/turn_checkpoint.rs::SessionServices::turn_checkpoint` and `session/requests.rs::RequestSlots`. **KEEP** physical authentication in `afd_fleet/src/lease/turn_checkpoint/signing.rs::issue_object`.

## 10. Cleanup stops behind one contended checkpoint owner

```text
BEFORE   maintenance: F01 retain times out -> pass exits
                      F02..F16 + identity cleanup + orphan purge wait
AFTER    bounded indexed scan -> advance cursor -> try F01 -> record deferral
                              -> F02..F16 -> other maintenance stages
                              -> wrap and retry F01 under fresh pin checks
```

The owner here is an event's checkpoint state. It can be busy even after the host that created it is gone. The consequence is growing retained storage and quota pressure over time. The cursor carries no deletion authority. Live reader and writer pins still protect their objects.

**EDIT** `afd_fleet/src/lease/turn_checkpoint/maintenance.rs::Leases::maintain_turn_checkpoints`, `task/checkpoint/maintenance/mod.rs::maintain`, and `task/types.rs::CheckpointMaintenanceCursor`. **EDIT** `turn_checkpoint/sql/maintenance_candidates.sql` and the two recovery migrations for bounded scans and FK indexes.

## 11. Expiry can let two fleets count the sixteenth worker

```text
Setup    B has 15 other live leases. F01 occupies worker 16.
         F01 finishes and asks to keep its sandbox warm. New fleet X waits.

BEFORE   report F01: sees live lease -> decides future hold .... pauses in the statement
         time:                            F01 lease expires
         issue X:                         sees 15 + 0 committed holds -> issues
         report F01:                                              commits hold
         durable claims on B = 15 other leases + X + F01 hold = 17 for 16

AFTER    report wins shared B lock -> validates -> commits hold -> releases
         issue X waits -> exclusive B lock -> fresh count = 15 + 1 -> waits

         issue wins after expiry -> exclusive B lock -> X takes worker 16
         late F01 report -> shared B lock -> expiry recheck -> no new hold
```

The seventeenth item is a **durable reservation**. A valid late report may still settle usage. A fleet consuming its own valid hold counts the worker once. The audit insert's foreign key takes a key-share lock on B by the time the report statement finishes. The regression must pause the report after it has computed the hold and before that audit lock, while its SQL statement is still running. [Report statement](../../../rustd/crates/daemon/afd_fleet/src/lease/sql/commit_report.sql) · [Audit foreign key](../../../schema/20261108123400_runner_events.sql)

**EDIT** `afd_fleet/src/lease/commit/mod.rs::Leases::run_report_statement`, `lease/sql/report_statement.rs::ReportStatement`, and `lease/sql/commit_report.sql` for a shared host lock through commit on the hold-producing path. **KEEP** exclusive fleet/authoring issue and the fresh capacity read after it obtains that lock. The PostgreSQL lock experiment established compatibility. The production-path race test still has to demonstrate both orderings.

The [file-by-file implementation plan](m247-scalability-and-refactor.md#production-files-planned-after-the-benchmark) records the full paths and the two measurement-dependent edits. The [review ledger](m247-review-ledger.md) tracks the missing proofs.
