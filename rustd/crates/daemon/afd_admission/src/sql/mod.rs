//! Statements owned by the admissions ledger; explicit parameters at each caller.

/// Locks the fleet, answers a repeat or admits, in one statement.
pub(crate) const ADMIT_ONE: &str = include_str!("admit_one.sql");
/// What a steer's disposition is named from, read after its admission.
pub(crate) const RUN_FACTS: &str = include_str!("run_facts.sql");
pub(crate) const SELECT_BACKLOG: &str = include_str!("select_backlog.sql");
pub(crate) const SELECT_REPEATED: &str = include_str!("select_repeated.sql");
/// The report reads the admitted reply destination on its existing transaction.
pub const SELECT_REPLY_DESTINATION: &str = include_str!("select_reply_destination.sql");
