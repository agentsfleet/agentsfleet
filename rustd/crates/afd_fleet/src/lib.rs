//! The runner control plane: what a host may do, and what it is owed.
//!
//! What lands first is the runner ROW — enrolment, liveness, and
//! the degraded verdict — because every other verb in the plane is gated on a
//! runner the authenticator has already proven and this crate has to be able to
//! describe.
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
//! In [`lease::sql`], beside the plane that runs it, per RULE SQLMOD. That
//! module says why each plane keeps its own rather than sharing one for the
//! crate.

// A dependency listed but unused is supply-chain surface and compile time for
// nothing. Gated on `not(test)` because the test build links dev-dependencies
// into this same target, where a test-only crate legitimately goes unused by
// the library's own code.
#![forbid(unsafe_code)]
#![cfg_attr(not(test), deny(unused_crate_dependencies))]

pub mod bundle;
pub mod error;
pub mod lease;

pub use crate::error::{Error, Result};
