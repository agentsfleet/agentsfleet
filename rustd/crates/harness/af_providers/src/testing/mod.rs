//! What a suite needs to prove a provider call, in this crate and in the
//! crates that embed it (M-TEST-UTIL).
//!
//! An observer it reads back, a scripted model and the measure every
//! scripted model answers with. The TLS listener a fake provider serves on is
//! guffbolt-ai's `testing::tls`.

pub mod observed;
#[cfg(feature = "test-util")]
pub mod scripted;

pub use self::observed::{Observed, Scoped, Tally, scoped};

use guffbolt::Request;

/// The input tokens `request` costs in the canonical chat shape: what a
/// scripted model measures with, where a connected provider measures on its
/// own wire.
///
/// # Errors
/// The request cannot be encoded.
pub fn measure(request: Request<'_>) -> crate::Result<u64> {
    guffbolt_ai::wire::estimate(request).map_err(crate::error::raise::wire)
}
