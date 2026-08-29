//! Every statement this crate runs, schema-qualified and named (RULE NSQ).
//!
//! # The column list is one macro, and it is no longer load-bearing
//!
//! Nine statements return a schedule row. They share one list so a column added
//! to the table is added in one place — but the decoder reads BY NAME
//! (`store::decode`), so the order in this file is no longer something
//! correctness rests on. It used to be, and that was the wrong design: a
//! positional read made two transposed `TEXT` columns a silent mis-read rather
//! than a build failure.
//!
//! # Why the claim and the finalize are separate statements
//!
//! `520_fleet_schedules.sql` gives this table a `generation`, a `sync_token`
//! and a `sync_lease_until` so one syncer at a time can push a schedule. The
//! claim takes the fence and bumps the generation; the finalize releases it and
//! is conditioned on BOTH the generation and the token it was given. A syncer
//! whose lease expired and whose row was taken by another finalizes nothing —
//! its `WHERE` matches no row — rather than overwriting the newer state.

/// Every column of a schedule row, in the order the decoder reads them.
///
/// A macro expanding to a LITERAL rather than a `const`, because `concat!`
/// takes literals only — the shape `afd_fleet_lifecycle::sql` already uses next
/// door. What it buys is one edit site for fifteen columns (RULE UFS); what it
/// no longer has to buy is a stable ORDER, because the decoder reads by name.
macro_rules! row_columns {
    () => {
        "id::text, fleet_id::text, source, source_key, cron_expression, \
         timezone, message, desired_status, sync_status, generation, \
         sync_token::text, sync_lease_until, last_error, created_at, updated_at"
    };
}

/// The column list with its `SELECT`, for the statements that read rather than
/// return.
///
/// A second macro rather than a second spelling of the keyword: `RETURNING`
/// takes the same list without it, so one of the two forms has to be composed
/// and composing it here keeps `SELECT ` at exactly one site (RULE UFS).
macro_rules! select_row {
    () => {
        concat!("SELECT ", row_columns!())
    };
}

/// Whether this fleet belongs to the workspace the caller was proven in.
///
/// Asked before anything else on every route, because the schedules surface is
/// workspace-scoped and the fleet id arrives in the path. A statement that
/// skipped it would let a person holding one workspace's capability edit
/// another workspace's schedules by guessing a fleet id.
pub const FLEET_IN_WORKSPACE: &str = "SELECT 1::bigint FROM core.fleets \
     WHERE id = $1::uuid AND workspace_id = $2::uuid LIMIT 1";

/// Takes the fleet's row for the length of a create.
///
/// `FOR UPDATE` on the FLEET rather than on the schedules, because what is
/// being protected is a count over rows that do not exist yet: two concurrent
/// creates both counting 31 would both insert and leave 33. Locking the parent
/// is how a count-then-insert becomes atomic.
pub const LOCK_FLEET: &str = "SELECT f.id::text FROM core.fleets f \
     WHERE f.id = $1::uuid FOR UPDATE OF f";

/// How many schedules this fleet already holds.
pub const COUNT_FOR_FLEET: &str =
    "SELECT COUNT(*) FROM core.fleet_schedules WHERE fleet_id = $1::uuid";

/// Whether this fleet already registered that upstream key.
pub const SOURCE_KEY_EXISTS: &str = "SELECT 1::bigint FROM core.fleet_schedules \
     WHERE fleet_id = $1::uuid AND source_key = $2 LIMIT 1";

/// One schedule of this fleet's.
///
/// Bound on BOTH the schedule and the fleet, so a schedule id belonging to
/// another fleet reads as absent rather than as someone else's row.
pub const SELECT_ONE: &str = concat!(
    select_row!(),
    " FROM core.fleet_schedules WHERE id = $1::uuid AND fleet_id = $2::uuid"
);

/// This fleet's schedules, oldest first.
///
/// Ordered by `created_at` then `id`: the timestamp alone is not a total order
/// — two schedules created in the same millisecond would page in whichever
/// order the planner chose, and a caller walking pages would see one twice and
/// another never.
pub const LIST_FOR_FLEET: &str = concat!(
    select_row!(),
    " FROM core.fleet_schedules WHERE fleet_id = $1::uuid ORDER BY created_at, id"
);

/// Writes a new schedule, already claimed by the syncer that will register it.
pub const INSERT: &str = concat!(
    "INSERT INTO core.fleet_schedules \
     (id, fleet_id, source, source_key, cron_expression, timezone, message, \
     desired_status, sync_status, generation, sync_token, sync_lease_until, \
     last_error, created_at, updated_at) \
     VALUES ($1::uuid, $2::uuid, $3, $4, $5, $6, $7, $8, $9, $10, \
     $11::uuid, $12, NULL, $13, $13) RETURNING ",
    row_columns!()
);

/// Applies a change and takes the fence in one statement.
///
/// Each `COALESCE` is a field the caller may leave alone, which is what makes a
/// partial edit one round trip rather than a read followed by a write that
/// could race the read. The trailing predicate is the fence: an unheld row, or
/// one whose lease has run out, may be claimed; one another syncer holds may
/// not, and the statement matches nothing rather than stealing it.
pub const CLAIM_MUTATION: &str = concat!(
    "UPDATE core.fleet_schedules SET cron_expression = COALESCE($3, cron_expression), \
     timezone = COALESCE($4, timezone), message = COALESCE($5, message), \
     desired_status = COALESCE($6, desired_status), sync_status = $7, \
     generation = generation + 1, sync_token = $8::uuid, \
     sync_lease_until = $9, last_error = NULL, updated_at = $10 \
     WHERE id = $1::uuid AND fleet_id = $2::uuid AND \
     (sync_token IS NULL OR sync_lease_until IS NULL OR sync_lease_until <= $10) RETURNING ",
    row_columns!()
);

/// Takes the fence over a row's CURRENT state, changing nothing.
///
/// What `:sync` runs. A reconcile is not an edit — it pushes what the row
/// already says — so this claims without a `COALESCE` in sight.
pub const CLAIM_CURRENT: &str = concat!(
    "UPDATE core.fleet_schedules SET sync_status = $3, \
     generation = generation + 1, sync_token = $4::uuid, \
     sync_lease_until = $5, last_error = NULL, updated_at = $6 \
     WHERE id = $1::uuid AND fleet_id = $2::uuid AND \
     (sync_token IS NULL OR sync_lease_until IS NULL OR sync_lease_until <= $6) RETURNING ",
    row_columns!()
);

/// Releases a fence a push succeeded under.
pub const FINALIZE_SUCCESS: &str = concat!(
    "UPDATE core.fleet_schedules SET sync_status = $4, sync_token = NULL, \
     sync_lease_until = NULL, last_error = NULL, updated_at = $5 \
     WHERE id = $1::uuid AND generation = $2 AND sync_token = $3::uuid RETURNING ",
    row_columns!()
);

/// Releases a fence a push failed under, keeping WHY on the row.
///
/// The error is stored rather than only logged because it is what the next
/// `:sync` and the operator reading the list both need — a schedule that is
/// silently `failed` with no reason is one nobody can act on.
pub const FINALIZE_FAILURE: &str = concat!(
    "UPDATE core.fleet_schedules SET sync_status = $4, sync_token = NULL, \
     sync_lease_until = NULL, last_error = $5, updated_at = $6 \
     WHERE id = $1::uuid AND generation = $2 AND sync_token = $3::uuid RETURNING ",
    row_columns!()
);

/// Removes a row whose upstream schedule is confirmed gone.
///
/// Conditioned on the same fence a finalize is, and it runs only after the
/// external scheduler has agreed — see [`crate::model::DesiredStatus::Deleting`]
/// on why the row cannot go first.
pub const DELETE_CLAIMED: &str = "DELETE FROM core.fleet_schedules \
     WHERE id = $1::uuid AND generation = $2 AND sync_token = $3::uuid RETURNING id::text";

/// What a signed fire resolves to.
///
/// Reads the fleet's own status alongside the schedule's, because both can
/// refuse the fire and for different reasons: a paused SCHEDULE is the external
/// scheduler not yet knowing it was paused, and a paused FLEET is an operator
/// who stopped the whole thing.
pub const FIRE_TARGET: &str = "SELECT s.fleet_id::text, f.workspace_id::text, s.message, \
     s.desired_status, f.status \
     FROM core.fleet_schedules s JOIN core.fleets f ON f.id = s.fleet_id \
     WHERE s.id = $1::uuid";
