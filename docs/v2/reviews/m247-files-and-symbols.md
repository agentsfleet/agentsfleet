# M247 files and symbols

This table compares the implementation worktree with `origin/main` (`97224ed7432c9dd124ea8797d4324156b7022c07`).
It includes tracked and untracked delivery paths, including tests and documentation.
Renamed files appear as their removed and added paths.

`+` means added, `~` means the declaration changed, and `-` means removed.
Functions, methods, traits, structs, enums, interfaces, types and module values come from Rust and TypeScript syntax trees.
A changed enclosing type also appears when its fields or variants changed.

The last column names SQL definitions or non-code changes.
The [implementation guide](m247-implementation-and-manual-check.md) explains behavior, transactions, limits, commands and the manual procedure.
This inventory describes source changes; it does not grade acceptance criteria or imply formal verification.

| File | Change | Functions / methods | Traits | Structs / enums / types | SQL / values / other |
|---|---|---|---|---|---|
| `VERSION` | EDIT | — | — | — | `documentation/build/version metadata` |
| `cli/package.json` | EDIT | — | — | — | `documentation/build/version metadata` |
| `cli/src/commands/fleet/attempt-output.ts` | ADD | `+ AttemptOutput::accept`; `+ AttemptOutput::constructor` | — | `+ AttemptOutput` | `+ FENCE`; `+ MAX_FENCE`; `+ OUTPUT_KINDS` |
| `cli/src/commands/fleet/steer-events.ts` | EDIT | `~ makeFrameCallback` | — | — | — |
| `cli/src/commands/fleet/task-activity.ts` | EDIT | `+ ActivityPrinter::reset` | — | `~ ActivityPrinter` | — |
| `cli/src/commands/fleet/tool-activity.ts` | EDIT | `+ ToolActivityPrinter::reset` | — | `~ ToolActivityPrinter` | — |
| `cli/src/commands/platform/runner-schema.ts` | EDIT | — | — | — | `~ runner` |
| `cli/src/commands/platform/runner-vocabulary.ts` | EDIT | — | — | `+ CordonReason`; `~ RunnerEventType` | — |
| `cli/src/commands/platform/runners.ts` | EDIT | — | — | — | `~ platformRunnerShow`; `~ platformRunnersList` |
| `cli/test/acceptance/streaming-follow.spec.ts` | EDIT | `~ fetch`; `~ startPhasedStub` | — | — | — |
| `cli/test/fleet-steer-attempt.unit.test.ts` | ADD | — | — | — | `module/test wiring` |
| `cli/test/fleet-steer-tail.unit.test.ts` | EDIT | `~ chunkFrame` | — | — | — |
| `cli/test/helpers-fleet-steer.ts` | EDIT | `~ eventStream` | — | — | — |
| `cli/test/platform-runners.unit.test.ts` | EDIT | — | — | — | `module/test wiring` |
| `docs/architecture/lease_flow.md` | EDIT | — | — | — | `documentation/build/version metadata` |
| `docs/architecture/observability.md` | EDIT | — | — | — | `documentation/build/version metadata` |
| `docs/architecture/push_scheduler.md` | EDIT | — | — | — | `documentation/build/version metadata` |
| `docs/architecture/runner_execution.md` | EDIT | — | — | — | `documentation/build/version metadata` |
| `docs/architecture/runner_fleet.md` | EDIT | — | — | — | `documentation/build/version metadata` |
| `docs/architecture/scaling.md` | EDIT | — | — | — | `documentation/build/version metadata` |
| `docs/metrics.census.tsv` | EDIT | — | — | — | `documentation/build/version metadata` |
| `docs/v2/active/M247_001_P1_API_CLI_DOCS_OBS_UI_FAILED_HOSTS_RESPAWN_FROM_CHECKPOINT.md` | ADD | — | — | — | `documentation/build/version metadata` |
| `docs/v2/pending/M247_001_P1_API_CLI_DOCS_OBS_UI_FAILED_HOSTS_RESPAWN_FROM_CHECKPOINT.md` | DELETE | — | — | — | `documentation/build/version metadata` |
| `docs/v2/reviews/m247-files-and-symbols.md` | ADD | — | — | — | `documentation/build/version metadata` |
| `docs/v2/reviews/m247-implementation-and-manual-check.md` | ADD | — | — | — | `documentation/build/version metadata` |
| `docs/v2/reviews/m247-review-ledger.md` | ADD | — | — | — | `documentation/build/version metadata` |
| `docs/v2/reviews/m247-scalability-and-refactor.md` | ADD | — | — | — | `documentation/build/version metadata` |
| `make/test-integration-rustd.mk` | EDIT | — | — | — | `documentation/build/version metadata` |
| `public/openapi.json` | EDIT | — | — | — | `documentation/build/version metadata` |
| `rustd/Cargo.lock` | EDIT | — | — | — | `documentation/build/version metadata` |
| `rustd/Cargo.toml` | EDIT | — | — | — | `documentation/build/version metadata` |
| `rustd/crates/daemon/afd_api/tests/harness/native/stores.rs` | EDIT | `~ Fleet::authoring_gateway` | — | — | — |
| `rustd/crates/daemon/afd_api/tests/openapi_stream_frames.rs` | EDIT | `~ one_of_each`; `~ slot` | — | — | `~ VARIANTS` |
| `rustd/crates/daemon/afd_api_operator/src/handler/operator/runners.rs` | EDIT | `~ item` | — | — | — |
| `rustd/crates/daemon/afd_api_tenant/src/handler/stream/mod.rs` | EDIT | `~ fleet` | — | — | — |
| `rustd/crates/daemon/afd_api_wire/src/admin/mod.rs` | EDIT | — | — | `~ RunnerEventType` | — |
| `rustd/crates/daemon/afd_api_wire/src/admin/tests.rs` | EDIT | `~ every_runner_history_value_serializes_as_the_spelling_postgres_stores` | — | — | — |
| `rustd/crates/daemon/afd_api_wire/src/operator.rs` | EDIT | `+ CordonReason::as_str` | — | `+ CordonReason`; `+ RunnerCordon`; `~ RunnerItem` | — |
| `rustd/crates/daemon/afd_api_wire/src/tail/mod.rs` | EDIT | — | — | `~ TailFrame` | — |
| `rustd/crates/daemon/afd_authoring/src/draft/mod.rs` | EDIT | `~ Drafts::check_connection`; `~ Drafts::lock_host_capacity` | — | — | — |
| `rustd/crates/daemon/afd_authoring/src/lease/claim/mod.rs` | EDIT | `~ Drafts::admit` | — | — | — |
| `rustd/crates/daemon/afd_authoring/src/lease/renew.rs` | EDIT | `~ Drafts::renew` | — | — | — |
| `rustd/crates/daemon/afd_bench/src/lane/lease/drain/stage.rs` | EDIT | `~ plane` | — | — | — |
| `rustd/crates/daemon/afd_db/src/migration.rs` | EDIT | — | — | — | `~ MIGRATIONS` |
| `rustd/crates/daemon/afd_db/tests/integration_migrate/immutable.rs` | EDIT | — | — | — | `~ IMMUTABLE` |
| `rustd/crates/daemon/afd_db/tests/integration_migrate/roles.rs` | EDIT | — | — | — | `~ GRANTS` |
| `rustd/crates/daemon/afd_fleet/src/error/classify/availability.rs` | EDIT | `~ Error::is_datastore_unavailable`; `~ Error::remote_cooldown`; `~ Error::task_timeout_reason` | — | — | — |
| `rustd/crates/daemon/afd_fleet/src/error/classify/code.rs` | EDIT | `~ Error::code` | — | — | — |
| `rustd/crates/daemon/afd_fleet/src/error/classify/decision.rs` | EDIT | `~ Error::decision_failure_reason` | — | — | — |
| `rustd/crates/daemon/afd_fleet/src/error/classify/detail.rs` | EDIT | `~ Error::detail` | — | — | — |
| `rustd/crates/daemon/afd_fleet/src/error/classify/task.rs` | EDIT | `~ Error::task_current_state`; `~ Error::task_precondition_tag` | — | — | — |
| `rustd/crates/daemon/afd_fleet/src/error/kind.rs` | EDIT | — | — | `~ ErrorKind` | — |
| `rustd/crates/daemon/afd_fleet/src/error/permanence.rs` | EDIT | `~ Error::is_config_permanent` | — | — | — |
| `rustd/crates/daemon/afd_fleet/src/lease/activity/published.rs` | EDIT | `~ Published::of`; `+ decimal` | — | `+ Frame`; `~ Published` | — |
| `rustd/crates/daemon/afd_fleet/src/lease/activity/tests.rs` | EDIT | `~ tool_frames_republish_their_call_id_and_a_chunk_names_none` | — | — | — |
| `rustd/crates/daemon/afd_fleet/src/lease/answer/mod.rs` | EDIT | `~ payload` | — | `~ Context` | — |
| `rustd/crates/daemon/afd_fleet/src/lease/answer/tests.rs` | EDIT | `~ rendered` | — | — | — |
| `rustd/crates/daemon/afd_fleet/src/lease/assign/ledger.rs` | EDIT | `~ Admission::acquired`; `+ OpenedRow::delivery` | — | `~ Admission` | — |
| `rustd/crates/daemon/afd_fleet/src/lease/bracket.rs` | EDIT | `+ Leases::publish_restarted`; `~ tests::leased` | — | — | — |
| `rustd/crates/daemon/afd_fleet/src/lease/commit/mod.rs` | EDIT | `~ Leases::settled` | — | `~ Committed` | — |
| `rustd/crates/daemon/afd_fleet/src/lease/commit/steps.rs` | EDIT | `~ tests::settled` | — | — | — |
| `rustd/crates/daemon/afd_fleet/src/lease/deliver/issue.rs` | EDIT | `~ LeaseService::issue_ready`; `~ LeaseService::render_offer` | — | — | — |
| `rustd/crates/daemon/afd_fleet/src/lease/deliver/queue_wait_tests.rs` | EDIT | `~ acquired` | — | — | — |
| `rustd/crates/daemon/afd_fleet/src/lease/envelope.rs` | EDIT | — | — | `~ Acquired` | — |
| `rustd/crates/daemon/afd_fleet/src/lease/mod.rs` | EDIT | — | — | — | `module/test wiring` |
| `rustd/crates/daemon/afd_fleet/src/lease/pull/mod.rs` | EDIT | — | — | `~ LeaseService` | — |
| `rustd/crates/daemon/afd_fleet/src/lease/report/assembly.rs` | EDIT | `~ reconciled` | — | — | — |
| `rustd/crates/daemon/afd_fleet/src/lease/report/finalize.rs` | EDIT | `~ LeaseService::finalize_report`; `+ LeaseService::replayed_hold` | — | — | — |
| `rustd/crates/daemon/afd_fleet/src/lease/report/observations/tests.rs` | EDIT | `~ settled` | — | — | — |
| `rustd/crates/daemon/afd_fleet/src/lease/report/result.rs` | EDIT | — | — | `~ Reconciled` | — |
| `rustd/crates/daemon/afd_fleet/src/lease/sql/commit_report.sql` | EDIT | — | — | — | `SQL statement` |
| `rustd/crates/daemon/afd_fleet/src/lease/sql/lease/claim_admission_head.sql` | EDIT | — | — | — | `SQL statement` |
| `rustd/crates/daemon/afd_fleet/src/lease/sql/lease/claim_inputs.sql` | EDIT | — | — | — | `SQL statement` |
| `rustd/crates/daemon/afd_fleet/src/lease/sql/lease/insert_lease_with_event.sql` | EDIT | — | — | — | `SQL statement` |
| `rustd/crates/daemon/afd_fleet/src/lease/sql/lease/release_fleet_slot.sql` | EDIT | — | — | — | `SQL statement` |
| `rustd/crates/daemon/afd_fleet/src/lease/sql/lease/report_hold.sql` | ADD | — | — | — | `SQL statement` |
| `rustd/crates/daemon/afd_fleet/src/lease/sql/lease/select_ledger_candidates.sql` | EDIT | — | — | — | `SQL statement` |
| `rustd/crates/daemon/afd_fleet/src/lease/sql/report_statement.rs` | EDIT | `~ ReportAnswer::read` | — | `~ AnswerRow`; `~ SettledReport` | — |
| `rustd/crates/daemon/afd_fleet/src/lease/store.rs` | EDIT | `~ Leases::lock_host_capacity` | — | — | — |
| `rustd/crates/daemon/afd_fleet/src/lease/test_dead.rs` | EDIT | `~ acquired`; `~ acquired_installed`; `~ plane` | — | — | — |
| `rustd/crates/daemon/afd_fleet/src/lease/turn_checkpoint/allocation.rs` | ADD | `+ create`; `+ head`; `+ insert`; `+ room` | — | — | — |
| `rustd/crates/daemon/afd_fleet/src/lease/turn_checkpoint/commit.rs` | ADD | `+ custody`; `+ manifest`; `+ pending`; `+ publish`; `+ readable`; `+ swap`; `+ validate`; `+ write` | — | `+ Publication` | `+ READ_ENVELOPE_BYTES` |
| `rustd/crates/daemon/afd_fleet/src/lease/turn_checkpoint/files.rs` | ADD | `+ FileOwner::commit`; `+ FileOwner::fresh`; `+ FileOwner::lock`; `+ public_failure` | — | `+ FileOwner` | — |
| `rustd/crates/daemon/afd_fleet/src/lease/turn_checkpoint/folds.rs` | ADD | `+ adopt`; `+ validate` | — | `+ ClaimRow` | — |
| `rustd/crates/daemon/afd_fleet/src/lease/turn_checkpoint/guard.rs` | ADD | `+ TurnLeaseGuard::commit`; `+ TurnLeaseGuard::fresh`; `+ TurnLeaseGuard::lock`; `+ TurnLeaseGuard::scope`; `+ admission`; `+ current`; `+ exact_fence`; `+ id`; `+ live`; `+ parents` | — | `+ Admission`; `+ Current`; `+ TurnLeaseGuard` | — |
| `rustd/crates/daemon/afd_fleet/src/lease/turn_checkpoint/guard_tests.rs` | ADD | `+ a_checkpoint_fence_must_equal_both_current_owners`; `+ a_lock_wait_cannot_reuse_a_pre_wait_deadline`; `+ api_only_cannot_discard_the_files_declared_by_a_capture`; `+ begin` | — | — | — |
| `rustd/crates/daemon/afd_fleet/src/lease/turn_checkpoint/head.rs` | ADD | `+ Leases::turn_resume` | — | — | — |
| `rustd/crates/daemon/afd_fleet/src/lease/turn_checkpoint/maintenance.rs` | ADD | `+ Leases::maintain_turn_checkpoints`; `+ collect`; `+ retain` | — | `+ Candidate` | `+ BATCH`; `+ PLANS_PER_OWNER`; `+ RETENTION_SECONDS` |
| `rustd/crates/daemon/afd_fleet/src/lease/turn_checkpoint/manifest.rs` | ADD | `+ Contents::append`; `+ complete`; `+ validate` | — | `+ Contents` | — |
| `rustd/crates/daemon/afd_fleet/src/lease/turn_checkpoint/mod.rs` | ADD | — | — | `+ TurnLease` | `+ PIN_SECONDS`; `+ READER`; `+ READY_GENERATIONS`; `+ TRANSFER_SECONDS`; `+ WRITER` |
| `rustd/crates/daemon/afd_fleet/src/lease/turn_checkpoint/objects.rs` | ADD | `+ attach`; `+ ids`; `+ matching`; `+ missing`; `+ ordered`; `+ protect`; `+ protect_registered`; `+ register`; `+ register_content`; `+ reserve`; `+ scoped`; `+ scoped_held` | — | — | `+ FILE`; `+ ORPHAN_SECONDS`; `+ WORKSPACE_BYTES` |
| `rustd/crates/daemon/afd_fleet/src/lease/turn_checkpoint/pages.rs` | ADD | `+ decode`; `+ link`; `+ read`; `+ saved_digest`; `+ selected`; `+ sizes_match`; `+ validate`; `+ write` | — | `+ Page` | — |
| `rustd/crates/daemon/afd_fleet/src/lease/turn_checkpoint/pins.rs` | ADD | `+ Pin::belongs`; `+ Pin::live`; `+ Pin::proves`; `+ Pin::wire`; `+ abandon`; `+ exact`; `+ insert`; `+ insert_on`; `+ reader`; `+ release`; `+ renew`; `+ update`; `+ update_owned`; `+ writer` | — | `+ Pin` | — |
| `rustd/crates/daemon/afd_fleet/src/lease/turn_checkpoint/plan.rs` | ADD | `+ Leases::begin_turn_checkpoint`; `+ replay`; `+ shape` | — | `+ Receipt` | — |
| `rustd/crates/daemon/afd_fleet/src/lease/turn_checkpoint/refusal.rs` | ADD | `+ Error::from`; `+ TurnCheckpointRefusal::code`; `+ TurnCheckpointRefusal::detail`; `+ TurnCheckpointRefusal::fmt` | — | `+ TurnCheckpointRefusal` | — |
| `rustd/crates/daemon/afd_fleet/src/lease/turn_checkpoint/refusal_tests.rs` | ADD | `+ recovery_refusals_are_safe_nonpermanent_domain_answers` | — | — | — |
| `rustd/crates/daemon/afd_fleet/src/lease/turn_checkpoint/restore.rs` | ADD | `+ adopt`; `+ read` | — | — | — |
| `rustd/crates/daemon/afd_fleet/src/lease/turn_checkpoint/root.rs` | ADD | `+ aad`; `+ fold_ids`; `+ invalid`; `+ open`; `+ seal`; `+ validate` | — | `+ Driver`; `+ History`; `+ Proof`; `+ Stored` | — |
| `rustd/crates/daemon/afd_fleet/src/lease/turn_checkpoint/root_tests.rs` | ADD | `+ a_corrupt_root_keeps_its_cause_and_hides_its_contents`; `+ a_root_cannot_silently_advance_only_its_fold_cursor`; `+ saved`; `+ saved_roots_bind_the_issued_model_and_each_scope_dimension`; `+ scope` | — | — | `+ ID`; `+ OTHER` |
| `rustd/crates/daemon/afd_fleet/src/lease/turn_checkpoint/row.rs` | ADD | `+ Plan::descriptor`; `+ Plan::folds`; `+ Plan::load`; `+ Plan::scope`; `+ Plan::wire` | — | `+ Plan` | — |
| `rustd/crates/daemon/afd_fleet/src/lease/turn_checkpoint/signing.rs` | ADD | `+ issue`; `+ issue_object`; `+ transfer` | — | `+ Batch` | — |
| `rustd/crates/daemon/afd_fleet/src/lease/turn_checkpoint/sql/abandon.sql` | ADD | — | — | — | `SQL statement` |
| `rustd/crates/daemon/afd_fleet/src/lease/turn_checkpoint/sql/abandon_stale.sql` | ADD | — | — | — | `SQL statement` |
| `rustd/crates/daemon/afd_fleet/src/lease/turn_checkpoint/sql/adopt_folds.sql` | ADD | — | — | — | `SQL statement` |
| `rustd/crates/daemon/afd_fleet/src/lease/turn_checkpoint/sql/commit.sql` | ADD | — | — | — | `SQL statement` |
| `rustd/crates/daemon/afd_fleet/src/lease/turn_checkpoint/sql/fresh_clock.sql` | ADD | — | — | — | `SQL statement` |
| `rustd/crates/daemon/afd_fleet/src/lease/turn_checkpoint/sql/head.sql` | ADD | — | — | — | `SQL statement` |
| `rustd/crates/daemon/afd_fleet/src/lease/turn_checkpoint/sql/head_allocate.sql` | ADD | — | — | — | `SQL statement` |
| `rustd/crates/daemon/afd_fleet/src/lease/turn_checkpoint/sql/head_insert.sql` | ADD | — | — | — | `SQL statement` |
| `rustd/crates/daemon/afd_fleet/src/lease/turn_checkpoint/sql/head_swap.sql` | ADD | — | — | — | `SQL statement` |
| `rustd/crates/daemon/afd_fleet/src/lease/turn_checkpoint/sql/lock_admission.sql` | ADD | — | — | — | `SQL statement` |
| `rustd/crates/daemon/afd_fleet/src/lease/turn_checkpoint/sql/lock_lease.sql` | ADD | — | — | — | `SQL statement` |
| `rustd/crates/daemon/afd_fleet/src/lease/turn_checkpoint/sql/lock_scope.sql` | ADD | — | — | — | `SQL statement` |
| `rustd/crates/daemon/afd_fleet/src/lease/turn_checkpoint/sql/lock_slot.sql` | ADD | — | — | — | `SQL statement` |
| `rustd/crates/daemon/afd_fleet/src/lease/turn_checkpoint/sql/maintenance_candidates.sql` | ADD | — | — | — | `SQL statement` |
| `rustd/crates/daemon/afd_fleet/src/lease/turn_checkpoint/sql/maintenance_delete.sql` | ADD | — | — | — | `SQL statement` |
| `rustd/crates/daemon/afd_fleet/src/lease/turn_checkpoint/sql/maintenance_head.sql` | ADD | — | — | — | `SQL statement` |
| `rustd/crates/daemon/afd_fleet/src/lease/turn_checkpoint/sql/maintenance_lock_head.sql` | ADD | — | — | — | `SQL statement` |
| `rustd/crates/daemon/afd_fleet/src/lease/turn_checkpoint/sql/maintenance_lock_pins.sql` | ADD | — | — | — | `SQL statement` |
| `rustd/crates/daemon/afd_fleet/src/lease/turn_checkpoint/sql/maintenance_lock_plans.sql` | ADD | — | — | — | `SQL statement` |
| `rustd/crates/daemon/afd_fleet/src/lease/turn_checkpoint/sql/maintenance_operations.sql` | ADD | — | — | — | `SQL statement` |
| `rustd/crates/daemon/afd_fleet/src/lease/turn_checkpoint/sql/maintenance_orphans.sql` | ADD | — | — | — | `SQL statement` |
| `rustd/crates/daemon/afd_fleet/src/lease/turn_checkpoint/sql/objects_scope.sql` | ADD | — | — | — | `SQL statement` |
| `rustd/crates/daemon/afd_fleet/src/lease/turn_checkpoint/sql/operation_insert.sql` | ADD | — | — | — | `SQL statement` |
| `rustd/crates/daemon/afd_fleet/src/lease/turn_checkpoint/sql/operation_replay.sql` | ADD | — | — | — | `SQL statement` |
| `rustd/crates/daemon/afd_fleet/src/lease/turn_checkpoint/sql/page_read.sql` | ADD | — | — | — | `SQL statement` |
| `rustd/crates/daemon/afd_fleet/src/lease/turn_checkpoint/sql/page_write.sql` | ADD | — | — | — | `SQL statement` |
| `rustd/crates/daemon/afd_fleet/src/lease/turn_checkpoint/sql/pages_allocate.sql` | ADD | — | — | — | `SQL statement` |
| `rustd/crates/daemon/afd_fleet/src/lease/turn_checkpoint/sql/pending.sql` | ADD | — | — | — | `SQL statement` |
| `rustd/crates/daemon/afd_fleet/src/lease/turn_checkpoint/sql/pin.sql` | ADD | — | — | — | `SQL statement` |
| `rustd/crates/daemon/afd_fleet/src/lease/turn_checkpoint/sql/pin_insert.sql` | ADD | — | — | — | `SQL statement` |
| `rustd/crates/daemon/afd_fleet/src/lease/turn_checkpoint/sql/pin_release.sql` | ADD | — | — | — | `SQL statement` |
| `rustd/crates/daemon/afd_fleet/src/lease/turn_checkpoint/sql/pin_renew.sql` | ADD | — | — | — | `SQL statement` |
| `rustd/crates/daemon/afd_fleet/src/lease/turn_checkpoint/sql/plan.sql` | ADD | — | — | — | `SQL statement` |
| `rustd/crates/daemon/afd_fleet/src/lease/turn_checkpoint/sql/plan_bytes.sql` | ADD | — | — | — | `SQL statement` |
| `rustd/crates/daemon/afd_fleet/src/lease/turn_checkpoint/sql/plan_insert.sql` | ADD | — | — | — | `SQL statement` |
| `rustd/crates/daemon/afd_fleet/src/lease/turn_checkpoint/sql/reader_capacity.sql` | ADD | — | — | — | `SQL statement` |
| `rustd/crates/daemon/afd_fleet/src/lease/turn_checkpoint/sql/reader_pin.sql` | ADD | — | — | — | `SQL statement` |
| `rustd/crates/daemon/afd_fleet/src/lease/turn_checkpoint/sql/ready_count.sql` | ADD | — | — | — | `SQL statement` |
| `rustd/crates/daemon/afd_fleet/src/lease/turn_checkpoint/sql/refs_insert.sql` | ADD | — | — | — | `SQL statement` |
| `rustd/crates/daemon/afd_fleet/src/lease/turn_checkpoint/sql/retire_ready.sql` | ADD | — | — | — | `SQL statement` |
| `rustd/crates/daemon/afd_fleet/src/lease/turn_checkpoint/sql/root.sql` | ADD | — | — | — | `SQL statement` |
| `rustd/crates/daemon/afd_fleet/src/lease/turn_checkpoint/sql/scope.sql` | ADD | — | — | — | `SQL statement` |
| `rustd/crates/daemon/afd_fleet/src/lease/turn_checkpoint/sql/validate_folds.sql` | ADD | — | — | — | `SQL statement` |
| `rustd/crates/daemon/afd_fleet/src/lease/turn_checkpoint/sql/writer_pin.sql` | ADD | — | — | — | `SQL statement` |
| `rustd/crates/daemon/afd_fleet/src/lease/turn_checkpoint/store.rs` | ADD | `+ TurnCheckpoints::execute`; `+ TurnCheckpoints::files`; `+ TurnCheckpoints::fmt`; `+ TurnCheckpoints::new`; `+ TurnCheckpoints::perform`; `+ TurnCheckpoints::with_storage` | — | `+ TurnCheckpoints` | — |
| `rustd/crates/daemon/afd_fleet/src/lease/turn_checkpoint/transaction.rs` | ADD | `+ acquire`; `+ discard`; `+ recycle` | — | `+ Connection` | — |
| `rustd/crates/daemon/afd_fleet/src/lease/turn_checkpoint/transfer.rs` | ADD | `+ confirm`; `+ execute`; `+ freeze` | — | `+ Descriptor` | — |
| `rustd/crates/daemon/afd_fleet/src/task/checkpoint/identity.rs` | EDIT | `~ Identity::content`; `+ create`; `~ load`; `~ quota` | — | `~ Identity` | — |
| `rustd/crates/daemon/afd_fleet/src/task/checkpoint/maintenance/identities.rs` | ADD | `+ remove`; `+ retain` | — | `+ Identity` | `+ BATCH` |
| `rustd/crates/daemon/afd_fleet/src/task/checkpoint/maintenance/mod.rs` | EDIT | `+ erase_orphans`; `~ maintain`; `+ orphans` | — | — | — |
| `rustd/crates/daemon/afd_fleet/src/task/checkpoint/mod.rs` | EDIT | — | — | — | `module/test wiring` |
| `rustd/crates/daemon/afd_fleet/src/task/checkpoint/object_row.rs` | EDIT | `+ Object::belongs`; `~ Object::open`; `+ Object::open_scoped`; `~ Object::own`; `~ Object::wire`; `+ Object::wire_scoped` | — | `~ Object` | — |
| `rustd/crates/daemon/afd_fleet/src/task/checkpoint/objects.rs` | EDIT | `+ allocate`; `+ matching`; `- pg_bytes`; `~ prepare`; `~ register`; `+ registered` | — | `- NewObject` | — |
| `rustd/crates/daemon/afd_fleet/src/task/checkpoint/sql/identity_delete.sql` | ADD | — | — | — | `SQL statement` |
| `rustd/crates/daemon/afd_fleet/src/task/checkpoint/sql/identity_retention.sql` | ADD | — | — | — | `SQL statement` |
| `rustd/crates/daemon/afd_fleet/src/task/checkpoint/sql/orphan_tombstone.sql` | EDIT | — | — | — | `SQL statement` |
| `rustd/crates/daemon/afd_fleet/src/task/checkpoint/sql/orphans.sql` | EDIT | — | — | — | `SQL statement` |
| `rustd/crates/daemon/afd_fleet/src/task/checkpoint/transfer/mod.rs` | EDIT | `- issue` | — | — | — |
| `rustd/crates/daemon/afd_fleet/src/task/checkpoint/transfer/signing.rs` | ADD | `+ Signing::object`; `+ Signing::transfer`; `+ issue` | — | `+ Signing` | — |
| `rustd/crates/daemon/afd_fleet/src/task/checkpoint/transfer/verify.rs` | EDIT | `+ open`; `~ remote`; `+ remote_scope` | — | `~ Receipt` | — |
| `rustd/crates/daemon/afd_fleet/src/task/mod.rs` | EDIT | — | — | — | `module/test wiring` |
| `rustd/crates/daemon/afd_fleet/src/task/storage/mod.rs` | EDIT | `~ R2CheckpointStore::bucket_reference`; `~ R2CheckpointStore::session` | — | `~ R2CheckpointStore` | — |
| `rustd/crates/daemon/afd_fleet/src/task/storage/registration.rs` | ADD | `+ pg_bytes`; `+ prepare` | — | `+ NewObject`; `+ Registration` | — |
| `rustd/crates/daemon/afd_fleet/src/task/storage/transfer.rs` | EDIT | `~ StorageSession::delete`; `~ StorageSession::download`; `~ StorageSession::read_object`; `~ StorageSession::upload` | — | `~ Authorization`; `~ StorageSession` | — |
| `rustd/crates/daemon/afd_fleet/src/task/types.rs` | EDIT | — | — | `~ CheckpointMaintenance`; `~ CheckpointMaintenanceCursor` | — |
| `rustd/crates/daemon/afd_fleet/tests/fleet_suite.rs` | EDIT | — | — | — | `module/test wiring` |
| `rustd/crates/daemon/afd_fleet/tests/integration_held_sandbox/report.rs` | EDIT | `~ test_a_report_carrying_held_until_records_the_hold`; `+ test_expired_late_report_settles_without_a_new_hold` | — | — | — |
| `rustd/crates/daemon/afd_fleet/tests/integration_lease_folds/limits.rs` | EDIT | `+ a_retained_task_generation_stops_the_ordinary_fold_prefix` | — | — | — |
| `rustd/crates/daemon/afd_fleet/tests/integration_lease_issue_capacity.rs` | ADD | `+ Case::issue`; `+ Case::new`; `+ Case::stamp`; `+ a_failed_task_source_attach_restores_the_exact_idle_hold_and_issue_tallies`; `+ extra_fleet`; `+ required`; `+ the_fleet_issuer_refuses_a_third_worker_and_transfers_its_own_hold_once` | — | `+ Case`; `+ Checked`; `+ Stamp` | — |
| `rustd/crates/daemon/afd_fleet/tests/integration_tasks/decisions/checkpoint_server.rs` | EDIT | `~ Server::finish`; `~ Server::start` | — | `~ Server` | `~ HOST` |
| `rustd/crates/daemon/afd_fleet/tests/integration_tasks/decisions/mod.rs` | EDIT | — | — | — | `module/test wiring` |
| `rustd/crates/daemon/afd_fleet/tests/integration_tasks/mod.rs` | EDIT | — | — | — | `module/test wiring` |
| `rustd/crates/daemon/afd_fleet/tests/integration_turn_checkpoints/expiry.rs` | ADD | `+ commit_rechecks_pin_and_plan_deadlines_after_manifest_locks`; `+ expired_commit`; `+ release_after_expiry`; `+ shorten` | — | — | — |
| `rustd/crates/daemon/afd_fleet/tests/integration_turn_checkpoints/files.rs` | ADD | `+ authenticated_chunk`; `+ begin`; `+ corrupt_uploaded_ciphertext_never_becomes_verified_or_a_recovery_head`; `+ download`; `+ ordinary_files_and_root_publish_under_real_private_sdk_custody`; `+ publish`; `+ read`; `+ release`; `+ unverified`; `+ upload_bytes` | — | — | `+ CONTENT` |
| `rustd/crates/daemon/afd_fleet/tests/integration_turn_checkpoints/folded.rs` | ADD | `+ claim`; `+ empty_manifest`; `+ file_recovery_adopts_the_saved_nonempty_prefix_under_the_replacement_fence`; `+ history`; `+ incorporate`; `+ publish`; `+ read`; `+ reader_expiry_during_a_fold_lock_wait_rolls_back_every_adopted_claim` | — | `+ Claim` | `+ MESSAGE` |
| `rustd/crates/daemon/afd_fleet/tests/integration_turn_checkpoints/garbage.rs` | ADD | `+ age_object`; `+ assert_storage`; `+ orphan`; `+ race`; `+ release_after_both_waits`; `+ tasks`; `+ turn_references_survive_an_orphan_snapshot_and_release_shared_quota_only_after_retention`; `+ waiting` | — | — | — |
| `rustd/crates/daemon/afd_fleet/tests/integration_turn_checkpoints/identity_retention.rs` | ADD | `+ event_identity_retention_is_bounded_fair_and_waits_for_object_and_purge_custody`; `+ finish_purge_after_asserting_retention`; `+ insert`; `+ namespace`; `+ purge`; `+ repository_keys`; `+ seed`; `+ used_identity` | — | `+ Keys` | — |
| `rustd/crates/daemon/afd_fleet/tests/integration_turn_checkpoints/lifecycle.rs` | ADD | `+ both_workspace_modes_bound_writer_custody_and_preserve_retired_allocation_identity`; `+ commit_read_and_adopt_refuse_foreign_fences_and_expired_leases`; `+ end_reader`; `+ expire_pending`; `+ foreign`; `+ generation`; `+ isolated`; `+ modes`; `+ publish`; `+ reader_retention`; `+ ready_retention_preserves_old_readers_until_release_or_expiry`; `+ refused`; `+ renew` | — | — | — |
| `rustd/crates/daemon/afd_fleet/tests/integration_turn_checkpoints/manifest.rs` | ADD | `+ Pending::allocate`; `+ Pending::commit`; `+ Pending::new`; `+ Pending::write`; `+ a_page_cannot_change_its_preallocated_ordinal`; `+ contained_link_chains_remain_publishable`; `+ cyclic_and_escaping_link_graphs_never_replace_the_ready_head`; `+ incompatible`; `+ link`; `+ no_head` | — | `+ Pending` | — |
| `rustd/crates/daemon/afd_fleet/tests/integration_turn_checkpoints/mod.rs` | ADD | `+ Case::begin`; `+ Case::command`; `+ Case::counts`; `+ Case::lease`; `+ Case::new`; `+ Case::tasks`; `+ Case::with_fixtures`; `+ a_changed_begin_or_a_retired_plan_cannot_reallocate_its_operation`; `+ a_foreign_runner_or_wrong_fence_allocates_no_checkpoint`; `+ a_lost_begin_acknowledgement_reuses_the_exact_generation_and_pin`; `+ a_parked_or_settled_source_cannot_begin_recovery`; `+ a_revoked_drained_or_degraded_host_cannot_allocate_a_checkpoint`; `+ required`; `+ valid` | — | `+ Case`; `+ Checked` | — |
| `rustd/crates/daemon/afd_fleet/tests/integration_turn_checkpoints/publication.rs` | ADD | `+ api_only_publication_is_sealed_atomic_replayable_and_readable_without_file_storage`; `+ assert_sealed`; `+ command`; `+ publish`; `+ read_and_adopt`; `+ release_and_refuse_replayed_read`; `+ root` | — | — | — |
| `rustd/crates/daemon/afd_fleet/tests/integration_turn_checkpoints/replacement.rs` | ADD | `+ Case::replace` | — | — | — |
| `rustd/crates/daemon/afd_fleet/tests/integration_turn_checkpoints/storage.rs` | ADD | `+ Files::execute`; `+ Files::finish`; `+ Files::for_case`; `+ Files::new`; `+ encrypt`; `+ upload` | — | `+ Files` | — |
| `rustd/crates/daemon/afd_fleet/tests/lease_renewal/holders.rs` | EDIT | `~ assert_host_full` | — | — | — |
| `rustd/crates/daemon/afd_fleet/tests/support/fleet_report_seed/plane.rs` | EDIT | `~ Fixtures::plane` | — | — | — |
| `rustd/crates/daemon/afd_gateway/src/operation.rs` | EDIT | `~ Operation::as_str`; `~ Operation::fmt` | — | `~ Operation` | — |
| `rustd/crates/daemon/afd_gateway/src/registry/mod.rs` | EDIT | `~ Registry::register_current` | — | — | — |
| `rustd/crates/daemon/afd_gateway/src/session/context.rs` | EDIT | `~ SessionServices::send` | — | — | — |
| `rustd/crates/daemon/afd_gateway/src/session/dispatch/mod.rs` | EDIT | `+ SessionServices::availability`; `~ SessionServices::dispatch`; `~ SessionServices::handle`; `~ operation` | — | — | — |
| `rustd/crates/daemon/afd_gateway/src/session/dispatch/report.rs` | EDIT | `~ SessionServices::report` | — | — | — |
| `rustd/crates/daemon/afd_gateway/src/session/dispatch/turn_checkpoint.rs` | ADD | `+ SessionServices::turn_checkpoint` | — | — | — |
| `rustd/crates/daemon/afd_runner/src/capacity/mod.rs` | EDIT | `~ Capacity::can_claim`; `+ Held::can_continue`; `~ lock`; `~ lock_host` | — | `~ Capacity`; `~ Census`; `+ Claimant`; `~ Held`; `~ Host` | — |
| `rustd/crates/daemon/afd_runner/src/capacity/sql/census.sql` | EDIT | — | — | — | `SQL statement` |
| `rustd/crates/daemon/afd_runner/src/capacity/sql/lock_host.sql` | EDIT | — | — | — | `SQL statement` |
| `rustd/crates/daemon/afd_runner/src/capacity/sql/mod.rs` | EDIT | — | — | — | `module/test wiring` |
| `rustd/crates/daemon/afd_runner/src/cordon.rs` | ADD | — | — | — | `+ CORDONED`; `+ LIFTED`; `+ REASON` |
| `rustd/crates/daemon/afd_runner/src/heartbeat/cordon.rs` | ADD | `+ Runners::bump_liveness`; `+ write_failed` | — | `+ Health` | — |
| `rustd/crates/daemon/afd_runner/src/heartbeat/mod.rs` | EDIT | `- Runners::bump_liveness`; `~ Runners::heartbeat`; `~ Runners::heartbeat_connected`; `~ Runners::record_beat` | — | — | — |
| `rustd/crates/daemon/afd_runner/src/heartbeat/policy.rs` | EDIT | `~ PolicyRow::from`; `~ read` | — | `~ PolicyColumns`; `~ PolicyRow` | — |
| `rustd/crates/daemon/afd_runner/src/lib.rs` | EDIT | — | — | — | `module/test wiring` |
| `rustd/crates/daemon/afd_runner/src/sql/runner/heartbeat_with_transition_event.sql` | EDIT | — | — | — | `SQL statement` |
| `rustd/crates/daemon/afd_runner/src/sql/runner/select_runner_assigned_policy.sql` | EDIT | — | — | — | `SQL statement` |
| `rustd/crates/daemon/afd_runner/src/sql/runner/touch_runner_last_seen.sql` | EDIT | — | — | — | `SQL statement` |
| `rustd/crates/daemon/afd_runner/src/sql/runner_view/list_runners_after.sql` | EDIT | — | — | — | `SQL statement` |
| `rustd/crates/daemon/afd_runner/src/sql/runner_view/list_runners_first.sql` | EDIT | — | — | — | `SQL statement` |
| `rustd/crates/daemon/afd_runner/src/sql/runner_view/runner_detail.sql` | EDIT | — | — | — | `SQL statement` |
| `rustd/crates/daemon/afd_runner/src/sql/sweep/cordon_silent_runner.sql` | ADD | — | — | — | `SQL statement` |
| `rustd/crates/daemon/afd_runner/src/sql/sweep/expire_active_lease_slots.sql` | DELETE | — | — | — | `SQL statement` |
| `rustd/crates/daemon/afd_runner/src/sql/sweep/insert_offline_event.sql` | DELETE | — | — | — | `SQL statement` |
| `rustd/crates/daemon/afd_runner/src/sql/sweep/mark_drained_if_idle.sql` | EDIT | — | — | — | `SQL statement` |
| `rustd/crates/daemon/afd_runner/src/sql/sweep/mod.rs` | EDIT | — | — | — | `+ CORDON_SILENT_RUNNER`; `- EXPIRE_ACTIVE_LEASE_SLOTS`; `- INSERT_OFFLINE_EVENT` |
| `rustd/crates/daemon/afd_runner/src/sql/sweep/select_due_runners.sql` | EDIT | — | — | — | `SQL statement` |
| `rustd/crates/daemon/afd_runner/src/sweep/liveness/mod.rs` | EDIT | `~ Liveness::record_offline`; `- Liveness::release_slots`; `~ Liveness::visit`; `- expire_slots` | — | — | `- CONTEXT_SLOTS`; `- EXPIRE_PAST` |
| `rustd/crates/daemon/afd_runner/src/view/decode.rs` | EDIT | `~ ItemRow::parse` | — | `~ ItemRow` | — |
| `rustd/crates/daemon/afd_runner/src/view/mod.rs` | EDIT | `+ RunnerItem::cordon` | — | `~ RunnerItem` | — |
| `rustd/crates/daemon/afd_runner/tests/capacity/choice.rs` | EDIT | `~ place_authoring`; `~ test_failed_placement_does_not_advance_preference`; `~ test_kind_preference_survives_a_new_store_handle` | — | — | — |
| `rustd/crates/daemon/afd_runner/tests/capacity/fixture.rs` | EDIT | `+ Fixture::capacity`; `+ Fixture::hold`; `~ Fixture::try_claim` | — | — | — |
| `rustd/crates/daemon/afd_runner/tests/capacity/holds.rs` | ADD | `+ test_a_lease_and_a_hold_refuse_a_third_fleet`; `+ test_a_renewal_counts_no_holds`; `+ test_an_expired_hold_does_not_reserve_a_worker`; `+ test_concurrent_claims_leave_room_for_a_live_hold`; `+ test_the_claimants_own_hold_takes_no_second_worker` | — | — | — |
| `rustd/crates/daemon/afd_runner/tests/capacity/mod.rs` | EDIT | — | — | — | `module/test wiring` |
| `rustd/crates/daemon/afd_runner/tests/integration_runner_plane/connections/mod.rs` | EDIT | `~ a_connection_proof_never_authorizes_another_host_and_debug_omits_its_digest`; `~ can_claim` | — | — | — |
| `rustd/crates/daemon/afd_runner/tests/integration_runner_plane/cordon.rs` | ADD | `+ beat`; `+ connected`; `+ missed_cadence_resets_an_old_healthy_proof`; `+ prove_elapsed`; `+ quarantined`; `+ silence`; `+ test_a_refused_cordon_leaves_no_offline_record`; `+ test_a_silent_host_is_cordoned_with_its_offline_record`; `+ test_automatic_lift_requires_continuous_valid_beats`; `+ unproved_liveness_resets_health_and_never_lifts` | — | — | — |
| `rustd/crates/daemon/afd_runner/tests/integration_runner_plane/mod.rs` | EDIT | — | — | — | `module/test wiring` |
| `rustd/crates/daemon/afd_runner/tests/integration_sweeps/mod.rs` | EDIT | `+ converge`; `~ liveness_and_retention_converge_real_runner_rows` | — | — | — |
| `rustd/crates/daemon/main/src/plane/mod.rs` | EDIT | `~ lease_plane`; `~ task_plane` | — | — | — |
| `rustd/crates/daemon/main/src/sweepers/checkpoints.rs` | EDIT | `~ spawn` | — | — | — |
| `rustd/crates/daemon/main/tests/daemon_suite.rs` | EDIT | — | — | — | `module/test wiring` |
| `rustd/crates/daemon/main/tests/recovery_process/mod.rs` | ADD | `+ killed_runner_mid_turn_restarts_on_another_host` | — | `+ Checked` | `+ ACCEPTED`; `+ COMPLETED`; `+ LATER_INPUT`; `+ SAVED_INPUT` |
| `rustd/crates/daemon/main/tests/recovery_process/model.rs` | ADD | `+ Asked::answer`; `+ Model::base`; `+ Model::finish`; `+ Model::next`; `+ Model::root`; `+ Model::start`; `+ answer` | — | `+ Asked`; `+ Model` | `+ HOST` |
| `rustd/crates/daemon/main/tests/recovery_process/process.rs` | ADD | `+ Worker::kill`; `+ Worker::next`; `+ Worker::start`; `+ Worker::stop` | — | `+ Worker` | — |
| `rustd/crates/daemon/main/tests/recovery_process/proof.rs` | ADD | `+ adopted`; `+ checkpoint`; `+ deadline`; `+ no_early_issue`; `+ prefix`; `+ quarantined`; `+ replacement`; `+ settled` | — | `+ Winner` | — |
| `rustd/crates/daemon/main/tests/recovery_process/seed.rs` | ADD | `+ configure`; `+ fold`; `+ runners`; `+ select` | — | — | — |
| `rustd/crates/foundation/af_core/src/error_code/fleet.rs` | EDIT | — | — | — | `+ RUN_CHECKPOINT_CONFLICT`; `+ RUN_CHECKPOINT_INCOMPATIBLE`; `+ RUN_CHECKPOINT_LIMIT`; `+ RUN_CHECKPOINT_UNAVAILABLE` |
| `rustd/crates/foundation/af_core/src/error_code/mod.rs` | EDIT | — | — | — | `~ REGISTRY` |
| `rustd/crates/foundation/af_core/src/problem/fleet.rs` | EDIT | — | — | — | `~ FLEET` |
| `rustd/crates/foundation/af_core/src/timing.rs` | EDIT | — | — | — | `+ UNCORDON_AFTER`; `~ _` |
| `rustd/crates/foundation/af_core/tests/error_code.rs` | EDIT | `~ assert_every_named_code_is_registered` | — | — | — |
| `rustd/crates/foundation/af_crypto/src/aad.rs` | EDIT | `+ Aad::turn_recovery` | — | — | — |
| `rustd/crates/foundation/af_crypto/tests/crypto_suite.rs` | EDIT | — | — | — | `module/test wiring` |
| `rustd/crates/foundation/af_crypto/tests/turn_checkpoint.rs` | ADD | `+ turn_history_authenticates_every_scope_and_version_dimension` | — | — | — |
| `rustd/crates/foundation/af_validate/src/checkpoint_links.rs` | ADD | `+ checkpoint_links`; `+ resolve` | — | — | — |
| `rustd/crates/foundation/af_validate/src/lib.rs` | EDIT | — | — | — | `module/test wiring` |
| `rustd/crates/harness/af_providers/src/testing/tls.rs` | EDIT | `+ client_from_file` | — | — | — |
| `rustd/crates/harness/guffbolt/Cargo.toml` | EDIT | — | — | — | `documentation/build/version metadata` |
| `rustd/crates/harness/guffbolt/README.md` | EDIT | — | — | — | `documentation/build/version metadata` |
| `rustd/crates/harness/guffbolt/src/drive/mod.rs` | EDIT | `~ answered`; `~ drive`; `~ turns` | — | — | — |
| `rustd/crates/harness/guffbolt/src/drive/state.rs` | ADD | `+ DriveState::answers`; `+ DriveState::completed`; `+ DriveState::completed_turns`; `+ DriveState::map_answers` | — | `+ DriveState` | — |
| `rustd/crates/harness/guffbolt/src/drive/tests/extension.rs` | EDIT | `~ Recording::before_model`; `~ a_budget_that_runs_out_mid_call_stops_the_loop_and_says_so`; `~ a_continued_answer_stays_in_the_conversation_and_the_model_is_told_why`; `~ a_loop_that_ends_within_its_budget_ends_as_it_would_unbounded`; `~ a_spent_budget_stops_the_loop_before_any_call`; `~ an_extension_can_end_the_loop_after_a_group`; `~ an_extension_can_end_the_loop_in_place_of_an_answer`; `~ an_extension_can_refuse_a_turn_before_its_calls_run`; `~ an_extension_hears_every_point_in_order`; `~ an_extension_that_ends_before_the_model_makes_no_call` | — | — | — |
| `rustd/crates/harness/guffbolt/src/drive/tests/mod.rs` | EDIT | `~ a_blank_answer_after_a_correction_adds_nothing_to_the_reply`; `~ a_blank_answer_after_work_is_an_answer`; `~ a_blank_first_turn_is_a_failure_not_an_answer`; `~ a_correction_sent_while_an_answer_after_tools_is_written_comes_after_that_answer`; `~ a_correction_sent_while_an_answer_is_written_comes_after_that_answer`; `~ a_correction_sent_while_the_answer_is_written_runs_one_more_turn`; `~ a_message_sent_mid_stream_is_read_by_the_next_call_and_never_cuts_this_one`; `~ a_model_that_fails_ends_the_run_failed_and_leaves_late_input_unread`; `~ a_run_with_no_input_answers_after_its_one_turn`; `~ a_stop_during_the_model_call_ends_stopped`; `~ a_stop_during_the_tools_keeps_their_results_so_far_and_ends_stopped`; `~ a_window_that_cannot_be_fitted_fails_the_run_before_any_call`; `~ messages_sent_while_tools_run_are_read_by_the_next_call`; `~ no_answer_ignores_input_that_arrived_before_it` | — | — | — |
| `rustd/crates/harness/guffbolt/src/drive/tests/recovery.rs` | ADD | `+ Boundary::before_model`; `+ recovered_driver_preserves_accepted_answers_and_first_attempt_reconciliation` | — | `+ Boundary` | — |
| `rustd/crates/harness/guffbolt/src/extension.rs` | EDIT | `~ Extension::before_model` | `~ Extension` | — | — |
| `rustd/crates/harness/guffbolt/src/lib.rs` | EDIT | — | — | — | `module/test wiring` |
| `rustd/crates/protocol/af_observability/src/metrics/declared/fleet.rs` | EDIT | — | — | — | `+ RUNNER_CORDON_CHANGES_TOTAL` |
| `rustd/crates/protocol/af_observability/src/metrics/label/fleet.rs` | EDIT | — | — | — | `module/test wiring` |
| `rustd/crates/protocol/af_observability/src/metrics/label/tests.rs` | EDIT | `~ fleet_products` | — | — | — |
| `rustd/crates/protocol/af_observability/src/producers/fleet/mod.rs` | EDIT | `~ Handles::claim` | — | `~ Handles` | — |
| `rustd/crates/protocol/af_observability/src/producers/fleet/runner/mod.rs` | EDIT | `+ cordon_changed` | — | — | — |
| `rustd/crates/protocol/af_observability/src/semconv/mod.rs` | EDIT | — | — | — | `~ CENSUS_LABEL_KEYS`; `+ LABEL_CHANGE` |
| `rustd/crates/protocol/af_observability/tests/producers_drive.rs` | EDIT | `~ test_every_runner_producer_runs_its_body` | — | — | — |
| `rustd/crates/protocol/af_wire/src/lease/mod.rs` | EDIT | — | — | — | `module/test wiring` |
| `rustd/crates/protocol/af_wire/src/lease/payload.rs` | EDIT | — | — | `~ LeasePayload` | — |
| `rustd/crates/protocol/af_wire/src/lease/turn_checkpoint/command.rs` | ADD | `+ TurnCheckpointAction::kind`; `+ TurnPrivateRoot::fmt`; `+ page_bound` | — | `+ TurnCheckpointAction`; `+ TurnCheckpointBegin`; `+ TurnCheckpointCommand`; `+ TurnCheckpointRef`; `+ TurnPrivateRoot` | — |
| `rustd/crates/protocol/af_wire/src/lease/turn_checkpoint/mod.rs` | ADD | — | — | `+ FoldRef`; `+ TurnCheckpointKind`; `+ TurnId`; `+ TurnMessageSize`; `+ TurnResume`; `+ TurnScope`; `+ TurnWorkspace` | `+ TURN_CHECKPOINT_BYTES`; `+ TURN_CHECKPOINT_FORMAT`; `+ TURN_CHECKPOINT_PAGE_BYTES` |
| `rustd/crates/protocol/af_wire/src/lease/turn_checkpoint/response.rs` | ADD | `+ TurnCheckpointResponse::kind` | — | `+ TurnCheckpointPlan`; `+ TurnCheckpointRead`; `+ TurnCheckpointResponse` | — |
| `rustd/crates/protocol/af_wire/src/protocol/down.rs` | EDIT | `~ Down::request_id`; `~ Down::stream_kind` | — | `~ Down` | — |
| `rustd/crates/protocol/af_wire/src/protocol/mod.rs` | EDIT | — | — | — | `~ VERSION` |
| `rustd/crates/protocol/af_wire/src/protocol/up/mod.rs` | EDIT | `~ Up::request_id`; `~ Up::stream_kind` | — | `~ Up` | — |
| `rustd/crates/protocol/af_wire/src/protocol/up/other.rs` | EDIT | `~ Up::from` | — | `~ Other` | — |
| `rustd/crates/protocol/af_wire/src/report/mod.rs` | EDIT | — | — | `+ AcceptedHold`; `~ ReportResponse` | — |
| `rustd/crates/protocol/af_wire/src/task/checkpoint/objects.rs` | EDIT | — | — | `~ CheckpointObject` | — |
| `rustd/crates/protocol/af_wire/src/task/checkpoint/plan.rs` | EDIT | — | — | `~ CheckpointPin` | — |
| `rustd/crates/protocol/af_wire/tests/turn_checkpoint.rs` | ADD | `+ a_building_workspace_has_no_serializable_recovery_variant`; `+ begin`; `+ capture_authority_cannot_name_another_event_or_bucket`; `+ checkpoint_debug_output_does_not_quote_root_history`; `+ native_turn_commands_use_reports_and_count_escaped_envelope_bytes`; `+ ordinary_state_is_independently_versioned_and_fenced`; `+ paths`; `+ required` | — | `+ Checked` | `+ ID` |
| `rustd/crates/protocol/af_wire/tests/wire_suite.rs` | EDIT | — | — | — | `module/test wiring` |
| `rustd/crates/runner/afr_agent/src/engine/boundary.rs` | ADD | `+ CommittedRecovery::digest`; `+ CommittedRecovery::generation`; `+ CommittedRecovery::id`; `+ RecoveryBoundarySink::save`; `+ RecoveryBoundarySink::workspace`; `+ RecoveryReceipt::matches`; `+ RecoverySnapshot::witness` | `+ RecoveryBoundarySink` | `+ CommittedRecovery`; `+ RecoveryReceipt`; `+ RecoverySave`; `+ RecoverySkip`; `+ RecoverySnapshot`; `+ RecoveryState`; `+ RecoveryWitness` | — |
| `rustd/crates/runner/afr_agent/src/engine/boundary_tests.rs` | ADD | `+ committed_receipt_requires_new_identity_generation_and_exact_history`; `+ committed_receipt_requires_the_exact_attempt`; `+ fixture` | — | — | — |
| `rustd/crates/runner/afr_agent/src/engine/meter.rs` | ADD | `+ Meter::add`; `+ Meter::backlog`; `+ Meter::default`; `+ Meter::lines`; `+ Meter::new`; `+ Meter::read`; `+ Meter::recorded_through`; `+ Meter::unrecorded`; `+ Totals::add`; `+ Totals::read`; `+ narrow` | — | `+ Lines`; `+ Meter`; `+ Totals` | — |
| `rustd/crates/runner/afr_agent/src/engine/mod.rs` | EDIT | `- Meter::add`; `- Meter::backlog`; `- Meter::default`; `- Meter::lines`; `- Meter::new`; `- Meter::read`; `- Meter::recorded_through`; `- Meter::unrecorded`; `- Totals::add`; `- Totals::read`; `- narrow` | — | `~ AgentRun`; `~ Folded`; `- Lines`; `- Meter`; `- Totals` | — |
| `rustd/crates/runner/afr_agent/src/engine/recovery.rs` | EDIT | `~ TaskRecovery::capture`; `~ TaskRecovery::restore` | — | `~ History` | — |
| `rustd/crates/runner/afr_agent/src/engine/recovery_tests.rs` | EDIT | `~ adopted_goal_restores_prior_work_as_scrubbed_data_but_cannot_change_task_identity`; `~ incomplete_or_unknown_history_never_becomes_an_empty_successful_resume`; `+ prior_work`; `+ rich_history_retains_driver_answers_and_pinned_input_index`; `~ task` | — | — | — |
| `rustd/crates/runner/afr_agent/src/engine/root_recovery.rs` | ADD | `+ RootRecovery::check`; `+ RootRecovery::scrub` | — | `+ RootRecovery` | `+ FOLD_REFS_MAX` |
| `rustd/crates/runner/afr_agent/src/engine/tests.rs` | EDIT | `~ a_run_debugs_without_the_leases_secrets` | — | — | — |
| `rustd/crates/runner/afr_agent/src/engine/turn_recovery.rs` | ADD | `+ TurnRecovery::fmt`; `+ TurnRecovery::restore`; `+ TurnRecovery::validate_for`; `+ TurnSnapshot::capture`; `+ TurnSnapshot::fmt`; `+ scope` | — | `+ Encoded`; `+ History`; `+ TurnRecovery`; `+ TurnSnapshot` | — |
| `rustd/crates/runner/afr_agent/src/engine/turn_recovery_tests.rs` | ADD | `+ capture_requires_plain_authority_a_frozen_setup_and_a_bounded_complete_root`; `+ changed_setup_scope_workspace_digest_or_adopted_membership_refuses`; `+ fixture`; `+ has_replay`; `+ id`; `+ lease`; `+ new_model_resolution_keeps_work_but_discards_provider_replay`; `+ ordinary_state_restores_answers_input_membership_and_current_secret_scrubbing`; `+ root`; `+ unsupported_format_refuses_even_when_its_bytes_match_the_descriptor` | — | — | — |
| `rustd/crates/runner/afr_agent/src/error/mod.rs` | EDIT | `~ Error::code`; `~ Error::unhosted` | — | `~ ErrorKind` | — |
| `rustd/crates/runner/afr_agent/src/error/raise.rs` | EDIT | `+ recovery_invalid`; `+ recovery_source` | — | — | — |
| `rustd/crates/runner/afr_agent/src/lib.rs` | EDIT | — | — | — | `module/test wiring` |
| `rustd/crates/runner/afr_agent/src/loop/budget_tests.rs` | EDIT | `~ test_meter_holds_what_the_report_sums` | — | — | — |
| `rustd/crates/runner/afr_agent/src/loop/children.rs` | EDIT | `~ Children::beside`; `~ Children::end_all`; `~ Children::new` | — | `~ Children` | — |
| `rustd/crates/runner/afr_agent/src/loop/compaction_tests.rs` | EDIT | `+ compaction_lease`; `~ run` | — | — | — |
| `rustd/crates/runner/afr_agent/src/loop/construction.rs` | ADD | `+ Harness::child`; `+ Harness::root`; `+ opening` | — | — | — |
| `rustd/crates/runner/afr_agent/src/loop/conversation.rs` | EDIT | `~ Harness::claim_folded` | — | — | — |
| `rustd/crates/runner/afr_agent/src/loop/finish.rs` | EDIT | `+ Harness::final_recovery`; `~ Harness::finish` | — | — | — |
| `rustd/crates/runner/afr_agent/src/loop/fold_tests/fixture.rs` | EDIT | `~ Arriving::on_claim`; `~ Interrupted::claim`; `~ run_with` | — | — | — |
| `rustd/crates/runner/afr_agent/src/loop/fold_tests/mod.rs` | EDIT | `~ a_message_the_inbox_replays_is_joined_once` | — | — | — |
| `rustd/crates/runner/afr_agent/src/loop/memory_round_trip_tests.rs` | ADD | `+ assert_round_trip`; `+ memory_script`; `+ read_back`; `+ test_memory_tools_round_trip_through_push` | — | `+ Checked` | — |
| `rustd/crates/runner/afr_agent/src/loop/memory_tests.rs` | EDIT | `~ a_cadence_with_nothing_stored_pushes_nothing`; `~ a_checkpoint_that_never_answers_does_not_outlive_the_lease`; `- read_back`; `~ store_then_answer`; `- test_memory_tools_round_trip_through_push`; `~ the_memory_is_checkpointed_every_n_calls` | — | — | — |
| `rustd/crates/runner/afr_agent/src/loop/mod.rs` | EDIT | `- Harness::child`; `~ Harness::drive`; `- Harness::root`; `~ Loop::run` | — | `~ Harness` | — |
| `rustd/crates/runner/afr_agent/src/loop/provider_failure_tests.rs` | EDIT | `~ a_provider_that_cannot_be_reached_is_admitted_then_an_engine_error` | — | — | — |
| `rustd/crates/runner/afr_agent/src/loop/recovery_boundary.rs` | ADD | `+ Harness::accept_snapshot`; `+ Harness::capture_task_root`; `+ Harness::root_snapshot`; `+ Harness::save_boundary`; `+ Harness::snapshot_state`; `+ Root::before_model`; `+ committed`; `+ prepare_root`; `+ restored`; `+ skipped` | — | `+ Root`; `+ RootSetup` | — |
| `rustd/crates/runner/afr_agent/src/loop/session_tests.rs` | EDIT | `~ drive_in` | — | — | — |
| `rustd/crates/runner/afr_agent/src/loop/shared.rs` | EDIT | `~ Shared::new` | — | `~ Shared` | — |
| `rustd/crates/runner/afr_agent/src/loop/task.rs` | EDIT | `~ Task::before_model` | — | — | — |
| `rustd/crates/runner/afr_agent/src/loop/task_tests/behavior/publication_fixture.rs` | EDIT | `~ publish` | — | — | — |
| `rustd/crates/runner/afr_agent/src/loop/task_tests/behavior/publication_guards.rs` | EDIT | `~ execute` | — | — | — |
| `rustd/crates/runner/afr_agent/src/loop/task_tests/behavior/recovery_fixture.rs` | EDIT | `~ execute` | — | — | — |
| `rustd/crates/runner/afr_agent/src/loop/task_tests/mod.rs` | EDIT | `~ run` | — | — | — |
| `rustd/crates/runner/afr_agent/src/loop/tests.rs` | EDIT | `~ a_dropped_run_still_closes_its_open_call_once`; `~ drive` | — | — | — |
| `rustd/crates/runner/afr_agent/src/loop/turn_tests.rs` | EDIT | `~ a_credential_minted_during_a_run_is_logged_under_the_runs_lease`; `+ assert_minted_under_lease` | — | — | — |
| `rustd/crates/runner/afr_agent/src/nested/mod.rs` | EDIT | — | — | — | `module/test wiring` |
| `rustd/crates/runner/afr_agent/src/nested/recovery.rs` | ADD | `+ CapturePermit::drop`; `+ ChildRecovery::check`; `+ Registry::capture_permit`; `+ Registry::restore`; `+ Registry::snapshot`; `+ Registry::try_capture` | — | `+ CapturePermit`; `+ ChildRecovery`; `+ SavedChild` | — |
| `rustd/crates/runner/afr_agent/src/nested/recovery_tests.rs` | ADD | `+ a_capture_gate_refuses_child_starts_and_releases_on_drop`; `+ live_children_inconsistent_counters_and_foreign_permits_refuse_recovery`; `+ terminal_children_restore_without_starting_a_future_or_reopening_input` | — | — | — |
| `rustd/crates/runner/afr_agent/src/nested/registry.rs` | EDIT | `~ Registry::new`; `~ Registry::start` | — | `~ Registry`; `+ Requests`; `~ Status` | — |
| `rustd/crates/runner/afr_agent/src/nested/run_tests.rs` | EDIT | `+ child_memory_script`; `~ test_a_child_writes_no_checkpoint_of_its_own` | — | — | — |
| `rustd/crates/runner/afr_agent/tests/providers/support/mod.rs` | EDIT | `~ run_with` | — | — | — |
| `rustd/crates/runner/afr_executor/src/api/mod.rs` | EDIT | `+ Executor::recovery_idle` | `~ Executor` | — | — |
| `rustd/crates/runner/afr_executor/src/client/mod.rs` | EDIT | `+ Client::recovery_idle` | — | — | — |
| `rustd/crates/runner/afr_executor/src/client/tests.rs` | EDIT | `+ recovery_idle_preserves_the_executors_explicit_busy_and_idle_answers` | — | — | — |
| `rustd/crates/runner/afr_executor/src/protocol/mod.rs` | EDIT | — | — | `+ RecoveryParams`; `+ RecoveryProof` | `+ METHOD_RECOVERY_IDLE` |
| `rustd/crates/runner/afr_executor/src/server/recovery_tests.rs` | ADD | `+ a_file_worker_remains_busy_after_its_call_has_left_the_agent`; `+ query` | — | `+ TestResult` | — |
| `rustd/crates/runner/afr_executor/src/server/session.rs` | EDIT | `~ Session::route` | — | — | — |
| `rustd/crates/runner/afr_memory/Cargo.toml` | EDIT | — | — | — | `documentation/build/version metadata` |
| `rustd/crates/runner/afr_memory/src/error.rs` | EDIT | `~ Error::code`; `~ Error::detail`; `+ recovery_invalid` | — | `~ ErrorKind` | — |
| `rustd/crates/runner/afr_memory/src/hydrated.rs` | EDIT | `+ Hydrated::recover`; `+ Hydrated::snapshot` | — | — | — |
| `rustd/crates/runner/afr_memory/src/lib.rs` | EDIT | `+ MemoryBackend::snapshot` | `~ MemoryBackend` | — | — |
| `rustd/crates/runner/afr_memory/src/recovery.rs` | ADD | `+ MemoryRecovery::fmt`; `+ MemoryRecovery::new`; `+ owned_entries`; `+ push_budget` | — | `+ MemoryRecovery` | `+ MAX_HIDDEN_KEYS` |
| `rustd/crates/runner/afr_memory/src/recovery_tests.rs` | ADD | `+ Remote::recall`; `+ entry`; `+ inconsistent_keys_and_unbounded_recall_state_cannot_restore`; `+ recovery_preserves_local_memory_changes_and_recall_budget`; `+ recovery_refuses_a_revoked_shared_publish_grant` | — | `+ Remote`; `+ TestResult` | — |
| `rustd/crates/runner/afr_sandbox/src/bubblewrap_engine/mod.rs` | EDIT | `+ Bubblewrapped::recovery_quiescent` | — | — | — |
| `rustd/crates/runner/afr_sandbox/src/bubblewrap_engine/parts/mod.rs` | EDIT | `+ Parts::tenant_empty` | — | — | — |
| `rustd/crates/runner/afr_sandbox/src/bubblewrap_engine/tests/freeze.rs` | EDIT | `~ test_a_prepared_sandbox_freezes_and_thaws_through_its_lease_cgroup` | — | — | — |
| `rustd/crates/runner/afr_sandbox/src/cgroup/mod.rs` | EDIT | `+ LeaseCgroup::tenant_empty` | — | — | — |
| `rustd/crates/runner/afr_sandbox/src/cgroup/recovery_tests.rs` | ADD | `+ recovery_quiescence_reads_the_entire_tenant_subtree` | — | — | — |
| `rustd/crates/runner/afr_sandbox/src/engine/mod.rs` | EDIT | `+ Sandbox::recovery_quiescent` | `~ Sandbox` | — | — |
| `rustd/crates/runner/afr_sandbox/src/engine/tests.rs` | EDIT | `+ an_engine_without_recovery_proof_refuses_capture` | — | — | — |
| `rustd/crates/runner/afr_supervisor/src/authoring/mod.rs` | EDIT | `~ Lessee::author` | — | — | — |
| `rustd/crates/runner/afr_supervisor/src/client/mod.rs` | EDIT | — | — | — | `module/test wiring` |
| `rustd/crates/runner/afr_supervisor/src/client/stream.rs` | EDIT | `~ ControlPlane::report` | — | — | — |
| `rustd/crates/runner/afr_supervisor/src/client/turn_checkpoint.rs` | ADD | `+ ControlPlane::turn_checkpoint`; `+ Turn::decode`; `+ Turn::expected`; `+ Turn::outgoing` | — | `+ Turn`; `+ Turn::Output` | `+ Turn::VERB` |
| `rustd/crates/runner/afr_supervisor/src/client/verb.rs` | EDIT | `~ Verb::as_str`; `~ Verb::code`; `~ Verb::method` | — | `~ Verb` | `~ Verb::ALL` |
| `rustd/crates/runner/afr_supervisor/src/downlink/dispatch.rs` | EDIT | `~ Dispatch::frame` | — | — | — |
| `rustd/crates/runner/afr_supervisor/src/downlink/pending.rs` | EDIT | `~ Expected::accepts` | — | `~ Expected` | — |
| `rustd/crates/runner/afr_supervisor/src/downlink/pending_tests.rs` | EDIT | `+ field`; `+ turn_replies_require_lease_operation_kind_and_matching_success_variant` | — | — | — |
| `rustd/crates/runner/afr_supervisor/src/downlink/policy/tests.rs` | EDIT | `~ test_a_saturation_release_beats_at_once` | — | — | — |
| `rustd/crates/runner/afr_supervisor/src/error/classify.rs` | EDIT | `~ Error::code`; `~ Error::push_failure`; `~ Error::refusal_code`; `~ Error::refusal_status` | — | — | — |
| `rustd/crates/runner/afr_supervisor/src/error/mod.rs` | EDIT | — | — | `~ ErrorKind` | — |
| `rustd/crates/runner/afr_supervisor/src/error/raise.rs` | EDIT | `+ capture_limit`; `+ capture_unavailable` | — | — | — |
| `rustd/crates/runner/afr_supervisor/src/holds/keeper.rs` | EDIT | `+ Keeper::accept`; `~ Keeper::cap`; `~ Keeper::destroy`; `~ Keeper::new`; `- Keeper::occupy`; `~ Keeper::park`; `+ Keeper::room`; `~ Keeper::run` | — | `~ Keeper` | — |
| `rustd/crates/runner/afr_supervisor/src/holds/keeper_tests.rs` | EDIT | `~ test_saturation_that_releases_nothing_does_not_ring` | — | — | — |
| `rustd/crates/runner/afr_supervisor/src/holds/mod.rs` | EDIT | `+ Holds::accept`; `- Holds::occupy`; `- Occupied::drop` | — | `- Occupied`; `~ Request` | — |
| `rustd/crates/runner/afr_supervisor/src/holds/occupancy.rs` | ADD | `+ Holds::occupy`; `+ Occupant::parts`; `+ Occupied::drop` | — | `+ Occupant`; `+ Occupied` | — |
| `rustd/crates/runner/afr_supervisor/src/holds/shutdown_tests.rs` | EDIT | `~ test_shutdown_destroys_every_hold_and_waits` | — | — | — |
| `rustd/crates/runner/afr_supervisor/src/holds/tests.rs` | EDIT | `~ test_a_finished_lease_frees_its_worker`; `~ test_fewer_workers_release_the_oldest_holds`; `~ test_holds_capped_oldest_first`; `~ test_release_ends_one_fleets_hold_for_the_reason_given`; `~ test_saturation_releases_holds`; `~ test_the_last_free_worker_keeps_the_hold_of_the_fleet_it_serves` | — | — | — |
| `rustd/crates/runner/afr_supervisor/src/holds/transfer_tests.rs` | ADD | `+ accepted_hold_deadlines_only_shorten_the_exact_parked_attempt`; `+ cancelled_park_and_guard_drop_leave_no_busy_reservation`; `+ concurrent_authoring_guard_drops_release_only_their_own_attempts`; `+ test_a_hold_never_takes_the_last_free_worker`; `+ test_authoring_occupancy_preserves_the_keeper_reserve`; `+ test_two_worker_hold_transfers_count_each_sandbox_once` | — | — | — |
| `rustd/crates/runner/afr_supervisor/src/lease_loop/build.rs` | EDIT | `~ LeaseRun::acquire`; `~ LeaseRun::built` | — | — | — |
| `rustd/crates/runner/afr_supervisor/src/lease_loop/drive.rs` | EDIT | `~ LeaseRun::drive`; `+ Recovered::from`; `~ tests::Waiting::claim` | — | `+ Recovered` | — |
| `rustd/crates/runner/afr_supervisor/src/lease_loop/hold.rs` | EDIT | `~ LeaseRun::keep` | — | — | — |
| `rustd/crates/runner/afr_supervisor/src/lease_loop/mod.rs` | EDIT | `~ Lessee::run_lease` | — | — | — |
| `rustd/crates/runner/afr_supervisor/src/lease_loop/recovery.rs` | EDIT | `~ LeaseRun::capture_frozen`; `~ LeaseRun::prepare_recovery`; `~ api_toolbox`; `+ prove_quiescent`; `~ recovered_history`; `~ verify_fresh_checkout` | — | `~ State` | — |
| `rustd/crates/runner/afr_supervisor/src/lease_loop/recovery_boundary.rs` | ADD | `+ BoundaryOwner::beside`; `+ BoundaryOwner::new`; `+ Sink::save`; `+ Sink::workspace` | — | `+ BoundaryOwner`; `+ Job`; `+ Sink`; `+ Workspace` | — |
| `rustd/crates/runner/afr_supervisor/src/lease_loop/recovery_capture.rs` | ADD | `+ BoundaryOwner::capture`; `+ BoundaryOwner::capture_ready`; `+ BoundaryOwner::publish`; `+ BoundaryOwner::publish_task`; `+ BoundaryOwner::publish_turn`; `+ executor_idle`; `+ super::LeaseRun::capture_idle` | — | `+ Ready` | — |
| `rustd/crates/runner/afr_supervisor/src/lease_loop/settle.rs` | EDIT | `~ LeaseRun::settle` | — | — | — |
| `rustd/crates/runner/afr_supervisor/src/lease_loop/task_workspace.rs` | ADD | `+ LeaseRun::native_base`; `+ LeaseRun::native_workspace` | — | — | — |
| `rustd/crates/runner/afr_supervisor/src/lease_loop/tests/holds/acknowledgement.rs` | ADD | `+ a_hold_receipt_for_a_different_attempt_cannot_keep_the_park`; `+ a_matching_receipt_cannot_extend_the_requested_hold`; `+ a_successful_report_without_an_accepted_hold_discards_the_exact_park` | — | — | — |
| `rustd/crates/runner/afr_supervisor/src/lease_loop/tests/holds/reuse.rs` | EDIT | `~ test_a_lease_on_the_last_free_worker_releases_other_holds`; `~ test_another_fleets_lease_leaves_the_hold_alone` | — | — | — |
| `rustd/crates/runner/afr_supervisor/src/lease_loop/tests/recovery/ownership.rs` | EDIT | — | — | — | `module/test wiring` |
| `rustd/crates/runner/afr_supervisor/src/lease_loop/tests/recovery/ownership/boundary.rs` | ADD | `+ a_foreign_snapshot_cancels_before_native_io`; `+ a_refused_or_busy_freeze_still_thaws_without_a_native_save`; `+ a_vanished_agent_cannot_release_the_owner_before_thaw_finishes`; `+ an_abandoned_executor_file_job_prevents_freeze_and_publication`; `+ an_oversized_ordinary_workspace_skips_after_thaw_without_stopping`; `+ building_workspace_cannot_publish_api_only_history`; `+ failed_thaw_cancels_the_run_even_when_storage_was_only_unavailable`; `+ snapshot`; `+ unavailable` | — | — | — |
| `rustd/crates/runner/afr_supervisor/src/lease_loop/turn_recovery.rs` | ADD | `+ LeaseRun::prepare_turn_recovery` | — | — | — |
| `rustd/crates/runner/afr_supervisor/src/lease_loop/work.rs` | EDIT | `~ LeaseRun::execute` | — | — | — |
| `rustd/crates/runner/afr_supervisor/src/lease_loop/workspace.rs` | EDIT | `~ LeaseRun::sandboxed`; `+ LeaseRun::workspace_ready` | — | — | — |
| `rustd/crates/runner/afr_supervisor/src/recovery/capture.rs` | EDIT | `~ Capture::capture`; `+ Capture::plan`; `~ Session::capture_chunks`; `~ Session::commit_catalog`; `+ Session::write_pages` | — | — | — |
| `rustd/crates/runner/afr_supervisor/src/recovery/catalog.rs` | EDIT | `+ Catalog::add_history`; `~ Catalog::build`; `+ Catalog::mapped_entry`; `~ Catalog::pages` | — | — | — |
| `rustd/crates/runner/afr_supervisor/src/recovery/catalog_tests.rs` | EDIT | `~ a_near_full_manifest_keeps_all_private_history_on_a_separate_bounded_page`; `+ assert_paths`; `~ complete_layout_spans_pages_without_repeating_shared_content`; `+ ordinary_manifest_pages_fit_the_native_turn_limit`; `~ oversized_history_is_refused_before_a_native_plan_can_be_allocated` | — | — | — |
| `rustd/crates/runner/afr_supervisor/src/recovery/files.rs` | EDIT | `~ Identity::of`; `~ Workspace::directory`; `~ Workspace::inventory`; `~ regular`; `~ valid_path` | — | — | `- MAX_BYTES`; `- MAX_ENTRIES` |
| `rustd/crates/runner/afr_supervisor/src/recovery/inventory.rs` | ADD | `+ Inventory::charge_name`; `+ Inventory::check_stop`; `+ Inventory::directory`; `+ Inventory::entry`; `+ scan` | — | `+ Inventory` | `+ MAX_BYTES`; `+ MAX_ENTRIES`; `+ MAX_NAMES` |
| `rustd/crates/runner/afr_supervisor/src/recovery/layout.rs` | ADD | `+ Pages::finish`; `+ Pages::new`; `+ Pages::next`; `+ Pages::push` | — | `+ Pages` | — |
| `rustd/crates/runner/afr_supervisor/src/recovery/links.rs` | EDIT | `~ validate` | — | — | — |
| `rustd/crates/runner/afr_supervisor/src/recovery/mod.rs` | EDIT | — | — | — | `module/test wiring` |
| `rustd/crates/runner/afr_supervisor/src/recovery/tests/capture.rs` | EDIT | `~ incompatible_plan_and_live_writer_refuse_before_publication` | — | — | — |
| `rustd/crates/runner/afr_supervisor/src/recovery/tests/capture_busy.rs` | ADD | `+ a_busy_task_object_returns_the_nonfatal_unavailable_class_without_committing`; `+ busy_objects` | — | — | — |
| `rustd/crates/runner/afr_supervisor/src/recovery/tests/mod.rs` | EDIT | — | — | — | `module/test wiring` |
| `rustd/crates/runner/afr_supervisor/src/recovery/tests/turn_ack.rs` | ADD | `+ Ledger::begin`; `+ Ledger::conflict`; `+ Ledger::default`; `+ Ledger::respond`; `+ confirm_commit_and_allow_acks`; `+ drained`; `+ losing_every_commit_ack_does_not_permanently_stall_later_captures`; `+ snapshot` | — | `+ Ledger` | — |
| `rustd/crates/runner/afr_supervisor/src/recovery/transfer_http/server.rs` | EDIT | `+ Server::configured`; `~ Server::finish`; `+ Server::simultaneous`; `~ Server::with_headers`; `~ serve` | — | `~ Reply`; `~ Server` | — |
| `rustd/crates/runner/afr_supervisor/src/recovery/turn/capture.rs` | ADD | `+ NativeTurn::capture`; `+ NativeTurn::publish`; `+ TurnSession::capture_files`; `+ catalog`; `+ intent` | — | — | — |
| `rustd/crates/runner/afr_supervisor/src/recovery/turn/files.rs` | ADD | `+ TurnSession::download`; `+ TurnSession::file`; `+ TurnSession::manifest`; `+ TurnSession::restore_files`; `+ append` | — | `+ Bounds` | — |
| `rustd/crates/runner/afr_supervisor/src/recovery/turn/files_tests.rs` | ADD | `+ assert_files`; `+ cancellation_after_the_manifest_prevents_directory_creation`; `+ encrypted`; `+ fixture_plane`; `+ ordinary_file_chunks_share_a_live_batch_and_restore_in_manifest_order`; `+ page`; `+ pin`; `+ restore`; `+ scope` | — | `+ Encrypted` | `+ ID`; `+ PAGE` |
| `rustd/crates/runner/afr_supervisor/src/recovery/turn/mod.rs` | ADD | `+ NativeTurn::adopt`; `+ NativeTurn::command`; `+ NativeTurn::new`; `+ NativeTurn::request` | — | `+ NativeTurn` | — |
| `rustd/crates/runner/afr_supervisor/src/recovery/turn/publication.rs` | ADD | `+ NativeTurn::acknowledge`; `+ NativeTurn::commit_capture`; `+ NativeTurn::reconcile`; `+ NativeTurn::settle_refusal`; `+ Pending::drop` | — | `+ Pending`; `+ Publication` | — |
| `rustd/crates/runner/afr_supervisor/src/recovery/turn/restore.rs` | ADD | `+ NativeTurn::restore`; `+ validate` | — | — | — |
| `rustd/crates/runner/afr_supervisor/src/recovery/turn/session.rs` | ADD | `+ TurnSession::authorize`; `+ TurnSession::new`; `+ TurnSession::register`; `+ TurnSession::renew`; `+ TurnSession::upload` | — | `+ TurnSession` | — |
| `rustd/crates/runner/afr_supervisor/src/test_support/daemon/stream.rs` | EDIT | `+ authoring_reply`; `+ checkpoint_reply`; `~ response` | — | — | — |
| `rustd/crates/runner/afr_supervisor/src/test_support/rig.rs` | EDIT | `+ acknowledge_report`; `~ daemon` | — | — | — |
| `rustd/crates/runner/afr_supervisor/src/test_support/sandbox.rs` | DELETE | `- FakeEngine::prepare`; `- FakeExecutor::kill`; `- FakeExecutor::list_dir`; `- FakeExecutor::read_file`; `- FakeExecutor::spawn`; `- FakeExecutor::write`; `- FakeExecutor::write_file`; `- FakeSandbox::destroy`; `- FakeSandbox::executor`; `- FakeSandbox::freeze`; `- FakeSandbox::is_running`; `- FakeSandbox::kill`; `- FakeSandbox::reallow`; `- FakeSandbox::step`; `- FakeSandbox::thaw`; `- FakeSandbox::toolbox_digest`; `- FakeSandbox::workspace`; `- bound` | — | `- FakeEngine`; `- FakeExecutor`; `- FakeSandbox`; `- Freezer`; `- Kill` | `- DESTROY`; `- EXECUTOR_GONE`; `- KILL`; `- NO_FREEZER`; `- NO_REFILL`; `- STILL_RUNNING` |
| `rustd/crates/runner/afr_supervisor/src/test_support/sandbox/executor.rs` | ADD | `+ FakeExecutor::kill`; `+ FakeExecutor::list_dir`; `+ FakeExecutor::read_file`; `+ FakeExecutor::recovery_idle`; `+ FakeExecutor::spawn`; `+ FakeExecutor::write`; `+ FakeExecutor::write_file` | — | `+ FakeExecutor` | — |
| `rustd/crates/runner/afr_supervisor/src/test_support/sandbox/mod.rs` | ADD | `+ FakeEngine::prepare`; `+ FakeSandbox::destroy`; `+ FakeSandbox::executor`; `+ FakeSandbox::freeze`; `+ FakeSandbox::is_running`; `+ FakeSandbox::kill`; `+ FakeSandbox::reallow`; `+ FakeSandbox::recovery_quiescent`; `+ FakeSandbox::step`; `+ FakeSandbox::thaw`; `+ FakeSandbox::toolbox_digest`; `+ FakeSandbox::workspace`; `+ bound` | — | `+ FakeEngine`; `+ FakeSandbox`; `+ Freezer`; `+ Kill` | `+ DESTROY`; `+ EXECUTOR_GONE`; `+ KILL`; `+ NO_FREEZER`; `+ NO_REFILL`; `+ STILL_RUNNING` |
| `rustd/crates/runner/afr_supervisor/src/test_util/mod.rs` | EDIT | `~ Healthy::to` | — | — | — |
| `rustd/crates/runner/afr_supervisor/src/test_util/native/reply.rs` | EDIT | `~ answer` | — | — | — |
| `rustd/crates/runner/afr_supervisor/src/uplink/mod.rs` | EDIT | `~ SessionSender::send` | — | — | — |
| `rustd/crates/runner/afr_supervisor/src/verbs/mod.rs` | EDIT | `~ FencedVerbs::claim` | — | — | — |
| `rustd/crates/runner/afr_supervisor/src/verbs/tests/decision/loop_boundary/mod.rs` | EDIT | `~ execute` | — | — | — |
| `rustd/crates/runner/afr_supervisor/src/verbs/tests/folds.rs` | EDIT | `~ a_claim_of_two_messages_hands_them_over_oldest_first` | — | — | — |
| `rustd/crates/runner/afr_tools/src/sandbox/sessions/mod.rs` | EDIT | `+ Sessions::is_quiescent` | — | — | — |
| `rustd/crates/runner/afr_tools/src/sandbox/sessions/tests.rs` | EDIT | `+ recovery_boundary_refuses_both_open_and_reserved_sessions` | — | — | — |
| `rustd/crates/runner/main/Cargo.toml` | EDIT | — | — | — | `documentation/build/version metadata` |
| `rustd/crates/runner/main/tests/recovery_process/main.rs` | ADD | `+ checked`; `+ main`; `+ serve` | — | `+ Arguments`; `+ Result` | — |
| `schema/20261108123100_runner_runners.sql` | EDIT | — | — | — | `table runner.runners`; `index idx_runners_decision_platform_model` |
| `schema/20261108123200_runner_leases.sql` | EDIT | — | — | — | `table runner.leases`; `index idx_leases_event_id_fencing_token`; `index idx_leases_fleet_id_status_fencing_token`; `index idx_leases_runner_id_created_at_id`; `index idx_leases_runner_id_status_lease_expires_at`; `index idx_leases_status_updated_at`; `index idx_leases_lease_expires_at_unbilled`; `index idx_runner_leases_task` |
| `schema/20261108130500_fleet_task_identity_keys.sql` | EDIT | — | — | — | `table fleet.task_identity_keys`; `index uq_task_identity_active`; `+ index idx_task_identity_retention` |
| `schema/20261108131100_fleet_task_storage_purges.sql` | EDIT | — | — | — | `table fleet.task_storage_purges`; `index idx_task_storage_purges_due`; `+ index idx_task_storage_purges_identity` |
| `schema/20261117154626_runner_turn_recovery.sql` | ADD | — | — | — | `+ table runner.turn_recovery` |
| `schema/20261117154627_runner_turn_checkpoints.sql` | ADD | — | — | — | `+ table runner.turn_checkpoints`; `+ index uq_turn_checkpoint_pending`; `+ index idx_turn_checkpoints_workspace`; `+ index idx_turn_checkpoints_ready`; `+ index idx_turn_checkpoints_expiry` |
| `schema/20261117154628_runner_turn_checkpoint_pages.sql` | ADD | — | — | — | `+ table runner.turn_checkpoint_pages`; `+ function runner.turn_checkpoint_pages_freeze`; `+ trigger trg_turn_checkpoint_pages_freeze` |
| `schema/20261117154629_runner_turn_checkpoint_refs.sql` | ADD | — | — | — | `+ table runner.turn_checkpoint_refs`; `+ index idx_turn_refs_object`; `+ function runner.turn_checkpoint_refs_refuse_update`; `+ trigger trg_turn_checkpoint_refs_refuse_update` |
| `schema/20261117154630_runner_turn_checkpoint_pins.sql` | ADD | — | — | — | `+ table runner.turn_checkpoint_pins`; `+ index uq_turn_pin_live`; `+ index uq_turn_pin_operation`; `+ index idx_turn_pins_expiry` |
| `schema/20261117154631_runner_turn_checkpoint_operations.sql` | ADD | — | — | — | `+ table runner.turn_checkpoint_operations`; `+ function runner.turn_checkpoint_operations_refuse_update`; `+ trigger trg_turn_checkpoint_operations_refuse_update`; `+ index idx_turn_operations_retention`; `+ index idx_turn_operations_event_retention` |
| `ui/packages/app/components/domain/activity/messages/fleetMessageReaders.test.ts` | EDIT | — | — | — | `module/test wiring` |
| `ui/packages/app/components/domain/activity/messages/fleetMessageReaders.ts` | EDIT | `+ readRestartAttempt`; `+ readRunRestarted` | — | — | — |
| `ui/packages/app/components/domain/activity/replies/FleetReplyAnnouncements.tsx` | EDIT | `~ SettledReplyStatus`; `+ useRestartAnnouncement`; `~ useSettleAnnouncement` | — | — | `+ AnnounceReply`; `- AnnounceSettled` |
| `ui/packages/app/components/domain/activity/replies/FleetReplyBody.test.tsx` | EDIT | `~ Transcriptless` | — | — | — |
| `ui/packages/app/components/domain/activity/replies/FleetReplyBody.tsx` | EDIT | `~ FleetReply` | — | — | — |
| `ui/packages/app/components/domain/conversation/stream/useFleetEventStream.ts` | EDIT | `~ convertEvent` | — | — | — |
| `ui/packages/app/components/domain/conversation/thread/useFleetThreadEntries.test.tsx` | EDIT | — | — | — | `module/test wiring` |
| `ui/packages/app/components/domain/conversation/thread/useFleetThreadEntries.ts` | EDIT | `~ hasReplyRow` | — | — | — |
| `ui/packages/app/lib/api/events-types.ts` | EDIT | — | — | — | `~ FRAME_KIND` |
| `ui/packages/app/lib/api/events.ts` | EDIT | — | — | `~ ActivityLiveFrame` | — |
| `ui/packages/app/lib/events/event-grouping.ts` | EDIT | `~ groupable` | — | — | — |
| `ui/packages/app/lib/streaming/fleet-stream-admitted.ts` | EDIT | `+ adoptInstant` | — | — | — |
| `ui/packages/app/lib/streaming/fleet-stream-attempt.ts` | ADD | `+ acceptAttemptFrame`; `+ restart` | — | — | `+ FENCE`; `+ MAX_FENCE`; `+ OUTPUT_KINDS` |
| `ui/packages/app/lib/streaming/fleet-stream-detail-recovery.ts` | EDIT | `~ readUntilAnswered`; `~ recoverFinalReply`; `~ refreshSavedTrace` | — | — | — |
| `ui/packages/app/lib/streaming/fleet-stream-dispatch.ts` | EDIT | `~ applyFrame` | — | — | `- RUNNER_ACTIVITY_KINDS` |
| `ui/packages/app/lib/streaming/fleet-stream-entry.ts` | EDIT | `~ createEntry` | — | `~ Entry` | — |
| `ui/packages/app/lib/streaming/fleet-stream-frames.ts` | EDIT | `- adoptInstant`; `~ mergeBackfill` | — | — | — |
| `ui/packages/app/lib/streaming/fleet-stream-registry.attempt.test.ts` | ADD | `+ chunk`; `+ hint`; `+ live`; `+ validActivityFrames` | — | — | `+ EVENT`; `+ entries` |
| `ui/packages/app/lib/streaming/fleet-stream-reply-registry.ts` | EDIT | `~ completeReply`; `~ disposeReplyStreams`; `~ settleFromDetail`; `~ settleRepliesFromBackfill`; `~ writeChunk` | — | — | — |
| `ui/packages/app/lib/streaming/fleet-stream-row.ts` | EDIT | — | — | `~ FleetEvent` | — |
| `ui/packages/app/lib/streaming/fleet-stream-tool-trace.ts` | EDIT | `~ settleTools` | — | — | — |
| `ui/packages/app/tests/e2e/fixtures/fleet-stream-browser-probe.ts` | EDIT | `~ render` | — | — | `+ reply`; `+ replyHost` |
| `ui/packages/app/tests/e2e/fixtures/restart-browser-probe.tsx` | ADD | `+ RestartBrowserProbe` | — | — | — |
| `ui/packages/app/tests/e2e/fleet-stream-renewal.spec.ts` | EDIT | — | — | — | `+ css` |
| `ui/packages/app/tests/helpers/fake-event-source.ts` | EDIT | `~ FakeEventSource::emit` | — | `~ FakeEventSource` | — |
