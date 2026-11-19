//! The model providers: `guffbolt`'s model seam implemented over every wire
//! (`docs/architecture/runner_execution.md` §Crates).
//!
//! A [`Connect`] picks the route a run's policy names and builds its
//! [`Provider`]. rig speaks every wire; the runner owns which providers a
//! run may reach, the transport under rig (one HTTP client, no redirect,
//! bounded retry) and the turn's chunks. The model key lives only here, in
//! the supervisor: no provider runs inside a sandbox.

#![cfg_attr(not(test), deny(unused_crate_dependencies))]
#![deny(clippy::self_named_module_files)]

pub mod decision;
pub mod error;
pub mod observe;
#[cfg(any(test, feature = "test-util"))]
pub mod testing;

mod connect;
mod image_input;
mod provider;
mod request;
mod retry;
mod transport;
mod turn;
mod usage;

pub use self::connect::{Connect, Connector};
pub use self::error::{Error, Result};
pub use self::provider::Provider;
pub use self::retry::settings::{RetryPolicy, RetrySettings};
pub use af_inference::log_filter;
pub use af_inference::transport::REPLY_MAX_BYTES;
pub use af_wire::inference::InferenceApi as Wire;
pub use request::context_document;
