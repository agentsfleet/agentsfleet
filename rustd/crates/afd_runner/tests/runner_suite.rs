//! Live runner sweep regression suite.

#[path = "integration_census.rs"]
mod integration_census;
#[path = "integration_reclaim.rs"]
mod integration_reclaim;
#[path = "integration_repair_dispatch.rs"]
mod integration_repair_dispatch;
#[path = "integration_sweeps.rs"]
mod integration_sweeps;
mod support;

#[path = "error_surface.rs"]
mod error_surface;
