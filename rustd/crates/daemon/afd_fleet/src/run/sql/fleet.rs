//! The assignment read: one fleet's installed shape, as one statement.

/// The fleet row and its session checkpoint, in ONE statement.
///
/// The join is the whole point. The per-assignment shape this replaces spent three
/// pool acquires on three single-row statements, on the path every run takes.
///
/// `LEFT JOIN`, not `JOIN`: a fleet that has never checkpointed has no session
/// row, and an inner join would make a first-ever run unleasable. `context_json`
/// comes back NULL for that fleet and the caller substitutes its fresh-context
/// sentinel.
///
/// Which fleet is executing right now is `thread.runs`, which has the
/// fence and the TTL that make the answer trustworthy; the session row is only
/// the resume bookmark.
///
/// `$1` fleet.
pub const SELECT_FLEET_WITH_THREAD: &str = include_str!("select_fleet_with_thread.sql");
