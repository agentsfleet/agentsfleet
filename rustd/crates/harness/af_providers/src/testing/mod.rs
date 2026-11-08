//! What a suite needs to prove a provider call, in this crate and in the
//! crates that embed it (M-TEST-UTIL): an observer it reads back, a scripted
//! model, and a TLS listener a fake provider serves on.

pub mod observed;
#[cfg(feature = "test-util")]
pub mod scripted;
#[cfg(feature = "test-util")]
pub mod tls;

pub use self::observed::{Observed, Scoped, Tally, scoped};
