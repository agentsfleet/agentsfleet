//! Every daemon-and-runner suite, in one test binary: the runner's protocol
//! client, supervisor and agent loop driven end to end against a booted
//! `agentsfleetd`.
//!
//! # Why this crate is safe to aggregate
//!
//! Aggregation makes the suites run CONCURRENTLY against whatever they share.
//! The scenarios share the lane's database: `scenario_database` returns the
//! lane URL unchanged, and `scenario` says so, keeping runs apart by the
//! identifiers `unique_ids` mints rather than by a database apiece. Nothing
//! here asserts a global count over a shared table, and every bound port is
//! `EPHEMERAL`.
//!
//! # The support modules are declared here, once
//!
//! They are reached as `crate::<name>`, the spelling the helpers use among
//! themselves. `money_gate` is NOT hoisted: it stays declared inside
//! `integration_runner_e2e` with its original `#[path]`, because it opens with
//! `use super::*`, which resolves against the file that declares it.

#[path = "support/e2e_steer_assignments.rs"]
mod assignments;
#[path = "support/definition_code.rs"]
mod definition_code;
#[path = "support/definition_install/mod.rs"]
mod definition_install;
#[path = "support/definition_repair.rs"]
mod definition_repair;
#[path = "support/definition_responder.rs"]
mod definition_responder;
#[path = "support/definition_run.rs"]
mod definition_run;
#[path = "support/e2e.rs"]
mod e2e;
#[path = "support/e2e_db.rs"]
mod e2e_db;
#[path = "support/e2e_event.rs"]
mod e2e_event;
#[path = "support/e2e_native.rs"]
mod e2e_native;
#[path = "support/e2e_retire.rs"]
mod e2e_retire;
#[path = "support/e2e_seed.rs"]
mod e2e_seed;
#[path = "support/e2e_seed_keys.rs"]
mod e2e_seed_keys;
#[path = "support/e2e_steers.rs"]
mod steers;
use af_egress::testing::upstream as https;
use af_providers::testing::scripted as fake_model;
#[path = "integration_cluster_outcomes.rs"]
mod integration_cluster_outcomes;
#[path = "integration_fleet_list_name.rs"]
mod integration_fleet_list_name;
#[path = "integration_runner_activity/mod.rs"]
mod integration_runner_activity;
#[path = "integration_runner_activity_call_id.rs"]
mod integration_runner_activity_call_id;
#[path = "integration_runner_brackets.rs"]
mod integration_runner_brackets;
mod integration_runner_e2e;
#[path = "integration_runner_fleets.rs"]
mod integration_runner_fleets;
#[path = "integration_runner_fleets_paging.rs"]
mod integration_runner_fleets_paging;
#[path = "integration_runner_frame.rs"]
mod integration_runner_frame;
#[path = "integration_runner_messages.rs"]
mod integration_runner_messages;
#[path = "integration_runner_messages_fence.rs"]
mod integration_runner_messages_fence;
#[path = "integration_runner_schedule_tool.rs"]
mod integration_runner_schedule_tool;
#[path = "integration_runner_schedules.rs"]
mod integration_runner_schedules;
#[path = "integration_runner_schedules_edit.rs"]
mod integration_runner_schedules_edit;
#[path = "integration_runner_schedules_fence.rs"]
mod integration_runner_schedules_fence;
#[path = "integration_runner_schedules_refusals.rs"]
mod integration_runner_schedules_refusals;
#[path = "integration_runner_schedules_runs.rs"]
mod integration_runner_schedules_runs;
#[path = "integration_runner_shapes.rs"]
mod integration_runner_shapes;
#[path = "integration_runner_steers.rs"]
mod integration_runner_steers;
#[path = "integration_runner_steers_chain.rs"]
mod integration_runner_steers_chain;
#[path = "integration_runner_steers_fence.rs"]
mod integration_runner_steers_fence;
#[path = "integration_runner_steers_refusals.rs"]
mod integration_runner_steers_refusals;
mod integration_runner_stream;
#[path = "integration_rust_runner.rs"]
mod integration_rust_runner;
#[path = "integration_rust_runner_definitions.rs"]
mod integration_rust_runner_definitions;
#[path = "integration_rust_runner_fleet_context.rs"]
mod integration_rust_runner_fleet_context;
#[path = "integration_rust_runner_nested.rs"]
mod integration_rust_runner_nested;
#[path = "integration_rust_runner_repairs.rs"]
mod integration_rust_runner_repairs;
#[path = "integration_rust_runner_reviews.rs"]
mod integration_rust_runner_reviews;
#[path = "integration_rust_runner_setup/mod.rs"]
mod integration_rust_runner_setup;
#[path = "integration_rust_runner_telemetry.rs"]
mod integration_rust_runner_telemetry;
#[path = "integration_sweepers.rs"]
mod integration_sweepers;
#[path = "integration_tenant_registry.rs"]
mod integration_tenant_registry;
#[path = "integration_tool_call_details.rs"]
mod integration_tool_call_details;
#[path = "integration_tool_call_details_lifecycle.rs"]
mod integration_tool_call_details_lifecycle;
#[path = "integration_tool_call_refusals.rs"]
mod integration_tool_call_refusals;
#[path = "integration_tool_trace.rs"]
mod integration_tool_trace;
#[path = "integration_unreadable_config.rs"]
mod integration_unreadable_config;
#[path = "support/e2e_reads.rs"]
mod reads;
mod recovery_process;
#[path = "support/repair_continue.rs"]
mod repair_continue;
#[path = "support/repair_remote.rs"]
mod repair_remote;
#[path = "support/repair_walk.rs"]
mod repair_walk;
#[path = "support/e2e_schedules.rs"]
mod schedules;
#[path = "support/e2e_speaking.rs"]
mod speaking;
#[path = "support/e2e_tail.rs"]
mod tail;
#[path = "support/e2e_verbs.rs"]
mod verbs;
#[path = "support/e2e_wire.rs"]
mod wire;

#[path = "integration_grant_denial.rs"]
mod integration_grant_denial;
