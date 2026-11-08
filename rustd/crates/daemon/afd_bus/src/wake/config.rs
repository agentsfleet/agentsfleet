//! Development and production use the same validated capacity setting.

use af_core::env::EnvSource;
use garde::{Unvalidated, Validate};

use crate::Result;

/// Per-daemon capacity; independent of runner concurrency and durable backlog.
pub const HINT_CAPACITY_ENV: &str = "RUNNER_STREAM_HINT_CAPACITY";
/// Default scheduling look-ahead, rather than an admission or runner limit.
pub const DEFAULT_HINT_CAPACITY: usize = 64;
/// Keeps a mistaken deployment setting from allocating an unbounded queue.
pub const MAX_HINT_CAPACITY: usize = 65_536;

#[derive(Validate)]
struct Capacity {
    #[garde(range(min = 1, max = MAX_HINT_CAPACITY))]
    value: usize,
}

pub(super) fn prove(value: usize) -> Result<usize> {
    Ok(Unvalidated::new(Capacity { value }).validate()?.value)
}

pub(super) fn resolve(env: &(impl EnvSource + ?Sized)) -> Result<usize> {
    let value = match env.get(HINT_CAPACITY_ENV) {
        Some(raw) => raw.parse()?,
        None => DEFAULT_HINT_CAPACITY,
    };
    prove(value)
}
