//! Operator and platform-administration HTTP regression suite.

mod harness;

#[path = "admin_operator_route_inventory.rs"]
mod admin_operator_route_inventory;
#[path = "admin_plane.rs"]
mod admin_plane;
#[path = "admin_scope_gates.rs"]
mod admin_scope_gates;
#[path = "integration_admin.rs"]
mod integration_admin;
#[path = "integration_operator_runner.rs"]
mod integration_operator_runner;
#[path = "operator_plane.rs"]
mod operator_plane;
