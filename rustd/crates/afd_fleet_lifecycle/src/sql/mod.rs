//! Every statement this crate runs, and nothing else.
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
//! `events_processed` and `budget_used_nanos` come from the one-to-one
//! `core.fleet_activity_counters` row migration-030's triggers maintain, never
//! from a subselect over `core.fleet_events`.
//!
//! A page reaches that row by PRIMARY-KEY lookup per fleet rather than by
//! joining, and the difference is measured: `LEFT JOIN` plans as a hash join
//! whose build side SEQUENTIALLY SCANS every counter row — the whole table
//! read to answer for one page of fifty. At 50k fleets and 14k counters that
//! is 1.69 ms against 0.21 ms, and the gap grows with the counters table
//! because the scan is O(counters) where the lookups are O(page). `LEFT JOIN
//! LATERAL` does not help; the planner flattens it back into the same join.
//!
//! An INNER JOIN is wrong in either shape: a fleet that has never run has no
//! counter row, and dropping those empties the list. On the development
//! database 126 of 174 fleets have no counter.

pub(crate) mod live_set;
pub(crate) mod purge;

/// The columns a list page reads, in the order [`crate::read`] indexes them.
///
/// A macro expanding to a LITERAL rather than a `const`, because `concat!`
/// takes literals only and the two page statements must not carry two
/// hand-kept copies of eight column expressions. A column added to one spelling
/// and not the other would not fail to compile — it would shift every field
/// after it and mis-read the row (RULE UFS).
macro_rules! page_columns {
    () => {
        "SELECT f.id::text, f.name, f.status, f.created_at, f.updated_at, \
                (f.config_json->'x-agentsfleet'->'triggers')::text, \
                COALESCE((SELECT c.events_processed FROM core.fleet_activity_counters c \
                           WHERE c.fleet_id = f.id), 0), \
                COALESCE((SELECT c.budget_used_nanos FROM core.fleet_activity_counters c \
                           WHERE c.fleet_id = f.id), 0) \
         FROM core.fleets f "
    };
}

/// The first page of a workspace's fleets, newest first.
///
/// `$1` workspace · `$2` how many rows to fetch. The caller asks for ONE more
/// row than the page it will serve, so whether a next page exists is a fact
/// about the walk rather than a guess from a full page.
pub(crate) const SELECT_FLEET_PAGE_FIRST: &str = concat!(
    page_columns!(),
    "WHERE f.workspace_id = $1::uuid \
     ORDER BY f.created_at DESC, f.id DESC \
     LIMIT $2"
);

/// A later page, resumed from the boundary the last one ended on.
///
/// `$1` workspace · `$2` boundary instant · `$3` boundary id · `$4` fetch count.
///
/// The tuple comparison is spelled out rather than written as a row constructor
/// because `f.id` is compared as TEXT: the cursor carries the identifier's
/// canonical spelling, and a `uuid` comparison would order by byte layout
/// instead — a different walk for two fleets minted in the same millisecond,
/// which is exactly the case the composite key exists to get right.
pub(crate) const SELECT_FLEET_PAGE_AFTER: &str = concat!(
    page_columns!(),
    "WHERE f.workspace_id = $1::uuid \
       AND (f.created_at < $2 OR (f.created_at = $2 AND f.id::text < $3)) \
     ORDER BY f.created_at DESC, f.id DESC \
     LIMIT $4"
);

/// The status alone, for the purge's pre-flight and the steer's ingress check.
///
/// `$1` fleet · `$2` workspace.
pub(crate) const SELECT_FLEET_STATUS: &str = "\
SELECT status FROM core.fleets \
WHERE id = $1::uuid AND workspace_id = $2::uuid \
LIMIT 1";

/// Deletes only from the expected status, reporting whether it happened.
///
/// `$1` fleet · `$2` workspace · `$3` the status the row must hold.
///
/// `RETURNING id` is what lets the caller tell "already gone" from "still
/// running, refused" without a second read, and it closes the window between
/// the pre-flight probe and this statement where a concurrent PATCH could have
/// brought the fleet back.
pub(crate) const DELETE_FLEET_IN_STATUS: &str = "\
DELETE FROM core.fleets \
WHERE id = $1::uuid AND workspace_id = $2::uuid AND status = $3 \
RETURNING id";
