//! Operator and platform-administration HTTP regression suite.

mod harness;

#[path = "admin_operator_route_inventory.rs"]
mod admin_operator_route_inventory;
mod admin_plane;
#[path = "admin_scope_gates.rs"]
mod admin_scope_gates;
#[path = "integration_admin.rs"]
mod integration_admin;
#[path = "integration_operator_runner.rs"]
mod integration_operator_runner;
#[path = "integration_operator_runner_edit.rs"]
mod integration_operator_runner_edit;
#[path = "operator_plane.rs"]
mod operator_plane;
#[path = "operator_plane_runner_edit.rs"]
mod operator_plane_runner_edit;
