//! Live runner plane regression suite.

mod capacity;

#[path = "integration_park_plans.rs"]
mod integration_park_plans;
mod integration_runner_plane;
mod rows;

#[path = "error_surface.rs"]
mod error_surface;

#[path = "placement_refusal.rs"]
mod placement_refusal;
