//! Every statement this crate runs, and nothing else.
//!
//! Each is a `.sql` file beside this module, schema-qualified (RULE NSQ), and
//! checked against the rebuilt schema by `afd_db`'s
//! `test_every_sql_file_prepares`.
//!
//! # Every statement carries the workspace in its predicate
//!
//! Not one of them trusts the handler to have checked first. A fleet id
//! belonging to another workspace resolves NO ROW rather than the wrong row,
//! which is what makes the 404 in front of it honest: existence is never
//! disclosed, because nothing here can answer about a row the caller does not
//! own. The ownership LAYER is a capability question and this is the tenancy
//! boundary; the two are independent on purpose, and a fault in either alone
//! still leaves the other standing.
//!
//! # The counters are read per row, never re-aggregated
//!
//! `event_count` and `budget_used_nanos` come from the one-to-one
//! `fleet.activity_counters` row its triggers maintain, never from a subselect
//! over `fleet.events`.
//!
//! A page reaches that row by PRIMARY-KEY lookup per fleet rather than by
//! joining, and the difference is measured: `LEFT JOIN` plans as a hash join
//! whose build side SEQUENTIALLY SCANS every counter row — the whole table
//! read to answer for one page of fifty. At 50k fleets and 14k counters that
//! is 1.69 ms against 0.21 ms, and the gap grows with the counters table
//! because the scan is O(counters) where the lookups are O(page). `LEFT JOIN
//! LATERAL` does not help; the planner flattens it back into the same join.
//! The head revision's triggers are reached the same way, for the same reason.
//!
//! An INNER JOIN is wrong in either shape: a fleet that has never run has no
//! counter row, and dropping those empties the list.

/// One page of a workspace's fleets, newest first.
///
/// `$1` workspace · `$2` how many rows to fetch · `$3` and `$4` the boundary
/// the last page ended on, both NULL for the first page. The caller asks for
/// ONE more row than the page it will serve, so whether a next page exists is
/// a fact about the walk rather than a guess from a full page.
///
/// One statement for every page: an absent boundary falls back to `infinity`,
/// which every `created_at` sorts before, so the first page and a resumed one
/// both seek `idx_fleets_workspace_id_created_at_id` on the same row
/// comparison, and the column list has one spelling.
pub(crate) const SELECT_FLEET_PAGE: &str = include_str!("select_fleet_page.sql");

/// The status alone, for the steer's ingress check.
///
/// `$1` fleet · `$2` workspace.
pub(crate) const SELECT_FLEET_STATUS: &str = include_str!("select_fleet_status.sql");

/// Locks the fleet row before its conditional purge, reading the status and
/// resource version the tag check compares against.
///
/// `$1` fleet · `$2` workspace.
pub(crate) const LOCK_FLEET: &str = include_str!("lock_fleet.sql");

/// Deletes only from the expected status, reporting whether it happened.
///
/// `$1` fleet · `$2` workspace · `$3` the status the row must hold.
///
/// `RETURNING id::text` is what lets the caller tell "already gone" from "still
/// running, refused" without a second read. Every child row cascades from
/// this one (deletion matrix: Fleet), approval gates included: their table
/// grants no DELETE, and the cascade runs as the table owner. Runner leases are
/// the exception: [`DELETE_ACTIVE_LEASES`] runs first, and settled ones stay.
pub(crate) const DELETE_FLEET_IN_STATUS: &str = include_str!("delete_fleet_in_status.sql");

/// Deletes one fleet's live leases inside its purge.
///
/// `$1` fleet · `$2` the live lease status. `runner.leases` holds no foreign key
/// to the fleet, so a settled lease outlives the purge for its own retention
/// window; a live one would name a fleet that no longer exists, and goes here.
pub(crate) const DELETE_ACTIVE_LEASES: &str = include_str!("delete_active_leases.sql");

/// Every fleet identifier in one workspace.
///
/// `$1` workspace. No ORDER BY: the caller collects into a `BTreeSet`, which
/// is what makes the order stable, and asking Postgres to sort a set the caller
/// re-sorts anyway would be paying twice.
pub(crate) const SELECT_FLEET_IDS: &str = include_str!("select_fleet_ids.sql");

/// The activity counters of every fleet in a set, one row per fleet.
///
/// `$1` workspace · `$2` the fleet identifiers, as text. Driven from
/// `fleet.fleets` rather than from the counters table so a fleet that has never
/// run — and so has no counter row — still answers, with the zeros the page
/// shows for it. The workspace rides the predicate like every other statement
/// here, even though the one caller hands over a set it enumerated for that
/// workspace: an identifier from another workspace answers NO ROW rather than
/// its spend.
pub(crate) const SELECT_FLEET_COUNTERS_FOR_SET: &str =
    include_str!("select_fleet_counters_for_set.sql");
