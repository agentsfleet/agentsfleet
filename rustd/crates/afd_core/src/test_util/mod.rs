//! Test support every crate's suites share (M-TEST-UTIL).
//!
//! Behind `test-util`, so a production build links none of it.

pub mod error_surface;
pub mod trace;
