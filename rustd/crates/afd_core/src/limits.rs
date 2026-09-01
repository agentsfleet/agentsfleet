//! Bounded numbers policy is expressed in, with their clamps.
//!
//! Ported from `src/lib/contract/protocol_policy.zig`, where the worker-pool
//! bounds are single-sourced so "a fat-fingered value can never fork unbounded
//! children on one host". The clamp is applied on BOTH sides — the control
//! plane at assignment, the host at apply — which only works if both sides read
//! the same numbers, so they get a type rather than a pair of loose constants.

use serde::{Deserialize, Serialize};

use crate::error::{Error, ErrorKind, Result};

/// Workers a runner starts when the control plane assigns nothing else.
pub const DEFAULT_WORKERS: u32 = 1;

/// Fewest workers an assignment may carry; below this a runner does no work.
pub const MIN_WORKERS: u32 = 1;

/// Most workers an assignment may carry, bounding concurrent children per host.
pub const MAX_WORKERS: u32 = 64;

/// How many workers a runner is assigned, guaranteed within `MIN..=MAX`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(transparent)]
pub struct WorkerCount(u32);

impl WorkerCount {
    /// Builds a worker count, rejecting a value outside `MIN_WORKERS..=MAX_WORKERS`.
    ///
    /// Use this where a caller can be told it is wrong — an operator-facing
    /// form, a configuration read. On the assignment path use
    /// [`WorkerCount::clamping`], which is what the Zig daemon does and
    /// therefore what parity requires.
    ///
    /// # Errors
    /// Returns an out-of-range error naming the bound and the offending value.
    pub fn new(workers: u32) -> Result<Self> {
        if (MIN_WORKERS..=MAX_WORKERS).contains(&workers) {
            Ok(Self(workers))
        } else {
            Err(Error::from(ErrorKind::OutOfRange {
                name: "worker_count",
                value: workers,
                min: MIN_WORKERS,
                max: MAX_WORKERS,
            }))
        }
    }

    /// Builds a worker count by clamping into range, never failing.
    ///
    /// This is the assignment-path behaviour: `RegisterResponse` echoes the
    /// assignment "as stored (`worker_count` clamped into the shared bounds)", so
    /// an out-of-range request is corrected and reflected back rather than
    /// refused. Rejecting here instead would be a behaviour change wearing a
    /// stricter type.
    #[must_use]
    pub const fn clamping(workers: u32) -> Self {
        Self(if workers < MIN_WORKERS {
            MIN_WORKERS
        } else if workers > MAX_WORKERS {
            MAX_WORKERS
        } else {
            workers
        })
    }

    /// The count as a plain number.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

impl Default for WorkerCount {
    fn default() -> Self {
        Self(DEFAULT_WORKERS)
    }
}

impl<'de> Deserialize<'de> for WorkerCount {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        // Clamping, not rejecting: this is the wire path, and the Zig daemon
        // clamps a stored or transmitted value rather than refusing the runner.
        Ok(Self::clamping(u32::deserialize(deserializer)?))
    }
}
