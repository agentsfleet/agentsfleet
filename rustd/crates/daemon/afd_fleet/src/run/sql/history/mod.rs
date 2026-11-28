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

/// A resolved host card's asking turn, bound to the continuation's ledger identity.
pub(crate) const SELECT_HOST_ASKING_TURN: &str = include_str!("select_host_asking_turn.sql");

/// The ledger's authenticated question-answer producer key.
pub(crate) const SELECT_ANSWER_KEY: &str = include_str!("select_answer_key.sql");
