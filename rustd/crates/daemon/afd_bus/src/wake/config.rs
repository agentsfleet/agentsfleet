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

/// The size a queue takes when nothing names one, and the most it may be.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QueueBounds {
    /// Capacity when [`ENV_WAKE_QUEUE_CAPACITY`] is unset.
    pub default: usize,
    /// The largest capacity a deployment may name.
    pub max: usize,
}

impl Default for QueueBounds {
    fn default() -> Self {
        Self {
            default: DEFAULT_WAKE_QUEUE_CAPACITY,
            max: MAX_WAKE_QUEUE_CAPACITY,
        }
    }
}

#[derive(Validate)]
#[garde(context(usize as max))]
struct Capacity {
    #[garde(range(min = 1, max = *max))]
    value: usize,
}

pub(super) fn prove(value: usize, max: usize) -> Result<usize> {
    Ok(Unvalidated::new(Capacity { value })
        .validate_with(&max)?
        .value)
}

pub(super) fn resolve(env: &(impl EnvSource + ?Sized), bounds: QueueBounds) -> Result<usize> {
    let value = match env.get(ENV_WAKE_QUEUE_CAPACITY) {
        Some(raw) => raw.parse()?,
        None => bounds.default,
    };
    prove(value, bounds.max)
}
