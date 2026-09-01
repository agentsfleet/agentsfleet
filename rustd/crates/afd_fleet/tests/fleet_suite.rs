//! Every fleet-plane integration suite, in one test binary.
//!
//! One binary rather than eighteen: cargo runs test BINARIES serially and
//! the tests inside one binary in parallel, so eighteen binaries were
//! eighteen serial stretches that each re-paid process start, dynamic
//! linking, and its own datastore connections. The support modules are
//! declared once here and reached as `crate::<name>` from each suite,
//! which is the shape `afd_api` already uses for its four planes.

#[path = "support/fleet_lease_reads.rs"]
mod lease_reads;
#[path = "support/fleet_queue.rs"]
mod queue;
#[path = "support/fleet_report_reads.rs"]
mod report_reads;
#[path = "support/fleet_report_seed.rs"]
mod report_seed;
#[path = "support/fleet_requests.rs"]
mod requests;
#[path = "support/fleet_lease_seed.rs"]
mod seed;
#[path = "support/fleet_fixtures.rs"]
mod support;
#[path = "support/view_heartbeat.rs"]
mod view_heartbeat;

#[path = "integration_activity_publish.rs"]
mod integration_activity_publish;
#[path = "integration_credential_mint.rs"]
mod integration_credential_mint;
#[path = "integration_gate_grants.rs"]
mod integration_gate_grants;
#[path = "integration_lease_affinity.rs"]
mod integration_lease_affinity;
#[path = "integration_lease_assign.rs"]
mod integration_lease_assign;
#[path = "integration_lease_block.rs"]
mod integration_lease_block;
#[path = "integration_lease_installed.rs"]
mod integration_lease_installed;
#[path = "integration_lease_issue.rs"]
mod integration_lease_issue;
#[path = "integration_memory_capture.rs"]
mod integration_memory_capture;
#[path = "integration_money_gates.rs"]
mod integration_money_gates;
#[path = "integration_renew_clamp.rs"]
mod integration_renew_clamp;
#[path = "integration_renew_coverage.rs"]
mod integration_renew_coverage;
#[path = "integration_report_settle.rs"]
mod integration_report_settle;
#[path = "integration_runner_admin.rs"]
mod integration_runner_admin;
#[path = "integration_runner_beat.rs"]
mod integration_runner_beat;
#[path = "integration_runner_maintenance.rs"]
mod integration_runner_maintenance;
#[path = "integration_runner_row.rs"]
mod integration_runner_row;
#[path = "integration_runner_views.rs"]
mod integration_runner_views;
#[path = "verdict_matrix.rs"]
mod verdict_matrix;
