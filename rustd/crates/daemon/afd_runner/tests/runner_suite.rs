//! Live runner sweep regression suite.

mod capacity;

#[path = "integration_census.rs"]
mod integration_census;
#[path = "integration_hold_plans.rs"]
mod integration_hold_plans;
mod integration_runner_plane;
mod integration_sweeps;
mod rows;

#[path = "error_surface.rs"]
mod error_surface;
