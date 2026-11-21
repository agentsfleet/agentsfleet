//! The run plane: assigning, renewing and settling a fleet's runs, and its
//! durable tasks.
//!
//! The host's own plane (enrolment, liveness, the degraded verdict) is
//! `afd_runner`'s; every verb here is gated on a runner it has already proven.
//!
//! # Deciding, apart from answering
//!
//! The seam between "decide" and "answer over HTTP" is the crate boundary.
//! Nothing in `afd_fleet` names axum, a status code, or a response body; every
//! operation answers a value or [`Error`], and `afd_api` decides what that
//! becomes on the wire. A handler that reached into a pool and wrote its own
//! response could swallow a Postgres failure into the same no-work answer an
//! idle fleet gets. Here a transient failure still answers no-work with a
//! backoff hint, but that is decided once, where it can be read.
//!
//! # Where the SQL lives
//!
//! In [`run::sql`], beside the plane that runs it, per RULE SQLMOD. That
//! module says why each plane keeps its own rather than sharing one for the
//! crate.

// A dependency listed but unused is supply-chain surface and compile time for
// nothing. Gated on `not(test)` because the test build links dev-dependencies
// into this same target, where a test-only crate legitimately goes unused by
// the library's own code.
#![forbid(unsafe_code)]
#![cfg_attr(not(test), deny(unused_crate_dependencies))]
#![deny(clippy::self_named_module_files)]

pub mod error;
pub mod run;
pub mod task;
#[cfg(feature = "test-util")]
pub mod test_util;

pub use crate::error::{Error, Result};
