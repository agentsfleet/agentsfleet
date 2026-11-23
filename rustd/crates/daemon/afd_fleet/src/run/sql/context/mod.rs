//! The statement a run's context read checks its holder with; the read and
//! the fenced save are `afd_setup::context`'s.

/// A run's thread owner, fence and status, if `runner` holds it.
///
/// `$1` run, `$2` runner. No row for another runner's run.
pub const SELECT_RUN_THREAD: &str = include_str!("select_run_thread.sql");
