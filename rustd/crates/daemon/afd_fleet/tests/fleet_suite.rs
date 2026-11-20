//! Every fleet-plane integration suite, in one test binary.
//!
//! One binary rather than eighteen: cargo runs test BINARIES serially and
//! the tests inside one binary in parallel, so eighteen binaries were
//! eighteen serial stretches that each re-paid process start, dynamic
//! linking, and its own datastore connections. The support modules are
//! declared once here and reached as `crate::<name>` from each suite,
//! which is the shape `afd_api` already uses for its four planes.
#![expect(
    clippy::print_stderr,
    reason = "a suite's helpers print their measured evidence into the lane log"
)]

mod installed;
mod integration_model_seed;
mod integration_tasks;
mod integration_turn_checkpoints;
mod issue_lifecycle;
mod run_renewal;

#[path = "support/fleet_setup.rs"]
mod setup;

#[path = "support/fleet_billing_lines.rs"]
mod billing_lines;
#[path = "support/fleet_faulty_biller.rs"]
mod faulty_biller;
#[path = "support/fleet_queue.rs"]
mod queue;
#[path = "support/fleet_recovery_seed.rs"]
mod recovery_seed;
#[path = "support/fleet_report_commit.rs"]
mod report_commit;
#[path = "support/fleet_report_reads.rs"]
mod report_reads;
#[path = "support/fleet_report_seed/mod.rs"]
mod report_seed;
#[path = "support/fleet_requests.rs"]
mod requests;
#[path = "support/fleet_rows.rs"]
mod rows;
#[path = "support/fleet_run_reads.rs"]
mod run_reads;
#[path = "support/fleet_run_seed.rs"]
mod seed;
#[path = "support/fleet_fixtures.rs"]
mod support;
#[path = "support/fleet_tenant_runners.rs"]
mod tenant_runners;
#[path = "support/view_heartbeat.rs"]
mod view_heartbeat;

#[path = "integration_activity_publish.rs"]
mod integration_activity_publish;
#[path = "integration_assignment_delivery.rs"]
mod integration_assignment_delivery;
#[path = "integration_billing_relay.rs"]
mod integration_billing_relay;
#[path = "integration_billing_sweep.rs"]
mod integration_billing_sweep;
#[path = "integration_billing_sweep_faults.rs"]
mod integration_billing_sweep_faults;
mod integration_credential_mint;
#[path = "integration_gate_grants.rs"]
mod integration_gate_grants;
mod integration_held_sandbox;
#[path = "integration_issue_host.rs"]
mod integration_issue_host;
mod integration_ledger_queue;
#[path = "integration_ledger_reads.rs"]
mod integration_ledger_reads;
#[path = "integration_memory_capture.rs"]
mod integration_memory_capture;
#[path = "integration_memory_load_order.rs"]
mod integration_memory_load_order;
#[path = "integration_money_gates.rs"]
mod integration_money_gates;
#[path = "integration_money_gates_debt.rs"]
mod integration_money_gates_debt;
#[path = "integration_renew_batch.rs"]
mod integration_renew_batch;
#[path = "integration_renew_caches.rs"]
mod integration_renew_caches;
#[path = "integration_renew_clamp.rs"]
mod integration_renew_clamp;
#[path = "integration_renew_coverage/mod.rs"]
mod integration_renew_coverage;
#[path = "integration_renew_gate_edges.rs"]
mod integration_renew_gate_edges;
#[path = "integration_renew_lock_order.rs"]
mod integration_renew_lock_order;
#[path = "integration_renew_refusals.rs"]
mod integration_renew_refusals;
#[path = "integration_reply_destination.rs"]
mod integration_reply_destination;
#[path = "integration_report_commit.rs"]
mod integration_report_commit;
#[path = "integration_report_fence.rs"]
mod integration_report_fence;
#[path = "integration_report_items.rs"]
mod integration_report_items;
#[path = "integration_report_memory.rs"]
mod integration_report_memory;
mod integration_report_owes_destination;
#[path = "integration_report_pricing.rs"]
mod integration_report_pricing;
#[path = "integration_report_renewal_lock.rs"]
mod integration_report_renewal_lock;
#[path = "integration_report_statement.rs"]
mod integration_report_statement;
#[path = "integration_run_affinity.rs"]
mod integration_run_affinity;
#[path = "integration_run_assign.rs"]
mod integration_run_assign;
#[path = "integration_run_assignment_race.rs"]
mod integration_run_assignment_race;
#[path = "integration_run_billing_races.rs"]
mod integration_run_billing_races;
#[path = "integration_run_block.rs"]
mod integration_run_block;
#[path = "integration_run_caches.rs"]
mod integration_run_caches;
#[path = "integration_run_edges.rs"]
mod integration_run_edges;
#[path = "integration_run_expiry_lock.rs"]
mod integration_run_expiry_lock;
#[path = "integration_run_fairness.rs"]
mod integration_run_fairness;
mod integration_run_folds;
mod integration_run_gates;
#[path = "integration_run_history.rs"]
mod integration_run_history;
#[path = "integration_run_issue.rs"]
mod integration_run_issue;
#[path = "integration_run_issue_lock_wait.rs"]
mod integration_run_issue_lock_wait;
#[path = "integration_run_issue_refusals.rs"]
mod integration_run_issue_refusals;
#[path = "integration_run_plan.rs"]
mod integration_run_plan;
#[path = "integration_run_started.rs"]
mod integration_run_started;
#[path = "integration_run_usage_flush.rs"]
mod integration_run_usage_flush;
#[path = "integration_run_verbs.rs"]
mod integration_run_verbs;
#[path = "integration_runner_admin.rs"]
mod integration_runner_admin;
mod integration_runner_beat;
#[path = "integration_runner_maintenance.rs"]
mod integration_runner_maintenance;
#[path = "integration_runner_retire.rs"]
mod integration_runner_retire;
#[path = "integration_runner_row.rs"]
mod integration_runner_row;
#[path = "integration_runner_tenant.rs"]
mod integration_runner_tenant;
#[path = "integration_runner_views.rs"]
mod integration_runner_views;
#[path = "integration_runner_views_malformed.rs"]
mod integration_runner_views_malformed;
#[path = "integration_steer_admitted_stamp.rs"]
mod integration_steer_admitted_stamp;
#[path = "integration_tenant_placement/mod.rs"]
mod integration_tenant_placement;
mod integration_terminal_redelivery;
#[path = "integration_unanswered_notice.rs"]
mod integration_unanswered_notice;
#[path = "integration_wall_counters.rs"]
mod integration_wall_counters;
#[path = "verdict_matrix.rs"]
mod verdict_matrix;

#[path = "integration_run_issue_capacity.rs"]
mod integration_run_issue_capacity;

#[path = "integration_memory_capture_race.rs"]
mod integration_memory_capture_race;
