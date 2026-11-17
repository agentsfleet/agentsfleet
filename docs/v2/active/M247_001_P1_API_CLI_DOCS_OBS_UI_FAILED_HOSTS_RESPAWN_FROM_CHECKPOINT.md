<!--
SPEC AUTHORING RULES (the one comment that survives):
- The spec standard (spec.md) holds every rule.
- Body order is the implementing agent's read order.
- No time or effort estimates, owners or implementation dates (SPEC-ESTIMATE).
-->

# M247_001: Failed hosts are cordoned and unfinished turns resume elsewhere

**Prototype:** v2.0.0
**Milestone:** M247
**Workstream:** 001
**Date:** Nov 06, 2026
**Reviewed:** Nov 17, 2026
**Status:** IN_PROGRESS
**Priority:** P1, a failed host must not lose a turn or mix two attempts' replies
**Categories:** API, CLI, DOCS, OBS, UI
**Batch:** B1, the only Workstream of M247
**Depends on:** M246_001, still pending, for tenant-owned placement and credential boundaries. M242_002, M243_001, M244_001 and M245_001 are merged. PR #803 is merged; its task recovery is reused.
**Provenance:** Original draft on `14539a73e`; source review against fetched `origin/main` and HEAD `97224ed7432c9dd124ea8797d4324156b7022c07`.
**Canonical architecture:** `docs/architecture/push_scheduler.md`, `runner_fleet.md`, `lease_flow.md`, `runner_execution.md`, `scaling.md`, `observability.md`.

## Overview

**Goal (testable):** After a runner dies, an eligible runner reclaims its expired lease, restores the last complete checkpoint, and completes one admission without mixing attempts.

**Readiness:** The original checkpoint plan is stale. Ordinary chat has no durable turn checkpoint, and task checkpoints are captured at run endings. Section 4 specifies the missing work and decisions that keep this spec pending; the [implementation and manual-check guide](../reviews/m247-implementation-and-manual-check.md) expands the low-level design.

A lease's expiry controls reclaim. Host liveness controls quarantine. These clocks are independent, so recovery can start before the failed host is cordoned.

```text
last renewal R ---- host A dies ---- lease deadline D=R+35s ---- B may reclaim
last heartbeat H ------------------ H+105s -------------------- sweep may cordon A
                                      |
                       eligible B + capacity + gates required
                                      v
                  same event E, new lease L2, higher fence F2
                  +--> plain chat: NEW capture/restore -----+
                  |                                        |
                  +--> task: existing files/history --------+
                                                           v
                                          attempt F2 -> one final settlement
```

Durations come from `af_core::timing`: `LEASE_TTL=35s`, `RUNNER_OFFLINE_AFTER=105s`, `HEARTBEAT_INTERVAL=10s`, `OFFER_FALLBACK_TICK=5s`.

**Source audit:** Current push streams, both capacity counts, native task recovery, actual CLI grammar and timestamped schema rules replace the original assumptions. The companion records their source paths and detailed amendments.

## PR Intent & comprehension handshake

- **PR title (eventual):** feat(runner): cordon failed hosts and resume fenced attempts from checkpoints
- **Intent:** A failed host leaves durable work for another eligible runner, and users see one coherent replacement attempt.
- **Authoring handshake:** Review and update the failure design, draw each scenario, and identify every implementation file and symbol category.
- **Assumptions:** Indy authorized implementation with orly-build on Nov 17, 2026. Section 1 can start independently; Section 4 remains blocked on recovery design and final M246 names.
- **Implementer handshake:** Restate the two recovery paths and their limits in Pull Request Session Notes before implementation edits.

## Implementing agent: read these first

1. `rustd/crates/daemon/afd_fleet/src/lease/assign/ledger.rs` and `lease/sql/lease/claim_admission_head.sql`: slot fence and predecessor selection.
2. `rustd/crates/daemon/afd_fleet/src/lease/issue.rs` and `lease/sql/lease/insert_lease_with_event.sql`: host lock, inlined capacity, task attachment and commit.
3. `rustd/crates/daemon/afd_runner/src/sweep/liveness/mod.rs` and `heartbeat/mod.rs`: offline transaction, liveness fallback and stream-connected beat.
4. `rustd/crates/daemon/afd_gateway/src/session/cache/mod.rs` and `session/dispatch/progress.rs`: cached activity and lease deadlines.
5. `rustd/crates/runner/afr_supervisor/src/lease_loop/recovery.rs` and `rustd/crates/runner/afr_agent/src/engine/recovery.rs`: task workspace and history recovery.
6. `rustd/crates/runner/afr_agent/src/loop/conversation.rs` and `ui/packages/app/lib/streaming/fleet-stream-reply-registry.ts`: fold cursor and decoder lifecycle.

## Files Changed and symbol inventory

Paths are repository-relative; braces enumerate files. EDIT and NEW describe future implementation; **proposed** names await Section 4's decisions.

| File(s) | Action | Methods / functions | Traits | Structs / enums / other change |
|---|---|---|---|---|
| `docs/v2/active/M247_001_P1_API_CLI_DOCS_OBS_UI_FAILED_HOSTS_RESPAWN_FROM_CHECKPOINT.md`, `docs/v2/reviews/m247-implementation-and-manual-check.md` | EDIT spec; NEW guide; later MOVE spec | Plan, low-level changes, exact CLI commands and manual proof | None | IN_PROGRESS; retain explicit recovery design gates |
| `rustd/crates/daemon/afd_runner/src/capacity/{mod.rs,sql/mod.rs,sql/census.sql,sql/lock_host.sql}` | EDIT | `lock`, `lock_host`, `Capacity::can_claim`, `Held::can_claim` | Preserve `Guard` conversions | New `Claimant`; extend `Host`, `Capacity`, `Held` for automatic cordon; claimant-aware hold census |
| `rustd/crates/daemon/afd_fleet/src/lease/store.rs` | EDIT | `Leases::lock_host_capacity`, `lock_host` | None | Thread claimant through full census calls; preserve connection authority |
| `rustd/crates/daemon/afd_authoring/src/draft/mod.rs` | EDIT | `Drafts::lock_host_capacity` | None | Pass authoring/existing claimant purpose |
| `rustd/crates/daemon/afd_authoring/src/lease/{claim/issue/mod.rs,renew.rs}` | EDIT | `admit`, `allowed`, `available`, `renew` | None | Authoring admission counts every hold; renewal excludes holds |
| `rustd/crates/daemon/afd_fleet/src/lease/{issue.rs,renew/mod.rs,commit/mod.rs}` | EDIT | `issue_for_revision`, `renew`, existing report commit helpers | None | Extend `Issued` only for new recovery metadata; preserve report fence |
| `rustd/crates/daemon/afd_fleet/src/lease/sql/lease/insert_lease_with_event.sql` | EDIT | Inlined claim census and guarded slot update | None | Count other fleets' holds; consume own hold in successful issue transaction |
| `rustd/crates/daemon/afd_fleet/src/lease/{sql/commit_report.sql,sql/lease/release_fleet_slot.sql,sql/report_statement.rs,commit/mod.rs,report/finalize.rs}`, `rustd/crates/daemon/afd_gateway/src/session/dispatch/report.rs`, `rustd/crates/runner/afr_supervisor/src/{client/stream.rs,lease_loop/settle.rs,lease_loop/mod.rs}` | EDIT | Guard late hold publication; carry accepted disposition through `ReportAnswer`, settlement, gateway and `Plane::report` | None | Add `ReportResponse::hold`; release exact local lease hold on None; replay never republishes a hold |
| `rustd/crates/daemon/afd_fleet/src/lease/{renew/batch/frame.rs,sql/renew_batch.sql}` | EDIT / prove unchanged count | `renew_frame`, `run_batch` | None | Preserve hold-free renewal and existing connection/fence checks |
| `rustd/crates/runner/afr_supervisor/src/holds/{mod.rs,keeper.rs,keeper_tests.rs}` | EDIT | `Holds::{take,park,occupy}`, `Occupied::drop`, `Keeper::{park,occupy,cap,retire}` | `Sandbox` reused | `Request`, `Occupied` support atomic busy/held handoff and cancellation; reserve counts each sandbox once |
| `rustd/crates/runner/afr_supervisor/src/lease_loop/{mod.rs,hold.rs}`, `rustd/crates/runner/afr_supervisor/src/{worker_pool/mod.rs,worker_pool/tests.rs,authoring/mod.rs}` | EDIT | `Worker::next`, `Lessee::{run_lease,author}` and guard handoff | `Sandbox` reused | Proposed `Occupant::{Fleet,Authoring}`; move shared occupancy above dispatch and release exactly once |
| `rustd/crates/daemon/afd_runner/tests/capacity/{mod.rs,fixture.rs,holds.rs}`, `rustd/crates/daemon/afd_runner/tests/integration_hold_plans.rs` | EDIT; NEW `holds.rs` | Capacity contention, renewal and plan tests | None | Fixtures include live/expired/own/other holds |
| `rustd/crates/daemon/afd_fleet/tests/integration_held_sandbox/{resume.rs,counted.rs}` | EDIT | Issue consumes hold; refused issue preserves it | None | Counterexamples for both fleet issue paths |
| `schema/20261108123100_runner_runners.sql` | EDIT | Table constraints and grants | None | `cordon_reason`, `cordoned_at`, proposed `cordon_healthy_since`; no migration slot 2900 |
| `rustd/crates/foundation/af_core/src/timing.rs` | EDIT | Compile-time timing assertions | None | New `UNCORDON_AFTER = 3 * HEARTBEAT_INTERVAL`; existing 35s/105s stay |
| `rustd/crates/daemon/afd_runner/src/{cordon.rs,lib.rs}` | NEW / EDIT | Reason parsing and row decoding | Existing spelling traits | New domain `AutomaticCordon`; reuse the wire `CordonReason` enum |
| `rustd/crates/daemon/afd_runner/src/sweep/liveness/{mod.rs,tests.rs}` | EDIT | `Liveness::{visit,record_offline,due}`; remove `release_slots`, `expire_slots` | Keep `Sweep` implementation | Remove `EXPIRE_PAST`, `CONTEXT_SLOTS`; lock and recheck stale observation |
| `rustd/crates/daemon/afd_runner/src/sql/sweep/{mod.rs,select_due_runners.sql,insert_offline_event.sql,cordon_silent_runner.sql,mark_drained_if_idle.sql}` | EDIT; NEW cordon statement | Offline/cordon transaction; fair actionable drain selection | None | Drain checks live fleet leases and authoring attempts; expired-unreported work does not block it |
| `rustd/crates/daemon/afd_runner/src/sql/sweep/expire_active_lease_slots.sql` | DELETE | Remove `EXPIRE_ACTIVE_LEASE_SLOTS` | None | Lease expiry remains the reclaim clock |
| `rustd/crates/daemon/afd_runner/src/heartbeat/{mod.rs,cordon.rs,policy.rs}` | EDIT; NEW helper | `record_beat`, `bump_liveness`, `heartbeat_connected` | None | Proposed heartbeat transition result; lift only on proved healthy beats |
| `rustd/crates/daemon/afd_runner/src/sql/runner/{heartbeat_with_transition_event.sql,touch_runner_last_seen.sql,select_runner_assigned_policy.sql,mod.rs}` | EDIT | Same-statement liveness and guarded lift | None | Return lift outcome; fallback touch resets recovery proof |
| `rustd/crates/daemon/afd_fleet/src/lease/sql/lease/{select_ledger_candidates.sql,claim_admission_head.sql}` | EDIT | Candidate filter and authoritative claim guard | None | Cordon blocks new claims; return prior lease ID; preserve M246 placement |
| `rustd/crates/daemon/afd_gateway/src/session/{offer.rs,dispatch/heartbeat.rs}` | EDIT if needed | `offer_state`, `heartbeat`, `apply_policy` callers | None | Advice reflects cordon; host-row check remains authoritative |
| `rustd/crates/daemon/afd_runner/tests/integration_sweeps/{mod.rs,seed.rs}` | EDIT | Atomic cordon, stale-read race, drain tests | None | Stop expecting backdated slots |
| `rustd/crates/daemon/afd_fleet/tests/integration_runner_beat/{mod.rs,cordon.rs}` | EDIT; NEW cordon tests | Connected beat, returning/flapping/operator cases | None | Use current native stream fixtures |
| `rustd/crates/daemon/afd_fleet/src/lease/{assign/ledger.rs,envelope.rs}` | EDIT | `Admission::acquired`, `ledger_claim` | None | `Admission`, `Acquired` gain optional predecessor ID; reuse `Kind::{Fresh,Reclaim}` |
| `rustd/crates/daemon/afd_fleet/src/lease/{deliver/issue.rs,bracket.rs,pull/restart.rs,pull/mod.rs}` | EDIT; NEW restart helper | `issue_ready`, proposed `publish_restarted` after issue commit | None | One best-effort publication attempt per issued reclaim; not per candidate |
| `rustd/crates/protocol/af_wire/src/report/mod.rs`, `rustd/crates/runner/afr_supervisor/src/test_util/mod.rs` | EDIT | Serialize committed hold disposition | Existing serialization derives | Proposed `AcceptedHold` with lease/fence/deadline; `ReportResponse::hold`; current `until_acknowledged` is already generic |
| `rustd/crates/daemon/afd_api_wire/src/tail/mod.rs` | EDIT | Serialization / OpenAPI | Serde / `ToSchema` derives | `TailFrame::RunRestarted` with event, fence and timestamp |
| `rustd/crates/daemon/afd_fleet/src/lease/activity/{mod.rs,published.rs,tests.rs,vocabulary.rs}` | EDIT | `Published::of`, `publish_activity`, `activity_for_target` | None | Every `Published` output carries daemon-stamped attempt fence; `Target::fence` already exists |
| `rustd/crates/daemon/afd_gateway/src/session/{cache/mod.rs,cache/snapshot.rs,dispatch/progress.rs}` | EDIT / regression proof | `LeaseCache::target`, `LeaseRecord::try_from`, `progress` | Preserve existing cache conversions | Expired/cancelled targets drop; never add a database lookup per chunk |
| `rustd/crates/daemon/afd_api_tenant/src/handler/stream/mod.rs` | EDIT | Stream frame documentation | None | Register `run_restarted` in documented frame kinds |
| `ui/packages/app/lib/api/{events.ts,events-types.ts}` | EDIT | Frame parsing / routing | None | `ActivityLiveFrame`, `FRAME_KIND.RUN_RESTARTED`; attempt fence on output types |
| `ui/packages/app/lib/streaming/{fleet-stream-entry.ts,fleet-stream-dispatch.ts,fleet-stream-reply-registry.ts,fleet-stream-reply-frames.ts,fleet-stream-row.ts}` | EDIT | `createEntry`, `applyFrame`, `dispatchReplyFrame`, `writeChunk`, disposal helpers | None | `Entry` greatest-fence map; `FleetEvent` restart metadata; decoder identity guards |
| `ui/packages/app/lib/streaming/{reply-stream.ts,reply-stream.test.ts,fleet-stream-registry.reply.test.ts}` | EDIT / regression proof | `ReplyStream::{write,dispose}` | None | Same-attempt second start stays a gap; new attempt replaces decoder |
| `ui/packages/app/components/domain/conversation/stream/useFleetEventStream.ts`, `ui/packages/app/components/domain/activity/messages/fleetMessageReaders.ts`, `ui/packages/app/components/domain/activity/replies/{FleetReplyBody.tsx,FleetReplyBody.test.tsx}` | EDIT | Hook/readers and `FleetReplyBody` | None | Render restart marker without restoring removed admin pages |
| `cli/src/commands/fleet/{steer-events.ts,task-activity.ts,tool-activity.ts}`, `cli/test/fleet-steer-tail.unit.test.ts` | EDIT | `makeFrameCallback`, `openEventTail`; printer reset helpers | `SteerPresentation` type changes | Keep greatest attempt; separate output with restart line; preserve JSON mode |
| `rustd/crates/daemon/afd_api_wire/src/{operator.rs,admin/mod.rs}` | EDIT | Serde / OpenAPI | Derives only | `RunnerItem::cordon` inherited by `RunnerDetail`; new `RunnerCordon`, canonical `CordonReason` and `RunnerEventType::RunnerUncordoned` |
| `rustd/crates/daemon/afd_runner/src/view/{mod.rs,decode.rs}`, `rustd/crates/daemon/afd_runner/src/sql/runner_view/{list_runners_first.sql,list_runners_after.sql,runner_detail.sql}` | EDIT | `ItemRow::parse`, `RunnerItem::cordon` accessor | None | Domain `RunnerItem` and `ItemRow` carry optional cordon |
| `rustd/crates/daemon/afd_api_operator/src/handler/operator/runners.rs` | EDIT | `item`, `detail_payload` | `Services` unchanged | Map domain cordon into actual API response |
| `cli/src/commands/platform/{runner-schema.ts,runner-vocabulary.ts,runners.ts}`, `cli/src/program/tree/platform-runners.command.ts` | EDIT | `platformRunnersList`, `platformRunnerShow`, `platformRunnerHistory` | None | Effect schemas decode cordon; CLI `RunnerEventType` gains `Uncordoned` |
| `rustd/crates/daemon/afd_fleet/tests/integration_runner_views.rs`, `cli/test/platform-runners.unit.test.ts` | EDIT / NEW if absent | Cordon wire shape, omission, reason and lift rendering | None | Current API and CLI consumer proofs |
| `rustd/crates/protocol/af_observability/src/{metrics/declared/fleet.rs,metrics/label/fleet.rs,producers/fleet/mod.rs,producers/fleet/runner/mod.rs}`, `rustd/crates/protocol/af_observability/tests/producers_drive.rs`, `docs/metrics.census.tsv` | EDIT | Proposed `runner::cordon_changed`; register instrument | Existing metrics traits reused | New bounded `CordonChange` label enum and counter instrument |
| `rustd/crates/daemon/afd_fleet/tests/{integration_respawn.rs,fleet_suite.rs}` | NEW / EDIT | Lease race, checkpoint, gates, billing, folds and output tests | Existing test seams | Add deterministic fault points, not mocked databases |
| `rustd/crates/runner/main/{tests/respawn.rs,tests/runner_suite.rs,Cargo.toml}` | NEW / EDIT | Two-process kill/restart drill | Existing runner entry point | Use actual daemon, native stream, Postgres and core NATS |
| `public/openapi.json` | REGENERATE | Existing generator with `--no-banner` | None | Restart frame, cordon field/event, finalized recovery surfaces |
| `docs/architecture/{push_scheduler.md,runner_fleet.md,lease_flow.md,runner_execution.md,scaling.md,observability.md}` | EDIT | Canonical timing, capacity, recovery and loss rules | None | Withdraw stale predecessor-column and Dragonfly proposals |
| Docs repository `runners.mdx`, `changelog.mdx` on `chore/m247-respawn-changelog` | EDIT during implementation | Public restart and cordon behavior | None | Own docs branch; no cross-repository write in this review |

### Recovery changes that the original file list omitted


| File(s) | Action | Methods / functions | Traits | Structs / enums / schema |
|---|---|---|---|---|
| `rustd/crates/protocol/af_wire/src/lease/{payload.rs,mod.rs,turn_checkpoint.rs}` | EDIT; NEW proposed module | Checkpoint validation and serialization | Serde, validation derives | Proposed `TurnCheckpoint`, `TurnCheckpointReceipt`, `TurnCheckpointRead`, `TurnCheckpointReadResponse`, descriptor-only `TurnResume`; `LeasePayload::turn_resume` |
| New timestamped `runner_turn_checkpoints.sql` under `schema/`; `rustd/crates/daemon/afd_db/src/migration.rs` and registration tests | NEW / EDIT | Register a new event-owned table | None | Proposed `runner.turn_checkpoints`; same event/workspace scope, generation, writer fence, history and fold prefix; no lease-owned recovery chain |
| `rustd/crates/daemon/afd_fleet/src/lease/mod.rs`, `rustd/crates/daemon/afd_fleet/src/lease/turn_checkpoint/{mod.rs,sql/mod.rs,sql/save.sql,sql/load.sql,sql/adopt_folds.sql,sql/cleanup_terminal.sql}` | NEW / EDIT proposed | `save_turn_checkpoint`, `load_turn_checkpoint`, `adopt_checkpoint_folds` | No new store trait required | Proposed encrypted checkpoint row and compare-and-swap publication; validate live lease/event; remove terminal-event snapshots |
| `rustd/crates/protocol/af_wire/src/protocol/{up/mod.rs,up/decode.rs,up/other.rs,down.rs}` | EDIT proposed | Encode/decode checkpoint save/ack and fenced read/response | Existing protocol derives | Both complete messages fit one MiB; saved history never rides inline in the offer |
| `rustd/crates/daemon/afd_gateway/src/{session/dispatch/mod.rs,session/dispatch/turn_checkpoint.rs,operation.rs}`, `rustd/crates/runner/afr_supervisor/src/{uplink/mod.rs,downlink/dispatch.rs,client/turn_checkpoint.rs,client/mod.rs}` | NEW / EDIT proposed | Route fenced save request and correlate committed acknowledgement | `RunnerApi` need not gain a new required method | Reuse request IDs and native reports lane; extend `Operation` and bound in-flight snapshots |
| `rustd/crates/runner/afr_agent/src/{engine/mod.rs,engine/turn_recovery.rs,loop/mod.rs,loop/shared.rs,loop/conversation.rs}`, `rustd/crates/runner/afr_supervisor/src/verbs/mod.rs` | NEW / EDIT proposed | Capture/restore full safe-boundary history; initialize fold cursor; preserve wire event IDs in `Folded` through `FencedVerbs::claim` | Proposed `RecoveryBoundarySink::save(RecoverySnapshot) -> RecoveryReceipt`; preserve semantic-memory `Checkpoint::push`, `Inbox::claim`, `AgentEngine::run` signatures | Proposed `TurnRecovery`; extend `AgentRun`, `Shared`; never equate `af_context::Checkpoint` with a turn snapshot |
| `rustd/crates/runner/afr_supervisor/src/lease_loop/{drive.rs,work.rs,turn_recovery.rs}` | NEW / EDIT proposed | Wire turn checkpoint sink, restore before model, await/validate receipt | Implement proposed sink | Ordinary chat history recovery; workspace policy remains an explicit blocker |
| `rustd/crates/daemon/afd_fleet/src/lease/{answer/mod.rs,deliver/issue.rs,fold.rs,commit/mod.rs,sql/lease/claim_inputs.sql,sql/lease/settle_folds.sql}` | EDIT proposed | `payload`, `render_offer`, `claim_folds`, `settle_folds`; adopt checkpoint-covered claims | Existing `Thread` unchanged | `answer::Context` adds resume metadata; settled folds must belong to new live lease/fence |
| `rustd/crates/runner/afr_supervisor/src/lease_loop/{recovery.rs,hold.rs,work.rs}`, `rustd/crates/runner/afr_supervisor/src/recovery/{capture.rs,restore.rs}`, `rustd/crates/runner/afr_agent/src/{engine/recovery.rs,loop/finish.rs,loop/task_slice.rs,loop/task.rs,loop/children.rs,loop/recovery_boundary.rs}` | EDIT; NEW proposed boundary; REUSE restore | `Harness::finish`, `capture_frozen`, `capture_without_sandbox`, `Capture::run`, `Restore::run`, `TaskRecovery::{capture,restore}` | Reuse `guffbolt::Extension` hooks and `Sandbox::{freeze,thaw}`; prove all-work quiescence before capture | Preserve `TaskContext`, checkpoint generation, `covered_input_sequence`, `effect_watermark`; no parallel task pointer |
| `rustd/crates/foundation/af_crypto/src/aad.rs`, `rustd/crates/foundation/af_crypto/tests/turn_checkpoint.rs`, `rustd/crates/daemon/main/src/{plane/mod.rs,sweepers/checkpoints.rs,sweepers/mod.rs}` | EDIT; NEW crypto test | Proposed `Aad::turn_recovery`; inject `TurnCheckpoints` in `lease_plane`; register bounded terminal cleanup | Existing encryption primitives reused | Domain-separated event/generation/format binding; no key reaches runners |
| `rustd/crates/runner/afr_agent/src/{engine/tests.rs,nested/run_tests.rs,loop/budget_tests.rs,loop/compaction_tests.rs,loop/memory_tests.rs,loop/provider_failure_tests.rs,loop/session_tests.rs,loop/tests.rs,loop/turn_tests.rs,loop/fold_tests/fixture.rs,loop/fold_tests/mod.rs,loop/task_tests/mod.rs,loop/task_tests/behavior/publication_fixture.rs,loop/task_tests/behavior/publication_guards.rs,loop/task_tests/behavior/recovery_fixture.rs}`, `rustd/crates/runner/afr_agent/tests/providers/support/mod.rs`, `rustd/crates/runner/afr_supervisor/src/verbs/tests/decision/loop_boundary/mod.rs`, `rustd/crates/runner/afr_supervisor/src/verbs/tests/folds.rs` | EDIT fixtures | Direct `AgentRun` construction supplies explicit recovery fields | No fake successful recovery sink | Preserve baseline fixture meaning; real recovery tests require a committed receipt |
| `rustd/crates/daemon/{afd_api/tests/harness/native/stores.rs,afd_bench/src/lane/lease/drain/stage.rs,afd_fleet/tests/integration_report_memory.rs,afd_fleet/src/lease/test_dead.rs,afd_fleet/tests/integration_lease_gates/history.rs,afd_fleet/tests/integration_lease_gates/window.rs,afd_fleet/tests/support/fleet_report_seed/plane.rs}` | EDIT fixtures | Direct `LeaseService` constructors supply the checkpoint owner | Existing test seams | Initialize the real encrypted store without changing unrelated behavior |
| `rustd/crates/daemon/afd_fleet/tests/integration_turn_checkpoints.rs`, `rustd/crates/runner/afr_agent/src/engine/turn_recovery_tests.rs`, `rustd/crates/runner/afr_supervisor/src/lease_loop/tests/recovery/{mod.rs,mid_turn.rs}` | NEW / EDIT proposed | Repeated kills, corrupt state, lost acknowledgement, fold adoption and no-model-on-restore-failure | Fault injection through existing seams | Prove each new writer and reader through production entry points |

**Trait verdict:** The existing `Sweep`, `CheckpointStore`, `Checkpoint`, `Inbox`, `AgentEngine`, `RunnerApi`, `Services` and `Sandbox` traits need no breaking signature change for the confirmed work. The proposed safe-boundary sink is new. Its capture semantics must be settled before coding; adding a default no-op would falsely advertise recovery.

## Applicable Rules and Gates

- `spec.md`: SPEC-MECH, SPEC-PIN, SPEC-FILES, SPEC-FAIL, SPEC-MODEL, SPEC-SIZE; keep six Sections and at most 500 lines.
- `domains/sql.md`: SCH, MIG, STS, SQL-ONEFACT, SQL-LOCKCLOCK, SQL-SWEEPCLOCK, SQL-PREPARE, SQL-PLAN, TXN.
- `code.md`, `languages/rust.md`, `languages/typescript.md`: OWN, CAS, CODE-SEAM, ERR-RS, RS-RACE, TS-SCHEMA.
- `tests.md`, `domains/http-surface.md`, `domains/logging.md`, `docs.md`: failure injection, contention, OpenAPI, enum consumers, bounded metrics and source-grounded prose.
- Final implementation gates: `make lint-all`, `make test-unit-all`, `make test-integration-rustd`, `make check-version`, `gitleaks detect --redact --no-banner`, `orly gate pr`.
- Run changed chat Playwright scenarios through `make dry-app`. Prepare new SQL, check hold query plans, regenerate OpenAPI, and update the metrics census.
- Keep the fleet issue's inlined count and M245 batch renewals. Do not restore polling or add a read for every activity frame.

## Prior Art / Reference Implementations

- `afd_runner/src/heartbeat/mod.rs`: connected owner proof and combined liveness write; preserve its safe fallback.
- `afd_fleet/src/lease/issue.rs`, `task/lease.rs`, `afr_supervisor/src/recovery/restore.rs`: committed issue and existing native task restore.
- `afd_gateway/src/session/cache/mod.rs`, `afd_fleet/src/lease/activity/published.rs`: cached expiry check and existing fenced tool-call IDs.
- Kubernetes node taints: <https://kubernetes.io/docs/concepts/scheduling-eviction/taint-and-toleration/>. Automatic quarantine and operator intent remain separate.

## Sections (implementation slices)

### Section 1: Count holds and reserve one free worker

Under the host lock, a new claim counts live fleet leases, authoring attempts and other fleets' live holds. An authoring claim counts every hold.
A fleet may consume its own hold without a second capacity charge. Successful issue clears that hold in the same transaction; refusal rolls the clear back.

The keeper counts fleet and authoring workers through shared dispatch. Its target is `held <= max(workers - busy - 1, 0)` while a worker remains free.
Make held-to-busy and busy-to-held transfers atomic, including cancellation cleanup; count each sandbox once before applying the target. Retire oldest first, and ring a beat after every retirement.

```text
workers=2: [busy C][held H] -> cold fleet X / authoring: REFUSE
                          -> fleet H resumes: consume H -> [busy C][busy H]
workers=3: [busy C][held H][free] -> keeper cannot fill the last free worker
hold retired locally -> immediate beat -> daemon count drops -> cold work fits
stale hold row + renewal -> ignore hold count -> current work keeps its worker
```

An expired late report may settle completion and usage, but cannot introduce a future hold after capacity was reassigned. Return accepted hold disposition in the report acknowledgement; None releases the exact parked lease, and replay only reads a still-live same-fence hold. Live lease-to-hold conversion counts once; physical teardown can lag.
The rollout pairs the keeper reserve with the daemon count; shipping the count alone could strand cold work behind holds.

- **Dimension 1.1**: Test `test_a_lease_and_a_hold_refuse_a_third_fleet` (integration): two workers, one lease and another hold refuse fleet and authoring claims.
- **Dimension 1.2**: Test `test_the_claimants_own_hold_takes_no_second_worker` (integration): that hold's fleet can issue; both census paths agree.
- **Dimension 1.3**: Test `test_a_renewal_counts_no_holds` (integration): stale hold plus live leases permits batch, task and authoring renewals.
- **Dimension 1.4**: Test `test_an_issued_lease_clears_its_fleets_hold` (integration): success consumes; capacity refusal and task-attachment rollback preserve.
- **Dimension 1.5**: Test `test_a_hold_never_takes_the_last_free_worker` (unit): park, occupy and resize evict oldest and ring a beat.
- **Dimension 1.6**: Test `test_two_worker_hold_transfers_count_each_sandbox_once` (unit): occupy-before-take and park-before-guard-drop preserve valid resume/park; cancelled transfers leak no reservation.
- **Dimension 1.7**: Test `test_authoring_occupancy_preserves_the_keeper_reserve` (unit): two workers, authoring A and held H retire H; errors/panics/cancellation release once.
- **Dimension 1.8**: Test `test_expired_late_report_cannot_add_a_hold_to_a_full_host` (integration): late report races new issues; completion/usage settle, future hold is refused, live lease-to-hold still works.

### Section 2: Quarantine silent hosts without changing lease deadlines

The sweeper locks the host and rereads `last_seen_at` against the database clock. A fresh beat invalidates an old due-query observation.
In one transaction, a new offline episode records `runner_offline`, sets automatic cordon fields on an active host, and records `runner_cordoned`.
A refused cordon rolls the offline event back. Remove both slot-backdating callers and their SQL; drain completion retains its idle check.
Select actionable offline episodes and drains, or traverse fairly. Processed stale hosts and busy drains cannot starve a newly silent host; `NEVER_SEEN` never triggers quarantine.

Keep operator `status` separate. An automatic cordon blocks new fleet and authoring issues, while existing live leases retain their normal renewal/report checks.
Every authoritative issue check enforces the cordon. Filtering offers alone would still allow an already selected claim to issue.

```text
silent past H+105s -> lock + recheck stale? -- no --> leave host unchanged
                                 |
                                yes -> [offline event + auto cordon + audit] COMMIT
returning valid beat -> start healthy_since -> continuous valid beats for 30s -> lift
           gap >= 20s / new silence -------> reset healthy_since ------------^
operator cordon / drain / revoke ---------> never changed by automatic lift
```

**Implementation default:** track `cordon_healthy_since` on the host, rather than deriving health from prunable event history.
Only an authenticated current connection's valid heartbeat advances this proof. A gap of two heartbeat intervals, degraded health or an invalid beat resets it.
Both the combined write and fallback recheck credential digest and connection ID under the host lock. Fallback updates liveness only; it resets proof and never lifts.

- **Dimension 2.1**: Test `test_a_silent_host_is_cordoned_with_its_offline_record` (integration): stale host records one episode; repeat sweep adds nothing.
- **Dimension 2.2**: Test `test_a_refused_cordon_leaves_no_offline_record` (integration): injected update failure rolls back both; retry writes both.
- **Dimension 2.3**: Test `test_the_sweep_leaves_a_renewing_hosts_lease_alone` (integration): lost beats with live renewals change no deadline or fence.
- **Dimension 2.4**: Test `test_a_cordoned_host_claims_nothing` (integration): direct claim and issue, including selected-before-cordon work, write no new lease.
- **Dimension 2.5**: Test `test_automatic_lift_requires_continuous_valid_beats` (integration): 30s healthy lifts; missed cadence, degraded/invalid beats, fallback and connection supersession between policy-read/write reset or advance no proof.
- **Dimension 2.6**: Test `test_a_fresh_beat_defeats_a_stale_sweep_observation` (integration): pause after due-read, beat, resume sweep; no false cordon.
- **Dimension 2.7**: Test `test_an_operator_cordon_outlasts_an_automatic_lift` (integration): operator states remain; current stream refusal rules still apply.
- **Dimension 2.8**: Test `test_processed_silent_hosts_do_not_starve_new_offline_episodes` (integration): 101 processed stale hosts or busy drains precede a newly stale host; the bounded pass handles it, and authoring/expired-unreported leases have correct drain behavior.

### Section 3: Fence every attempt's output and reset clients once

Use `issue_ready` after the lease transaction commits and before encoding its offer. Publish `run_restarted` only for an issued `Kind::Reclaim`.
There is one publication attempt per issued reclaim. Core NATS provides no durable replay, so the publish can be lost.

All attempt output carries the daemon's fence, including chunks, tools, context progress and task progress. Derive it from `Target`, never runner-supplied activity.
First ordinary output initializes the event's fence silently; a fleet fence above one does not prove this event restarted. Only a greater observed attempt resets provisional state; older output drops.

```text
A/F7 chunk seq0 "abc" -> B/F8 restart -> B/F8 seq0 "xy" -> shown answer "xy"
A/F7 late chunk -------------------------------------------------> DROP
restart hint lost ----- B/F8 first frame -> reset once ------------> B/F8 seq0
B/F8 hint repeated / delayed after F8 chunks ---------------------> NO RESET
B/F8 missing seq1 -> B/F8 seq2 ------------------------------------> GAP -> durable repair
old decoder callback queued -> reset F8 -> callback F7 ------------> DROP
```

Guard asynchronous decoder callbacks by their decoder identity and attempt, in addition to the current fleet entry.
Retain completed tools provisionally during retry; final saved history replaces them and excludes deleted prior-attempt tool records. Close unfinished tools as interrupted; terminal events cannot reopen.

The CLI prints one restart separator per greater attempt. Already printed terminal or redirected output remains; the final saved reply is authoritative.
Use "Run restarted" because a recovered healthy host can reclaim its own expired work. Placement preserves eligibility without promising a different physical host.
Keep cached target expiry/cancel checks and SQL fences for reports, folds, tools and checkpoint writes. A per-frame SQL read cannot prevent delayed publication.

- **Dimension 3.1**: Test `test_an_issued_reclaim_attempts_one_restart_publication` (integration): 100 claimers yield one issued reclaim; refused/fresh issues publish none.
- **Dimension 3.2**: Test `a newer attempt resets once and ignores delayed older output` (unit): fresh-event/midstream first observation, explicit first-seen restart, lost/reordered/duplicate hints, every output kind and old decoder callbacks.
- **Dimension 3.3**: Test `a second start in the same attempt remains a gap` (unit): second sequence zero without a greater fence enters repair.
- **Dimension 3.4**: Test `test_a_superseded_lease_can_neither_save_nor_settle` (integration): stale execution/checkpoint/fold/native-effect writes and final settlement fail; authenticated known-request usage may still be recorded idempotently against the old lease.
- **Dimension 3.5**: Test `prints one restart line for its own event and attempt` (unit): another event, duplicate hint and stale chunks print no restart/text.
- **Dimension 3.6**: Test `test_delayed_cached_activity_cannot_replace_a_new_attempt` (integration): pause publication after cache lookup; issue replacement; release old publish.

### Section 4: Restore the correct kind of checkpoint

A checkpoint means the last complete committed snapshot. It never means a partially streamed token prefix or a checkpoint upload awaiting commit.
Carry predecessor ID for diagnostics, but resolve recovery by stable event or task identity. Repeated failures must not lose an older valid snapshot.

```text
new lease L2/F8 for same event E
  |
  +-- durable task T/generation G
  |     +-- committed checkpoint -> verify pins/digests/toolbox -> files + history
  |     |                            -> covered inputs + confirmed effects -> model
  |     +-- no checkpoint -------> accepted initial target -> normal task start
  |     +-- corrupt/unavailable --> stop restore; NO model/tools; existing failure policy
  |
  +-- ordinary chat E
        +-- NEW event-owned checkpoint -> history + matching fold prefix -> resume
        +-- absent checkpoint ---------> replay original event + unsettled folds
        +-- present invalid state ------> refuse restore; summary alone is insufficient
```

**Existing task path:** `task::lease::attach_task` freezes `TaskContext` during issue. The supervisor restores files and history through `Restore::run`.
Use checkpoint `covered_input_sequence` and `effect_watermark`, then reconcile native receipts/effects. Task leases do not call the plain-chat fold inbox.
A corrupt task checkpoint must not fall back to an empty workspace. Restore refusal must prevent the model and tools from starting.

**Missing task work:** add `RecoveryBoundarySink::save` at proved safe boundaries; existing task checkpoint commit already updates the live task pointer.
Reuse `Extension::after_group` plus answer-only continuation/terminal handling, but first quiesce children and executor sessions with supervisor-owned workspace access.
Unsafe boundaries keep the previous snapshot; cancellation must thaw or destroy safely. A first-slice kill can still precede every complete checkpoint.

**Proposed plain-chat path:** format one binds full event-owned history and exact fold IDs/sequences; preserve IDs in `Folded` instead of guessing from a cursor. Offers carry descriptors; a fenced native read returns history, with complete save/read messages each capped at one MiB under the two-MiB native limit. Encrypt with `Sealer`/`Envelope`; validate scope/model/setup and replay digest; invalid-present state refuses, while absence starts clean.

```text
m1 claimed + incorporated -> snapshot(history includes m1, prefix=m1) COMMIT
m2 claimed -> A dies before next snapshot
B restores snapshot -> atomically adopt covered m1 claim under F8 -> no model replay m1
                   -> claim suffix m2 under F8 -> model sees m2 -> successful report
                   -> settle m1 AND m2 once under B's lease and fence
```

A restored cursor alone is insufficient: `settle_folds.sql` requires the current lease ID and fence on each covered admission.
Reassign covered claims to the new holder under its live fence before skipping their delivery. No checkpoint means replaying all unsettled folds.
Reject or sanitize incompatible setup/model state before restore. A claim receipt alone never proves that the saved conversation includes its message.
Keep model-request replay separate from delivery deduplication: work after the last committed snapshot can run again after failure.

**Billing:** Every lease has its own meter cursor and usage-line identity. Bill the old lease only through its deadline, even if reclaim already marked it expired.
Re-resolve gates/provider/model at the replacement issue; current code does not carry the dead lease's billing posture forward. Preserve uniqueness and final admission settlement.
The expiry biller currently waits 120s after the lease deadline (`lease/billing_sweep.rs::GRACE`). Do not require that charge to appear at reclaim.

**Readiness blockers:** finalize all-work quiescence, supervisor lifetime/cancellation and plain-chat filesystem consistency; the companion pins proposed interfaces, encryption, receipt/replay and bounded storage defaults.
A fresh-disk chat restore can claim tool results whose files vanished; it needs an explicit policy and owner disposition, not a blanket fallback.

- **Dimension 4.1**: Test `test_reclaim_restores_the_last_complete_task_checkpoint` (integration): committed files/history restore; pending upload is ignored; a second kill still finds the committed generation.
- **Dimension 4.2**: Test `test_unreadable_task_checkpoint_never_starts_the_model` (integration): tamper, unavailable storage and toolbox mismatch invoke no model/tools.
- **Dimension 4.3**: Test `test_plain_turn_checkpoint_and_fold_prefix_commit_together` (integration): kill before/after save and lose acknowledgement; retry never advances only the cursor.
- **Dimension 4.4**: Test `test_respawn_adopts_covered_folds_without_model_replay` (integration): checkpoint covers m1; m2 follows; new report settles both once.
- **Dimension 4.5**: Test `test_reclaim_without_checkpoint_replays_all_unsettled_folds` (integration): first-checkpoint kill loses no input; successful report settles the new claims.
- **Dimension 4.6**: Test `test_a_respawned_turn_bills_each_slice_once` (integration): expiry and reclaim race, repeated sweep/report, changed provider resolution; no duplicate usage or settlement.
- **Dimension 4.7**: Test `test_reclaim_respects_placement_gates_and_attempt_limits` (integration): M246 `own_only`, tags, capacity, funds, approval and exhausted task/plain attempt limits prevent unauthorized issue.

### Section 5: Show quarantine through current operator surfaces

Add optional `cordon` to current runner reads, with reason and timestamp. Automatic quarantine leaves the operator status visible independently.
`agentsfleet platform runners` has `list`, `show`, `events` and `leases`; `runner:read` is required. Add cordon reason/time and return rule to operator reads; tenant-owned list visibility remains M246 work. Add `runner_uncordoned` to both server and CLI enums.
The deleted web admin pages are not implementation targets. The browser receives the chat marker from Section 3.

```text
GET /v1/admin/runners/{runner_id} -> status=active + cordon=missed_heartbeats
                                  -> platform runners show -> reason + lift rule
30s healthy beats -> cordon absent -> runner_uncordoned history
operator status=cordoned ---------> stays cordoned after healthy beats
```

- **Dimension 5.1**: Test `test_runner_reads_carry_an_automatic_cordon` (integration): actual routes include reason/time, omit absent cordon, retain operator status and history.
- **Dimension 5.2**: Test `platform runners shows automatic cordon and lift events` (unit): schema decoding, text rendering and JSON output retain the new data.

### Section 6: Prove failure through real processes

Run two real runner processes against the real daemon router, Postgres and core NATS. Use a local streaming model server and real native runner sessions.
Synchronize on checkpoint commit and the first replacement chunk. Kill only the intended process; do not infer checkpoint completion from streamed text.

```text
A commits checkpoint -> A streams -> SIGKILL A -> wait until saved deadline
                                                |
                         eligible B offer <-----+-- wake or 5s recovery probe
                         restore -> newer attempt -> final report -> one settlement
A remains silent -> 105s threshold + paced sweep -> auto cordon -> operator read
```

The 5s probe bounds detection under available capacity and healthy dependencies, not every queue/lock/gate/restore delay.
After build and focused unit checks, prepare the dedicated container lab and controllable checkpoint/action barrier before Indy follows the companion manual check. Observe committed receipt, pause A, verify takeover/restore, then unpause; the task-file variant needs snapshot-capable sandboxes and configured storage. Automated integration and hard-kill checks follow the human result; acceptance retains its separate authorization.

- **Dimension 6.1**: Test `killed_runner_mid_turn_restarts_on_another_host` (e2e): higher fence, same event, no issue before expiry, restored progress, final settlement once.
- **Dimension 6.2**: Test `stream_reconnect_before_deadline_keeps_the_same_lease` (e2e): reconnect/Hello before expiry rebuilds ownership without a restart marker or new fence.
- **Dimension 6.3**: Test `committed_issue_with_lost_offer_reclaims_after_its_deadline` (integration): crash after commit/before offer; no phantom completion or permanent lock.
- **Dimension 6.4**: Test `terminal_or_killed_fleet_is_never_respawned` (integration): report/kill before reclaim or during issue prevents a new runnable attempt.
- **Dimension 6.5**: Test `claim_without_issue_recovers_after_reservation_expiry` (integration): daemon dies after claim; no issued lease or restart hint; the expired reservation becomes claimable.
- **Dimension 6.6**: `manual_recovery_before_automated_lanes` (manual): Indy records receipt, event/runner/lease/fence/deadline, restore proof and quarantine/lift from the companion procedure; streamed text alone fails this proof. Evidence goes in Pull Request Session Notes, not private transcripts.

## Failure Modes and ASCII use-case atlas

`X` means process/host loss; `F+1` means a new slot fence. Each row draws a distinct branch of Sections 1-6.

| Scenario | ASCII path | Observable result / proof |
|---|---|---|
| Healthy run | `A -> renew -> model/tools -> report -> settled` | No restart; Section 6 baseline fixture. |
| Hard host death | `A:X -> deadline -> B/F+1 -> restore -> report` | Replacement after expiry; Dimension 6.1. |
| Brief stream break | `A -> link:X -> reconnect/Hello < deadline -> A` | Same lease and fence; Dimension 6.2. |
| Partition exceeds deadline | `A --X-- daemon -> expiry -> B/F+1; A returns -> stale` | Old durable writes fail; old output drops; 3.4, 3.6. |
| Heartbeats lost, renewals live | `A:renew OK + beats:X -> cordon -> existing lease continues` | Sweep changes no lease deadline; 2.3. |
| Old sweep observation | `sweep reads stale -> A beats -> sweep locks -> fresh` | No false quarantine; 2.6. |
| Offline/cordon write fails | `offline + cordon:X -> ROLLBACK -> next sweep retries` | No half-recorded episode; 2.2. |
| Stable return | `auto cordon -> healthy beats 0..30s -> lift` | One lift event; 2.5. |
| Flapping return | `beat -> gap >=20s -> beat -> reset 30s proof` | No early lift; 2.5. |
| Operator action during return | `auto cordon -> operator cordon/drain/revoke -> beats` | Operator intent stays; 2.7. |
| Existing selection after cordon | `candidate selected -> cordon commits -> issue refuses` | No new work, even without offer advice; 2.4. |
| Host full with holds | `[lease][hold] -> cold fleet / authoring -> refuse` | Includes inlined fleet count; 1.1. |
| Own hold reused | `[lease][own hold] -> issue -> [lease][resumed lease]` | No double charge; 1.2, 1.4. |
| Keeper shrinks or grows busy | `busy + held -> reserve exceeded -> oldest hold retired -> beat` | Last free worker preserved; 1.5. |
| Stale hold at renewal | `live lease + old hold row -> renewal excludes holds -> extend` | Existing turn survives; 1.3. |
| Expired report creates no hold | `expired A + full host -> late report -> settle usage, hold=None` | Local parked hold is released; 1.8. |
| Authoring consumes shared worker | `authoring A + held H -> reserve exceeded -> retire H` | All work counts once; 1.7. |
| Busy drain does not starve sweep | `101 busy drains -> actionable/fair scan -> new silent host cordoned` | Both fleet and authoring deadlines count; 2.8. |
| Concurrent reclaims | `B,C,... -> expired slot -> one F+1 issue; others lose` | One issued winner/publication attempt; 3.1. |
| Kill before first checkpoint | `A:X -> no committed snapshot -> original input + pending folds` | Inputs retained; task starts accepted target; 4.5. |
| Pending/partial checkpoint | `upload -> X before commit -> last committed generation` | No partial restore; 4.1, 4.3. |
| Repeated failures | `L1 saves C -> L2:X before save -> L3 loads C` | Stable event/task ownership finds C; 4.1. |
| Corrupt task snapshot | `checkpoint -> validation:X -> stop restore -> no model/tools` | Safe failure; 4.2. |
| Fold after checkpoint | `C[m1] -> m2 -> X -> adopt m1 -> deliver m2 -> settle both` | Model prefix and ownership agree; 4.4. |
| Restart hint dropped/duplicated | `hint:X or hint,hint -> higher-fence output -> one reset` | Reply remains coherent; 3.2, 3.5. |
| Cross-replica frame reordering | `F8 output -> delayed F7 output -> drop F7` | Covers all output types; 3.6. |
| Same-attempt stream gap | `F8 seq0 -> seq2 -> gap -> saved final reply` | Real gaps still repair; 3.3. |
| Issue committed, offer lost | `L2 COMMIT -> daemon:X -> L2 expires -> L3/F+1` | No claim that offer was acknowledged; 6.3. |
| Claim committed, no lease issued | `slot claim -> daemon:X -> reservation expires -> claim again` | No false restart hint or lost input; 6.5. |
| Same host recovers after expiry | `A:X -> A returns healthy -> old lease expires -> A/F+1` | New attempt; no promise of a different physical host; 4.7. |
| No eligible replacement | `expiry -> tags/own_only/capacity/gates refuse -> wait` | No ownership bypass; 4.7. |
| Funds/attempt/task limits exhausted | `expiry -> eligibility/gate denies -> existing refusal/terminal path` | No endless reclaim or bypass; 4.7. |
| Completion/kill wins race | `report or fleet kill COMMIT -> reclaim -> no runnable event` | No restart of terminal work; 6.4. |
| Billing workers race | `expiry sweep + reclaim + late report -> per-lease cursor + unique lines` | Bill each slice once; 4.6. |
| Checkpoint store unavailable | `save/restore -> dependency:X -> retain prior snapshot / refuse task restore` | Acknowledgement never invents durability; 4.2, 4.3. |

## Interfaces

- **Restart hint:** `{"kind":"run_restarted","event_id":"0193a4c2-5d1e-7b3a-9f10-2c4d6e8f0a1b","fencing_token":"8","restarted_at":"2026-11-17T10:43:12.000Z"}`.
- **Attempt output:** all daemon-published output includes the same decimal-string `fencing_token`. Validate before comparison; decimal strings retain the complete database integer in JavaScript.
- **Reset rule:** first ordinary output initializes silently; an explicit first-seen restart can mark restart. A greater observed event fence resets once; equal-fence hints do nothing, lower-fence frames drop, and same-fence gaps still repair.
- **Runner reads:** `/v1/admin/runners` and `/v1/admin/runners/{runner_id}` gain optional `cordon: {reason: "missed_heartbeats", cordoned_at: <instant>}`.
- **Runner events:** add `runner_uncordoned`; `runner_cordoned` automatic metadata includes `reason`. Server and platform CLI enum consumers change together.
- **Capacity:** extend `capacity::lock(transaction, guard, claimant)` without weakening `Guard` authority. `Claimant::{Fleet,Authoring,Existing}` controls holds; fleet issue retains its inlined census.
- **Task recovery:** reuse `LeasePayload::task`, `TaskContext::{checkpoint_id,checkpoint_generation,covered_input_sequence,confirmed_effect_watermark}`. Task generation, checkpoint generation and lease fence are separate counters.
- **Plain recovery:** proposed native save/acknowledgement, `TurnCheckpoint`/`TurnResume`, event-owned generation and fold adoption remain unfinalized; implementation cannot substitute a memory-summary pointer.
- **Error behavior:** no HTTP error is promised for one-way progress. Durable stale writes retain their existing ownership errors; finalize new checkpoint validation errors with its surface.

## Invariants

1. **Reclaim authority:** the slot's expired deadline and guarded fence increment decide ownership. Liveness changes no deadline; test 2.3 is the counterexample.
2. **Capacity authority:** host-row lock, claimant-aware count and guarded issue commit prevent over-admission. The inlined census has the same rule; test 1.1.
3. **Settlement authority:** current admission/slot fence and `settled_at IS NULL` guard settlement; usage lines retain their uniqueness and meter cursor; tests 3.4, 4.6.
4. **Quarantine authority:** host-row automatic fields and operator status are separate; only continuous proved heartbeats lift automatic fields; tests 2.5-2.7.
5. **Output authority:** daemon-stamped attempt fence and each consumer's maximum fence prevent older output from mutating a replacement; tests 3.2, 3.6.
6. **Recovery authority:** complete committed snapshot plus live fenced adoption controls restored state; cursor and history never advance separately; tests 4.1-4.5.

## Metrics & Observability

| Signal | Trigger | Allowed properties | Proof |
|---|---|---|---|
| `agentsfleet_runner_cordon_changes_total` | Committed automatic cordon or lift | `change=cordoned` or `lifted`; no host IDs in labels | 2.1, 2.5 and producer/census checks |
| `runner_cordoned`, `runner_cordon_lifted` logs | Same committed transitions | Runner ID, reason, last seen; never tokens | 2.1, 2.5 |
| Recovery failure logs | Snapshot validation/storage failure | Error code, lease/predecessor, checkpoint identity; no contents or signed URLs | 4.2, 4.3 |
| `respawn_checkpoint_committed`, `respawn_checkpoint_restored` debug logs | Verified committed receipt; completed file/history restore before model/tools | Event/task, lease, fence, checkpoint identity/generation; no bytes, keys or signed URLs | 6.6; automated checkpoint proofs |

Existing `agentsfleet_fleet_runs_started_total{kind="reclaimed"}` continues counting reclaimed claims. It does not prove a lease was delivered or restored.

## Acceptance Rubric (single scoring surface)

| # | Observable outcome | Verify | Expected | Priority | Graded |
|---|---|---|---|---|---|
| R1 | Real killed runner restores and settles once | `make test-integration-rustd` | `killed_runner_mid_turn_restarts_on_another_host ... ok` | P0 | |
| R2 | Atomic quarantine, no starvation or forced expiry, safe return | `make test-integration-rustd` | Dimensions 2.1-2.8 pass | P0 | |
| R3 | Both capacity paths count holds; late reports and authoring preserve reserve | `make test-integration-rustd` | Dimensions 1.1-1.4 and 1.8 pass; S2 covers 1.5-1.7 | P0 | |
| R4 | Delayed output cannot mix attempts | `make test-unit-all` | `a newer attempt resets once and ignores delayed older output` passes | P0 | |
| R5 | Chat snapshot and fold adoption lose no input | `make test-integration-rustd` | Dimensions 4.3-4.5 pass; blocked until recovery design closes | P0 | |
| R6 | Task restore refuses invalid state before model | `make test-integration-rustd` | Dimension 4.2 passes | P0 | |
| R7 | Placement, gates, limits and billing stay fenced | `make test-integration-rustd` | Dimensions 4.6-4.7 pass | P0 | |
| R8 | CLI separates attempts and shows cordon | `make test-unit-all` | Dimensions 3.5 and 5.2 pass | P1 | |
| R9 | No forced-expiry implementation remains | `git grep -n -w -e EXPIRE_ACTIVE_LEASE_SLOTS -e expire_slots -e release_slots -- rustd` | No matches; exit 1 | P0 | |
| R10 | New public shapes are generated | `rg -n 'run_restarted|runner_uncordoned|missed_heartbeats' public/openapi.json` | All three spellings present | P1 | |
| R11 | Architecture prose passes | `orly gate verify` | Zero `docs.language` findings | P1 | |
| R12 | Matching docs branch exists | `git -C ~/Projects/docs rev-parse --verify chore/m247-respawn-changelog` | Exit 0 | P1 | |
| S1 | Lint | `make lint-all` | Exit 0 | P0 | |
| S2 | Unit | `make test-unit-all` | Exit 0 | P0 | |
| S3 | Integration | `make test-integration-rustd` | Exit 0 | P0 | |
| S4 | Version | `make check-version` | Exit 0 | P0 | |
| S5 | No secrets | `gitleaks detect --redact --no-banner` | Exit 0 | P0 | |

## Dead Code Sweep

Delete `expire_active_lease_slots.sql`, its constant, `expire_slots`, `release_slots`, `EXPIRE_PAST` and `CONTEXT_SLOTS`.
Use the R9 word-boundary search for runtime code. Historical records under `docs/v2/` retain their original references.
Do not add the draft's nonexistent `Reused`, `lease/reclaim.rs`, `select_lease_target.sql`, numbered migration or deleted admin web pages.

## Out of Scope

Host-down broadcasts, memory/processor packing, operator uncordon actions and exactly-once external tool effects remain outside this milestone.
Task workspace restore already exists and is in scope for reuse. Plain-chat workspace consistency is an open design decision, not an accepted omission.

## Product Clarity (authoring record)

1. **Successful moment:** A chat finishes under one replacement attempt; the user sees why the provisional reply restarted.
2. **Preserved behavior:** Healthy runs, real-gap repair, held follow-ups, operator intent and task authority continue.
3. **Optimal way:** Reuse expiry, fences, push offers and native task restore; add no host-down bus authority.
4. **Rebuild versus iterate:** Quarantine/capacity/output are local changes. Ordinary chat recovery needs a real writer, snapshot and reader.
5. **What we build:** Hold accounting, automatic quarantine/lift, attempt-aware output, checkpoint recovery and operator API/CLI visibility.
6. **What we do not build:** Resource packing or an operator dashboard revival.
7. **Fit:** M246 owns placement; task input receipts and ordinary chat folds keep their separate meanings.
8. **Surface order:** Daemon output fences and both clients ship together; keeper reserve and capacity count ship together.
9. **Dashboard restraint:** Chat marker only in the browser; current platform CLI serves operators.
10. **Next step for a confused user:** Read the restart marker or the runner's quarantine reason and return rule.

## Decomposition & alternatives

Build Section 1, Section 2 and Section 3 independently; Section 4 must close its design blockers before recovery implementation.
Section 5 uses the cordon fields; Section 6 proves the complete wired result. Do not label checkpoint work a pointer-only patch.

Rejected: auto-cordon in operator `status`, event-history-derived healthy proof, SQL reads per activity chunk, and a best-effort restart hint as the sole reset authority.
Rejected: treating a compaction summary as execution state, restarting a corrupt task from empty disk, or skipping folded inputs without adopting their claims.
A durable restart outbox is unnecessary for cosmetic reset when attempt-tagged output repairs a lost hint. Durable work and checkpoint publication still use Postgres.

## Discovery (consult log)

- **Build opening:** Indy: "okay start implementation using orly-build". Worktree `feat/m247-checkpoint-respawn` starts at `97224ed7432c9dd124ea8797d4324156b7022c07`; Section 4 gates remain open.
- **Review model workflow:** Indy manually changes models when requested. Ask and stop before a review requiring a model switch; do not substitute a model-selected subagent.

- **Current source review:** `git fetch origin main` returned HEAD and `origin/main` `97224ed7432c9dd124ea8797d4324156b7022c07`. PR #803 merged as `67f5f7d19`.
- **Model reviews:** `gpt-6-astra`, reasoning `ultra`: initial review found 12 issues, all accepted. Final review found four clarifications and one diagram correction, all accepted. Revisions add atomic keeper transfers, degraded-health resets, mutation-time connection proof and first-observation semantics; recovery design blockers remain open.
- **Review validation:** `orly doctor`, `docs.language` and `git diff --check` pass. The initial `orly gate verify` stopped at missing `cmake`; CMake 4.4.5 is now installed. The full gate has not been rerun; implementation and manual rubric rows remain ungraded.
- **User request, Nov 17, 2026:** "if the doc is stale use ASTRA and review and update." This authorizes this source review and document update.
- **Original owner decisions retained:**
  > Indy (Nov 06, 2026: 08:50 AM): "And we are not in production yet, so i want to rethink the approach on admissions, lease, and so on as i am focussed on scale. If something fails in one host bare metal, it should automatically cordon and respawn in another runner"
  > Indy (Nov 06, 2026: 10:19 AM): "This will be on by default durable_tasks_v1. ... my plan also was to have folks spin a runner and point to the agentsfleetd (in cloud) and keep using it."
  > Indy (Nov 06, 2026: 10:43 AM): "1. Yes placement rules are via the tags and labels 2. Respawn across ownership - this is a tenant setting ... Based on the scope i would like to build this ... 4. Yes, but donot overengineer now, we can improve this."
  > Indy (Nov 06, 2026: 11:57 AM): "Yes i am ok with postgres store and accepting messages, fences runs"
  > Indy (Nov 06, 2026: 04:18 PM): "yes" — context: the original author asked whether each milestone should drop schema it leaves without readers or writers.
- **Full-spec review requested Nov 17, 2026:** `gpt-6-astra`, reasoning `ultra`, reviewed the entire specification and source: nine material findings accepted. Seven major amendments cover late holds, authoring occupancy, drains, exact operator commands, metering exceptions, provisional trace lifetime and the human smoke. Two critical blockers remain: plain-chat state/disk consistency and a safe live-capture handoff. A focused follow-up found four further interface/proof issues: preserve fold IDs, add fenced checkpoint reads, acknowledge accepted holds and prepare a controlled manual lab. Those amendments are included; lab setup remains ungraded.
- **Implementation defaults:** Separate quarantine from lease expiry; host-row healthy-return proof; attempt fences on every output; stable event/task checkpoint ownership.
- **Open blockers:** Ordinary-chat full-state format/bound and disk consistency; native save/acknowledgement; live task safe-boundary capture/freeze ordering; final M246 placement names and behavior.
- **Owner disposition still needed before implementation:** Plain-chat filesystem/tool replay policy. The original draft proposed fresh disk and repeatable effects but recorded no acceptance.
- **Deferrals:** None approved. Recovery remains in this milestone's P0 rubric; keeping this spec pending does not defer its missing implementation.
