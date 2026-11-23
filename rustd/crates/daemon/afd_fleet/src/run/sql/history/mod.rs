//! The statements a chat run's history is read with: a keyset page of the
//! fleet thread's finished turns, and the tool calls one turn's runs made.

/// One page of finished turns before a cursor, newest first.
///
/// `$1` workspace, `$2` fleet, `$3` and `$4` the cursor, `$5` and `$6` the
/// two finished statuses, `$7` the page size.
pub const SELECT_FINISHED_BEFORE: &str = include_str!("select_finished_before.sql");

/// The tool calls one turn holds, in the order made.
///
/// `$1` workspace, `$2` fleet, `$3` the turn.
pub const SELECT_CALLS_OF: &str = include_str!("select_calls_of.sql");
