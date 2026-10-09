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
/// `$1::uuid` because sqlx binds a `&str` as `text` and `core.fleets.id` is a
/// `UUID` column; the cast lets Postgres compare the two.
///
/// # `execution_id` is deliberately absent
///
/// Nothing on this path reads or clears an execution handle: the column has no
/// production writer of a value and no production reader, and what it tries to
/// express — which fleet is executing right now — is `fleet.runner_leases`,
/// which has the fence and the TTL that make the answer trustworthy. A handle
/// with no expiry can only go stale.
///
/// `$1` fleet.
pub const SELECT_FLEET_WITH_SESSION: &str = "\
SELECT f.workspace_id::text, f.config_json::text, f.source_markdown, f.status,
       f.bundle_content_hash, f.name, s.context_json::text
FROM core.fleets f
LEFT JOIN core.fleet_sessions s ON s.fleet_id = f.id
WHERE f.id = $1::uuid";

/// A fleet's stored config, whatever its status.
///
/// The renewal's ceiling read. No status filter, because a fleet an operator
/// stopped or killed mid-run still has a stored ceiling for its run to obey.
///
/// `$1` fleet.
pub const SELECT_FLEET_CONFIG: &str = "\
SELECT f.config_json::text
FROM core.fleets f
WHERE f.id = $1::uuid";
