//! The claim read: one fleet's installed shape, as one statement.
//!
//! Copied from `fleet/sql.zig`'s `SELECT_FLEET_WITH_SESSION`.

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
/// `$1::uuid` where the Zig binds a bare `$1`: the Zig driver sends an untyped
/// parameter and lets Postgres infer it, while sqlx binds a `&str` as `text`
/// and `core.fleets.id` is a `UUID` column. The cast is the same accommodation
/// [`super::gate`]'s statements carry, and the only difference from the
/// original text.
///
/// # `execution_id` is deliberately absent
///
/// The Zig reads and clears an execution handle on this path. It is not ported
/// and neither is its `CLEAR_STALE_EXECUTION` companion: the column has no
/// production writer of a value and no production reader, and what it tries to
/// express — which fleet is executing right now — is `fleet.runner_leases`,
/// which has the fence and the TTL that make the answer trustworthy. A handle
/// with no expiry can only go stale, which is why it needed crash recovery at
/// all.
///
/// `$1` fleet.
pub const SELECT_FLEET_WITH_SESSION: &str = "\
SELECT f.workspace_id::text, f.config_json::text, f.source_markdown, f.status,
       f.bundle_content_hash, f.name, s.context_json::text
FROM core.fleets f
LEFT JOIN core.fleet_sessions s ON s.fleet_id = f.id
WHERE f.id = $1::uuid";
