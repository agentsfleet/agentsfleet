//! The scoped parent lock is shared with setup edits; a row lock covers the tag.

/// Whether a live syncer holds the schedule, judged on `clock_timestamp()`
/// after the lock wait rather than at the transaction's start.
pub(super) const HELD: &str = include_str!("held.sql");

/// Replaces the intent and takes the sync fence in one write.
pub(super) const UPDATE: &str = include_str!("update.sql");
