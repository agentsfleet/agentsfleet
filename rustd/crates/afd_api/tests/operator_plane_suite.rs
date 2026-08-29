//! Operator and platform-administration HTTP regression suite.

mod harness;

#[path = "admin_live.rs"]
mod admin_live;
#[path = "admin_operator_route_inventory.rs"]
mod admin_operator_route_inventory;
#[path = "admin_plane.rs"]
mod admin_plane;
#[path = "admin_scope_gates.rs"]
mod admin_scope_gates;
#[path = "operator_plane.rs"]
mod operator_plane;
#[path = "operator_runner_live.rs"]
mod operator_runner_live;
