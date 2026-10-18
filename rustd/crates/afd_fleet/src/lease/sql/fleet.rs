//! The claim read: one fleet's installed shape, as one statement.

/// The fleet row and its session checkpoint, in ONE statement.
///
/// The join is the whole point. The per-claim shape this replaces spent three
/// pool acquires on three single-row statements, on the path every lease takes.
///
/// `LEFT JOIN`, not `JOIN`: a fleet that has never checkpointed has no session
/// row, and an inner join would make a first-ever run unleasable. `context_json`
/// comes back NULL for that fleet and the caller substitutes its fresh-context
/// sentinel.
///
/// `$1::uuid` because sqlx binds a `&str` as `text` and `fleet.fleets.id` is a
/// `UUID` column; the cast lets Postgres compare the two.
///
/// Which fleet is executing right now is `runner.leases`, which has the
/// fence and the TTL that make the answer trustworthy; the session row is only
/// the resume bookmark.
///
/// `$1` fleet.
pub const SELECT_FLEET_WITH_SESSION: &str = include_str!("select_fleet_with_session.sql");

/// A fleet's stored config, whatever its status.
///
/// The renewal's ceiling read. No status filter, because a fleet an operator
/// stopped or killed mid-run still has a stored ceiling for its run to obey.
///
/// `$1` fleet.
pub const SELECT_FLEET_CONFIG: &str = include_str!("select_fleet_config.sql");
