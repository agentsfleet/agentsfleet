---
type: reference
audience: contributor
verified: 2026-11-18
product_version: 0.78.0
executable: false
---

# M247 review and verification ledger

This ledger records review findings and the focused verification completed on the baseline and refactor. Burst comparison, per-file coverage and the final PR gate have separate evidence.

Base: `97224ed7432c9dd124ea8797d4324156b7022c07`; pre-refactor implementation: `86f481126c1aa053c34ef3bdaaa6f07baa1c74ab` plus candidate prefix `22ac73ab741b`.
The frozen production baseline is `d4823563e80829b97589846a1c7997964fdf0274`. The Nov 18 Astra direction review and two failing scheduler tests leave its production source intact.

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

## Open second-pass findings

The table preserves the 19 findings from the second pass. The verification below records their current evidence.

| # | Severity / source | Location | Finding |
|---|---|---|---|
| F1 | CRITICAL / testing | `rustd/crates/runner/afr_supervisor/src/lease_loop/turn_recovery.rs:11` | Missing ordinary Files offer through production supervisor/model entry, warm-hold retirement, thaw, and failure-before-model proof |
| F2 | CRITICAL / testing | `rustd/crates/daemon/afd_fleet/src/lease/turn_checkpoint/guard.rs:60` | Missing connection-bound Begin/Commit/Read/Adopt mutation-time supersession and credential-rotation races |
| F3 | CRITICAL / testing | `rustd/crates/daemon/afd_runner/src/heartbeat/cordon.rs:33` | Missing degraded/invalid/fallback and assignment/connection-race automatic-lift proofs |
| F4 | CRITICAL / testing | `rustd/crates/daemon/afd_fleet/src/lease/deliver/issue.rs:52` | Missing real NATS issued-reclaim versus fresh/refused/reserved publication proof |
| F5 | CRITICAL / state-machines | `rustd/crates/daemon/afd_fleet/src/lease/turn_checkpoint/pins.rs:267` | untested cell: ApiOnly × Abandon → retain the abandoned pending plan, release its writer pin and preserve the ready head |
| F6 | CRITICAL / state-machines | `rustd/crates/daemon/afd_fleet/src/lease/turn_checkpoint/maintenance.rs:98` | untested cell: ApiOnly × CollectTerminal → erase terminal private generations, recovery head and allocation receipts after retention without erasing public source state |
| F7 | CRITICAL / state-machines | `rustd/crates/daemon/afd_fleet/src/task/checkpoint/transfer/signing.rs:81` | untested cell: Incremental × VerifyUpload → authenticate and persist a newly uploaded incremental object and account its unique stored quota |
| F8 | CRITICAL / state-machines | `rustd/crates/daemon/afd_fleet/src/task/checkpoint/transfer/signing.rs:141` | untested cell: Incremental × Restore → restore the exact committed incremental manifest/object references under reader custody while preserving its base |
| F9 | CRITICAL / state-machines | `rustd/crates/daemon/afd_fleet/src/task/checkpoint/maintenance/mod.rs:75` | untested cell: Incremental × CollectOrphan → erase expired incremental private keys/references and release its physical quota once without retiring a live base |
| F10 | CRITICAL / state-machines | `rustd/crates/daemon/afd_fleet/src/lease/turn_checkpoint/pins.rs:287` | untested cell: writer × PinRelease → release the exact writer pin while preserving its pending plan and head, and refuse later protected operations |
| F11 | CRITICAL / state-machines | `rustd/crates/runner/afr_supervisor/src/recovery/turn/publication.rs:47` | untested cell: Commit × ReconcileRefusal → clear indeterminate publication only after exact abandonment, or stop when abandonment cannot settle it |
| F12 | CRITICAL / native | `rustd/crates/daemon/afd_fleet/src/lease/sql/commit_report.sql:181` | A report can decide a hold before the audit insert's foreign-key check takes a host lock. Exercise expiry and competing issue in that interval, then serialize hold-producing reports from before their decision through commit |
| F13 | INFORMATIONAL / data-migration | `schema/20261117154627_runner_turn_checkpoints.sql:6` | Missing fleet_id-leading index for fleet purge |
| F14 | INFORMATIONAL / parent | `schema/20261117154626_runner_turn_recovery.sql:4` | Missing head_turn_checkpoint_id-leading index for referenced checkpoint deletion |
| F15 | INFORMATIONAL / api-contract | `rustd/crates/daemon/afd_api_operator/src/handler/operator/events.rs:31` | Allowed event_type query description omits runner_uncordoned; regenerate OpenAPI |
| F16 | INFORMATIONAL / design | `ui/packages/app/tests/e2e/fixtures/fleet-stream-browser-probe.ts:21` | New reply status adds a second output; transport-status assertions need a dedicated selector |
| F17 | INFORMATIONAL / performance | `rustd/crates/runner/afr_supervisor/src/recovery/turn/capture.rs:149` | Ordinary upload serializes singleton native calls and physical transfers while frozen |
| F18 | INFORMATIONAL / performance | `rustd/crates/daemon/afd_fleet/src/lease/turn_checkpoint/maintenance.rs:35` | First owner lock timeout aborts later maintenance and repeats oldest candidate without cursor |
| F19 | INFORMATIONAL / performance | `rustd/crates/daemon/afd_fleet/src/lease/turn_checkpoint/sql/maintenance_candidates.sql:2` | LIMIT bounds returned owners but not the joined candidate scan; empty heads are created for uncaptured offers |

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

See [the architecture and benchmark decision](m247-scalability-and-refactor.md) for the concrete design, calculated bounds and implementation map.

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
| Post-refactor and file-backed performance | NOT RUN |
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

## Fingerprint policy

Review receipts, source fingerprints, exact commands and raw evidence remain separate.
This repo has no reviewed Orly hermetic reuse policy, so standalone Make passes are not reusable Orly completion records.
Batch coherent edits, rerun affected proof, and execute the required final gate on the pushed tree; never manufacture an override or a current binding.

Tenant-runner work is parked by the owner; the human check remains unperformed.
Thirteen identical full-history secret-scan findings were observed at the exact base and implementation history; no suppression, history rewrite or passing scan is claimed.
