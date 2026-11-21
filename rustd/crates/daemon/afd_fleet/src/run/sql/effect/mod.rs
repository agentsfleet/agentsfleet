//! The statements a runner effect lands once with: the lock a call's retries
//! share, its item's stored answer, the item written with it, and the calls a
//! dead run of the turn finished.
//!
//! Each names `thread.items` (`afd_db/schema/20261108125110_thread_items.sql`).

/// Holds one run's call until the transaction ends.
///
/// `$1` run, `$2` the call's number.
pub const LOCK_ITEM: &str = include_str!("lock_item.sql");

/// The answer a call's effect stored on its item.
///
/// `$1` run, `$2` the call's number.
pub const SELECT_ITEM_OUTPUT: &str = include_str!("select_item_output.sql");

/// Writes a call's item with its effect's answer.
///
/// `$1` id, `$2` workspace, `$3` fleet, `$4` run, `$5` turn, `$6` fence,
/// `$7` the call's number, `$8` kind, `$9` tool, `$10` status, `$11`
/// arguments as JSON text, `$12` output, `$13` its line count.
pub const INSERT_ITEM: &str = include_str!("insert_item.sql");

/// The calls other runs of a turn finished, oldest first.
///
/// `$1` fleet, `$2` turn, `$3` the run taking it over.
pub const SELECT_FINISHED_CALLS: &str = include_str!("select_finished_calls.sql");
