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
              |   Lease TTL = 35 s from last grant/renewal.
              |
              +-> old lease expires -> compatible B can claim a higher fence
              |   -> start renewal -> restore exact committed generation
              |   -> verify root and files -> adopt covered inputs -> thaw
              |   -> first resumed model call -> one fenced settlement
              |
              +-> missed-heartbeat cordon has its own 105 s threshold + sweep
```

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
BEFORE   A:F01 safe boundary -> freeze -> register missing chunks [a b c d]
         a: native upload authorization -> PUT -> request authenticated readback
         b: native upload authorization -> PUT -> request authenticated readback
         c: ... -> d: ... -> commit -> thaw

AFTER    admission before snapshot/freeze -> register [a b c d]
         one bounded descriptor request -> authorized PUTs within limit
         -> one batch verify request -> all required objects verified -> commit -> thaw
         optional capture busy -> skip this boundary -> previous head remains
```

Capture happens while A is alive and can also recur on B after recovery. Four distinct missing chunks need eight singleton upload/verify commands before, versus two when they fit one batch, excluding registration, pages, pin renewal and commit. The physical uploads and authentication still take time. Skipping a capture can increase how much work a later crash replays.

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
BEFORE   authenticated plaintext map [a b c d]  32 MiB
                      copy bytes -> [d b a c]  +32 MiB -> writer
AFTER    authenticated owners       [a b c d]  32 MiB
                manifest ordering -> handles [d b a c] -> writer -> release
```

For four distinct 8-MiB chunks the old ordering step can retain about 64 MiB of plaintext for one restore. If all 16 workers reach that point together, this one component can approach 1 GiB. Both numbers exclude ciphertext, HTTP, manifests and allocator overhead. The new design charges each backing buffer through its last use.

**EDIT** `recovery/turn/files.rs::TurnSession::download` and `recovery/session.rs::Session::download_batch`. **NEW** window-scoped shared owners in `recovery/window.rs`. Reuse stays within the authorized scope.

## 6. Sixteen workers can independently exhaust B's resources

```text
BEFORE   B:w01 -> session client + private transfer window -> disk/network
             ...                                         ...
         B:w16 -> session client + private transfer window -> same disk/network
AFTER    B:w01..w16 -> one Lessee -> shared HTTP client + admission limits
                       |-- admitted restores -> bounded work -> release
                       |-- queued required restores -> renewal stays active
                       +-- optional capture without admission -> typed skip
```

Sixteen workers establish possible lease capacity. They do not imply the disk can restore 16 large trees at once. A required restore acquires its reader pin and short-lived URLs close to use. Cancellation or lease loss while queued prevents model entry. Started writers retain their permits through completion.

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
BEFORE   B:F01 read -> prove custody -> sign -> recheck custody -> lock quota Q
         B:F02 read -> prove custody -> sign -> recheck custody -> wait on Q
AFTER    restore, zero verification receipts -> recheck custody -> return URLs
         newly verified upload receipts      -> lock Q -> account once
```

Q is the same row across all 16 fleets in this example. A benchmark with one workspace per fleet would miss that contention. The second authority check matters because the connection, lease or pin can expire while the signer runs.

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

BEFORE   report F01: sees live lease -> prepares future hold .... pauses before COMMIT
         time:                            F01 lease expires
         issue X:                         sees 15 + 0 committed holds -> issues
         report F01:                                              commits hold
         durable claims on B = 15 other leases + X + F01 hold = 17 for 16

AFTER    report wins shared B lock -> validates -> commits hold -> releases
         issue X waits -> exclusive B lock -> fresh count = 15 + 1 -> waits

         issue wins after expiry -> exclusive B lock -> X takes worker 16
         late F01 report -> shared B lock -> expiry recheck -> no new hold
```

The seventeenth item is a **durable reservation**. This example does not require 17 operating-system processes. A valid late report may still settle usage. A fleet consuming its own valid hold counts the worker once.

**EDIT** `afd_fleet/src/lease/commit/mod.rs::Leases::run_report_statement`, `lease/sql/report_statement.rs::ReportStatement`, and `lease/sql/commit_report.sql` for a shared host lock through commit on the hold-producing path. **KEEP** exclusive fleet/authoring issue and the fresh capacity read after it obtains that lock. The PostgreSQL lock experiment established compatibility. The production-path race test still has to demonstrate both orderings.

The [file-by-file implementation plan](m247-scalability-and-refactor.md#production-files-planned-after-the-benchmark) records the full paths and the two measurement-dependent edits. The [review ledger](m247-review-ledger.md) tracks the missing proofs.
