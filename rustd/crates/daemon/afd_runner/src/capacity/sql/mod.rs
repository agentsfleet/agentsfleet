//! Statements share the caller's transaction; no second connection is taken.

pub(super) const CONTEXT: &str = "lock and count host run capacity";

/// The host row, locked until the caller's transaction ends. `$1` runner, `$2` and
/// `$3` a stream's credential digest and connection, `$4` the work's tenant or NULL
/// for none, `$5` whether the call is a stream's, which must prove `$2` and `$3`.
pub(super) const LOCK_HOST: &str = include_str!("lock_host.sql");

/// The same host proof, shared with other continuing runs.
pub(super) const SHARE_HOST: &str = include_str!("share_host.sql");

/// Live runs and applicable idle holds, counted at one clock read.
///
/// A separate statement from [`LOCK_HOST`], run after it in the same
/// transaction, so its `clock_timestamp()` is read once the host lock is held:
/// a run that expired while this assignment waited for the lock no longer
/// counts. `$1` runner, `$2` active status, `$3` the thread the assignment consumes
/// (a fleet's thread is keyed by the fleet's id) or NULL, `$4` whether this is a
/// new assignment that counts holds.
pub(super) const CENSUS: &str = include_str!("census.sql");

/// The host's run-type counter and the type it chose last.
pub(super) const CHOICE: &str = include_str!("choice.sql");

/// Advances the counter, only from the version the caller read. `$1` runner,
/// `$2` the version read, `$3` the type chosen.
pub(super) const RECORD_CHOICE: &str = include_str!("record_choice.sql");
