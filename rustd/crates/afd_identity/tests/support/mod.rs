//! Shared scaffolding for this crate's test targets.
//!
//! Each test target compiles this module separately, so a helper only some
//! targets reach for reads as dead in the others. That is a property of how
//! Cargo builds integration tests, not unused code.
#![allow(
    dead_code,
    reason = "shared test support is compiled into every including target; not all of them use all of it"
)]

pub(crate) mod capability;
pub(crate) mod signing;

/// One millisecond, the step that puts a clock just past a window's edge.
pub(crate) const ONE_MS: std::time::Duration = std::time::Duration::from_millis(1);

/// Where every capability suite's clock starts.
///
/// Arbitrary, and identical across every window test on purpose: the
/// assertions are about elapsed time from a common origin, so one name is what
/// keeps them comparable.
pub(crate) const CLOCK_ORIGIN: jiff::Timestamp = jiff::Timestamp::constant(1_000, 0);
