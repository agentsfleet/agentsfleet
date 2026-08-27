//! Read-only operator projections over fleet state.
//!
//! This crate is deliberately smaller than the runner execution plane. It owns
//! cross-table views an operator reads, while `afd_fleet` retains enrolment,
//! heartbeats, lease execution, settlement, and mutations. Keeping the read
//! projection separate prevents each dashboard join from enlarging the crate
//! on the latency-critical runner path.

#![forbid(unsafe_code)]
#![cfg_attr(not(test), deny(unused_crate_dependencies))]

mod error;
mod runner_leases;
mod sql;

pub use self::error::{Error, Result};
pub use self::runner_leases::RunnerLeaseHistory;
