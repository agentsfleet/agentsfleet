//! The run plane's deployment settings: retries, scan sizes, cache sizes and
//! turn checkpoint lifetimes.
//!
//! Each default is the number the plane ran with before it was a setting.

use std::time::Duration;

/// The deployment's run-plane settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RunLimits {
    /// Runs one turn is issued before it fails.
    pub max_runs_per_turn: u32,
    /// Waiting turns one assignment pass considers.
    pub assign_batch_size: usize,
    /// Bytes of setup documents the definition cache weighs.
    pub definition_cache_bytes: u64,
    /// Fleets whose head revision the replica remembers.
    pub head_cache_count: u64,
    /// Model price books the replica holds.
    pub price_book_cache_count: u64,
    /// Turn checkpoint lifetimes.
    pub turn_checkpoints: TurnCheckpointLimits,
    /// Durable tasks' slices, operations and checkpoints.
    pub tasks: crate::task::policy::TaskTimes,
    /// The tool-window tiers a run's context resolves against.
    pub tool_windows: afd_gate::policy::context::ToolWindows,
}

impl Default for RunLimits {
    fn default() -> Self {
        Self {
            max_runs_per_turn: af_core::limits::DEFAULT_MAX_RUNS_PER_TURN,
            assign_batch_size: super::assign::MAX_CANDIDATES_PER_POLL,
            definition_cache_bytes: super::cache::DEFINITION_CAPACITY_BYTES,
            head_cache_count: super::cache::HEAD_CAPACITY,
            price_book_cache_count: super::cache::PRICE_BOOK_CAPACITY,
            turn_checkpoints: TurnCheckpointLimits::default(),
            tasks: crate::task::policy::TaskTimes::default(),
            tool_windows: afd_gate::policy::context::ToolWindows::default(),
        }
    }
}

impl RunLimits {
    /// The retry ceiling as the `earlier_runs` count statements return.
    pub(crate) fn runs_per_turn(&self) -> i64 {
        i64::from(self.max_runs_per_turn)
    }
}

/// How long a turn checkpoint and its transfers live.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TurnCheckpointLimits {
    /// A settled turn's checkpoint is deleted after this.
    pub retention: Duration,
    /// How long a runner's upload link stays valid.
    pub upload_url_ttl: Duration,
    /// A checkpoint being restored cannot be deleted during this window.
    pub restore_pin: Duration,
    /// An upload never committed is deletable after this.
    pub orphan_after: Duration,
}

impl Default for TurnCheckpointLimits {
    fn default() -> Self {
        Self {
            retention: super::turn_checkpoint::RETENTION,
            upload_url_ttl: super::turn_checkpoint::TRANSFER,
            restore_pin: super::turn_checkpoint::PIN,
            orphan_after: super::turn_checkpoint::ORPHAN,
        }
    }
}

impl TurnCheckpointLimits {
    pub(crate) fn retention_seconds(&self) -> i32 {
        seconds(self.retention)
    }

    pub(crate) fn transfer_seconds(&self) -> i32 {
        seconds(self.upload_url_ttl)
    }

    pub(crate) fn pin_seconds(&self) -> i32 {
        seconds(self.restore_pin)
    }

    pub(crate) fn orphan_seconds(&self) -> i32 {
        seconds(self.orphan_after)
    }
}

/// Whole seconds as the `int` interval statements bind, saturating.
pub(crate) fn seconds(duration: Duration) -> i32 {
    i32::try_from(duration.as_secs()).unwrap_or(i32::MAX)
}

#[cfg(test)]
mod tests {
    use super::{RunLimits, TurnCheckpointLimits};
    use std::time::Duration;

    #[test]
    fn defaults_are_the_numbers_the_plane_ran_with() {
        let limits = RunLimits::default();
        assert_eq!(limits.max_runs_per_turn, 3);
        assert_eq!(limits.assign_batch_size, 64);
        assert_eq!(limits.definition_cache_bytes, 64 * 1024 * 1024);
        assert_eq!(limits.head_cache_count, 16_384);
        assert_eq!(limits.price_book_cache_count, 1_024);
        let checkpoints = limits.turn_checkpoints;
        assert_eq!(checkpoints.retention_seconds(), 7 * 24 * 60 * 60);
        assert_eq!(checkpoints.transfer_seconds(), 15 * 60);
        assert_eq!(checkpoints.pin_seconds(), 2 * 60);
        assert_eq!(checkpoints.orphan_seconds(), 60 * 60);
    }

    #[test]
    fn a_configured_lifetime_is_what_a_statement_binds() {
        let checkpoints = TurnCheckpointLimits {
            retention: Duration::from_hours(24),
            upload_url_ttl: Duration::from_mins(5),
            restore_pin: Duration::from_mins(1),
            orphan_after: Duration::from_secs(u64::MAX),
        };
        assert_eq!(checkpoints.retention_seconds(), 86_400);
        assert_eq!(checkpoints.transfer_seconds(), 300);
        assert_eq!(checkpoints.pin_seconds(), 60);
        assert_eq!(checkpoints.orphan_seconds(), i32::MAX);
    }
}
