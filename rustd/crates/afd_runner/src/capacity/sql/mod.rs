//! Statements share the caller's transaction; no second connection is taken.

pub(super) const CONTEXT: &str = "lock and count host lease capacity";

/// The host row, locked until the caller's transaction ends.
pub(super) const LOCK_HOST: &str = include_str!("lock_host.sql");

/// Both lease kinds the host holds, counted at one clock read.
///
/// A separate statement from [`LOCK_HOST`], run after it in the same
/// transaction, so its `clock_timestamp()` is read once the host lock is held:
/// a lease that expired while this claimant waited for the lock no longer
/// counts. `$1` runner, `$2` the active status both tables share.
pub(super) const CENSUS: &str = include_str!("census.sql");

/// The host's lease-type counter and the type it chose last.
pub(super) const CHOICE: &str = include_str!("choice.sql");

/// Advances the counter, only from the version the caller read. `$1` runner,
/// `$2` the version read, `$3` the type chosen.
pub(super) const RECORD_CHOICE: &str = include_str!("record_choice.sql");
