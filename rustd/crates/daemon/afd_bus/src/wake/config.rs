//! Development and production use the same validated capacity setting.

use af_core::env::EnvSource;
use garde::{Unvalidated, Validate};

use crate::Result;

/// Per-daemon capacity; independent of runner concurrency and durable backlog.
pub const ENV_WAKE_QUEUE_CAPACITY: &str = "AGENTSFLEET_RUNNER_WAKE_QUEUE_CAPACITY";
/// Default scheduling look-ahead, rather than an admission or runner limit.
pub const DEFAULT_WAKE_QUEUE_CAPACITY: usize = 64;
/// Keeps a mistaken deployment setting from allocating an unbounded queue.
pub const MAX_WAKE_QUEUE_CAPACITY: usize = 65_536;

#[derive(Validate)]
struct Capacity {
    #[garde(range(min = 1, max = MAX_WAKE_QUEUE_CAPACITY))]
    value: usize,
}

pub(super) fn prove(value: usize) -> Result<usize> {
    Ok(Unvalidated::new(Capacity { value }).validate()?.value)
}

pub(super) fn resolve(env: &(impl EnvSource + ?Sized)) -> Result<usize> {
    let value = match env.get(ENV_WAKE_QUEUE_CAPACITY) {
        Some(raw) => raw.parse()?,
        None => DEFAULT_WAKE_QUEUE_CAPACITY,
    };
    prove(value)
}
