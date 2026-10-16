//! Live runner sweep regression suite.

mod capacity;

#[path = "integration_census.rs"]
mod integration_census;
#[path = "integration_hold_plans.rs"]
mod integration_hold_plans;
#[path = "integration_reclaim.rs"]
mod integration_reclaim;
#[path = "integration_reclaim_faults.rs"]
mod integration_reclaim_faults;
#[path = "integration_reclaim_paging.rs"]
mod integration_reclaim_paging;
mod integration_sweeps;
mod support;

#[path = "error_surface.rs"]
mod error_surface;
