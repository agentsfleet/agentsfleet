//! Every statement this crate runs, schema-qualified and named (RULE NSQ).
//!
//! Each statement is a whole `.sql` file beside this module, loaded with
//! `include_str!`, so what runs is what a reader opens and what `PREPARE`
//! checks.
//!
//! # The row's column list is spelled in each statement
//!
//! Nine statements return a schedule row. Each spells the full column list,
//! and the decoder reads BY NAME (`store::decode`), so the order is not
//! load-bearing; [`tests`] keeps the nine lists identical, which is what the
//! shared macro used to buy.
//!
//! # Why the claim and the finalize are separate statements
//!
//! `schema/1750_fleet_schedules.sql` gives this table a `resource_version`, a
//! `sync_token` and a `sync_lease_expires_at` so one syncer at a time can push
//! a schedule. The claim takes the fence and bumps the version; the finalize
//! releases it and is conditioned on BOTH the version and the token it was
//! given. A syncer whose lease expired and whose row was taken by another
//! finalizes nothing — its `WHERE` matches no row — rather than overwriting
//! the newer state.
//!
//! # Whose clock
//!
//! Postgres stamps every row: `created_at` and `updated_at` are `now()`. A
//! lease is set and judged on `clock_timestamp()`, the instant the statement
//! actually runs, because a claim may have waited on the row lock and a lease
//! that ran out during that wait has run out.

/// Whether this fleet belongs to the workspace the caller was proven in.
///
/// Asked before anything else on every route, because the schedules surface is
/// workspace-scoped and the fleet id arrives in the path. A statement that
/// skipped it would let a person holding one workspace's capability edit
/// another workspace's schedules by guessing a fleet id. A create asks it on
/// its own transaction; every other route asks it through
/// `Schedules::holds_fleet` before the schedule is read or claimed.
pub const FLEET_IN_WORKSPACE: &str = include_str!("fleet_in_workspace.sql");

/// Takes the fleet's row for the length of a create.
///
/// `FOR UPDATE` on the FLEET rather than on the schedules, because what is
/// being protected is a count over rows that do not exist yet: two concurrent
/// creates both counting 31 would both insert and leave 33. Locking the parent
/// is how a count-then-insert becomes atomic.
pub const LOCK_FLEET: &str = include_str!("lock_fleet.sql");

/// Takes the fleet's row, scoped to the caller's workspace, for an edit or a
/// setup's recipe replacement.
///
/// The same parent lock both take, so a recipe replacement and an edit of one
/// of its schedules serialize; no row means a fleet outside the workspace.
pub const LOCK_FLEET_IN_WORKSPACE: &str = include_str!("lock_fleet_in_workspace.sql");

/// How many schedules this fleet already holds.
pub const COUNT_FOR_FLEET: &str = include_str!("count_for_fleet.sql");

/// How many schedules of one source this fleet holds.
///
/// Read under the same fleet lock as [`COUNT_FOR_FLEET`], for a create whose
/// source carries a cap of its own: a fleet's own schedules are bounded below
/// the fleet's whole complement.
pub const COUNT_FOR_SOURCE: &str = include_str!("count_for_source.sql");

/// How many of this fleet's schedules no setup recipe owns.
///
/// What a recipe replacement adds its new count to before it is admitted.
pub const COUNT_INDEPENDENT: &str = include_str!("count_independent.sql");

/// Whether this fleet already registered that upstream id.
pub const PROVIDER_SCHEDULE_ID_EXISTS: &str = include_str!("provider_schedule_id_exists.sql");

/// One schedule of this fleet's.
///
/// Bound on BOTH the schedule and the fleet, so a schedule id belonging to
/// another fleet reads as absent rather than as someone else's row.
pub const SELECT_ONE: &str = include_str!("select_one.sql");

/// Locks one scoped schedule while checking its conditional edit.
pub(crate) const LOCK_ONE: &str = include_str!("lock_one.sql");

/// This fleet's schedules, oldest first.
///
/// Ordered by `created_at` then `id`: the instant alone is not a total order
/// — two schedules created in the same millisecond would page in whichever
/// order the planner chose, and a caller walking pages would see one twice and
/// another never.
pub const LIST_FOR_FLEET: &str = include_str!("list_for_fleet.sql");

/// Writes a new schedule, already claimed by the syncer that will register it.
///
/// No `fire_at`: a `once` schedule's instant is computed from the row's own
/// `created_at` and written by [`SET_FIRE_AT`] on the same transaction.
pub const INSERT: &str = include_str!("insert.sql");

/// Applies a change and takes the fence in one statement.
///
/// Each `COALESCE` is a field the caller may leave alone, which is what makes a
/// partial edit one round trip rather than a read followed by a write that
/// could race the read. The trailing predicate is the fence: an unheld row, or
/// one whose lease has run out, may be claimed; one another syncer holds may
/// not, and the statement matches nothing rather than stealing it.
pub const CLAIM_MUTATION: &str = include_str!("claim_mutation.sql");

/// Sets the instant a claimed `once` schedule's expression is for.
///
/// Its own statement, after the write that stamped the row: the instant is
/// computed from that stamp and from the expression and zone as the write left
/// them. Conditioned on the claim's token, so only its holder writes it.
pub const SET_FIRE_AT: &str = include_str!("set_fire_at.sql");

/// Takes the fence over a row's CURRENT state, changing nothing.
///
/// What `:sync` runs. A reconcile is not an edit — it pushes what the row
/// already says — so this claims without a `COALESCE` in sight.
pub const CLAIM_CURRENT: &str = include_str!("claim_current.sql");

/// Releases a fence a push succeeded under, recording the id it registered.
///
/// `provider_schedule_id = COALESCE($5, provider_schedule_id)` and not a plain
/// assignment: only the ACTIVE push registers an id, while pausing removes
/// upstream and registers nothing, and a pause must not blank the id its own
/// later delete names. `NULL` there means "this push had nothing to record".
///
/// Recording it replaces the placeholder a create stores before anything was
/// pushed with the derived id the scheduler filed the schedule under — see
/// `crate::upstream`, and `qstash::QStash::upsert` on refusing an answer that
/// names any other id.
pub const FINALIZE_SUCCESS: &str = include_str!("finalize_success.sql");

/// Releases a fence a push failed under, keeping WHY on the row.
///
/// The error is stored rather than only logged because it is what the next
/// `:sync` and the operator reading the list both need — a schedule that is
/// silently `failed` with no reason is one nobody can act on.
pub const FINALIZE_FAILURE: &str = include_str!("finalize_failure.sql");

/// Removes a row whose upstream schedule is confirmed gone.
///
/// Conditioned on the same fence a finalize is, and it runs only after the
/// external scheduler has agreed — see [`crate::model::DesiredStatus::Deleting`]
/// on why the row cannot go first.
pub const DELETE_CLAIMED: &str = include_str!("delete_claimed.sql");

/// Whether a fence is still the row's: the token and the version its claim set.
///
/// Read just before the external scheduler is called — see
/// `crate::service::Schedules::reconcile` on the stale syncer it turns away.
pub const HOLDS_FENCE: &str = include_str!("holds_fence.sql");

/// What a signed fire resolves to.
///
/// Reads the fleet's own status alongside the schedule's, because both can
/// refuse the fire and for different reasons: a paused SCHEDULE is the external
/// scheduler not yet knowing it was paused, and a paused FLEET is an operator
/// who stopped the whole thing.
pub const FIRE_TARGET: &str = include_str!("fire_target.sql");

#[cfg(test)]
mod tests;
