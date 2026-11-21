//! The model providers: `guffbolt`'s model seam implemented over every wire
//! (`docs/architecture/runner_execution.md` §Crates).
//!
//! A [`Connect`] picks the route a run's policy names and builds its
//! [`Provider`]. `guffbolt_ai::wire` speaks every wire over rig, retries a
//! send and streams the turn's chunks; the runner owns which providers a run
//! may reach, the guarded client under the wire (one HTTP client, no
//! redirect), the reply bound, and how a failure is reported and logged. The
//! model key lives only here, in the supervisor: no provider runs inside a
//! sandbox.

#![cfg_attr(not(test), deny(unused_crate_dependencies))]
#![deny(clippy::self_named_module_files)]

pub mod decision;
pub mod error;
pub mod observe;
#[cfg(any(test, feature = "test-util"))]
pub mod testing;

mod connect;
mod provider;
mod retry;
mod transport;
mod turn;

pub use self::connect::{Connect, Connector};
pub use self::error::{Error, Result};
pub use self::provider::{Provider, context_document};
pub use self::retry::settings::{RetryPolicy, RetrySettings};
