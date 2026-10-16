//! Every fleet-plane integration suite, in one test binary.
//!
//! One binary rather than eighteen: cargo runs test BINARIES serially and
//! the tests inside one binary in parallel, so eighteen binaries were
//! eighteen serial stretches that each re-paid process start, dynamic
//! linking, and its own datastore connections. The support modules are
//! declared once here and reached as `crate::<name>` from each suite,
//! which is the shape `afd_api` already uses for its four planes.

mod installed;
mod lease_renewal;

#[path = "support/fleet_setup.rs"]
mod setup;

#[path = "../../afd_dragonfly/tests/support/fake_redis/mod.rs"]
#[allow(
    unused_imports,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::format_push_string,
    reason = "test support shared with `afd_dragonfly`, which uses the half this suite does not"
)]
mod fake_redis;
#[path = "support/fleet_lease_reads.rs"]
mod lease_reads;
#[path = "support/fleet_queue.rs"]
mod queue;
#[path = "support/fleet_recovery_seed.rs"]
mod recovery_seed;
#[path = "support/fleet_report_commit.rs"]
mod report_commit;
#[path = "support/fleet_report_reads.rs"]
mod report_reads;
#[path = "support/fleet_report_seed.rs"]
mod report_seed;
#[path = "support/fleet_requests.rs"]
mod requests;
#[path = "support/fleet_lease_seed.rs"]
mod seed;
#[path = "../../afd_dragonfly/tests/support/subscriber.rs"]
mod subscriber;
#[path = "support/fleet_fixtures.rs"]
mod support;
#[path = "support/view_heartbeat.rs"]
mod view_heartbeat;

#[path = "integration_activity_publish.rs"]
mod integration_activity_publish;
mod integration_admission_recovery;
mod integration_credential_mint;
#[path = "integration_delivery_lookup.rs"]
mod integration_delivery_lookup;
#[path = "integration_gate_grants.rs"]
mod integration_gate_grants;
mod integration_held_sandbox;
#[path = "integration_lease_affinity.rs"]
mod integration_lease_affinity;
#[path = "integration_lease_assign.rs"]
mod integration_lease_assign;
#[path = "integration_lease_block.rs"]
mod integration_lease_block;
#[path = "integration_lease_fairness.rs"]
mod integration_lease_fairness;
mod integration_lease_gates;
#[path = "integration_lease_history.rs"]
mod integration_lease_history;
#[path = "integration_lease_issue.rs"]
mod integration_lease_issue;
#[path = "integration_lease_plan.rs"]
mod integration_lease_plan;
#[path = "integration_lease_ready.rs"]
mod integration_lease_ready;
#[path = "integration_lease_retention.rs"]
mod integration_lease_retention;
#[path = "integration_lease_started.rs"]
mod integration_lease_started;
mod integration_lease_wake;
#[path = "integration_ledger_fleet_name.rs"]
mod integration_ledger_fleet_name;
#[path = "integration_ledger_orphan_charge.rs"]
mod integration_ledger_orphan_charge;
#[path = "integration_ledger_reads.rs"]
mod integration_ledger_reads;
mod integration_ledger_scope;
#[path = "integration_memory_capture.rs"]
mod integration_memory_capture;
#[path = "integration_memory_hydrate_order.rs"]
mod integration_memory_hydrate_order;
#[path = "integration_money_gates.rs"]
mod integration_money_gates;
#[path = "integration_recovery_budget.rs"]
mod integration_recovery_budget;
#[path = "integration_recovery_outage.rs"]
mod integration_recovery_outage;
#[path = "integration_recovery_progress.rs"]
mod integration_recovery_progress;
#[path = "integration_recovery_progress_deleted.rs"]
mod integration_recovery_progress_deleted;
#[path = "integration_recovery_progress_live.rs"]
mod integration_recovery_progress_live;
#[path = "integration_recovery_progress_restart.rs"]
mod integration_recovery_progress_restart;
#[path = "integration_recovery_progress_walk.rs"]
mod integration_recovery_progress_walk;
#[path = "integration_recovery_row_locks.rs"]
mod integration_recovery_row_locks;
#[path = "integration_renew_clamp.rs"]
mod integration_renew_clamp;
#[path = "integration_renew_coverage.rs"]
mod integration_renew_coverage;
#[path = "integration_reply_destination.rs"]
mod integration_reply_destination;
#[path = "integration_report_commit.rs"]
mod integration_report_commit;
#[path = "integration_report_fence.rs"]
mod integration_report_fence;
mod integration_report_owes_destination;
#[path = "integration_report_settle.rs"]
mod integration_report_settle;
#[path = "integration_runner_admin.rs"]
mod integration_runner_admin;
mod integration_runner_beat;
#[path = "integration_runner_maintenance.rs"]
mod integration_runner_maintenance;
#[path = "integration_runner_retire.rs"]
mod integration_runner_retire;
#[path = "integration_runner_row.rs"]
mod integration_runner_row;
#[path = "integration_runner_views.rs"]
mod integration_runner_views;
#[path = "integration_runner_views_malformed.rs"]
mod integration_runner_views_malformed;
#[path = "integration_steer_admitted_stamp.rs"]
mod integration_steer_admitted_stamp;
#[path = "integration_steer_receipt_cutover.rs"]
mod integration_steer_receipt_cutover;
mod integration_terminal_redelivery;
#[path = "integration_wall_counters.rs"]
mod integration_wall_counters;
#[path = "verdict_matrix.rs"]
mod verdict_matrix;
