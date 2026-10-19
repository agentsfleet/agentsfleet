//! The statements an operator's mutation runs.
//!
//! Split from [`super::runner`] by CALLER, the way [`super::sweep`] already is:
//! nothing here runs on a runner's own request path. Every one is reached from
//! [`crate::admin`] or [`crate::edit`], each carries the runner id in its
//! predicate, and each returns the prior state alongside its verdict, so the
//! service tells a harmless repeat from a refused attempt without a
//! read-then-write race.

/// Retire one runner's record, but only from the terminal state and only once
/// no lease of its is still active.
///
/// One statement rather than a SELECT then a DELETE, so an operator revoking
/// concurrently cannot slip between the check and the write. The row's
/// leases and events cascade and its slots clear, all declared in schema; the
/// daemon deletes nothing else by hand. A lease still active would take with
/// it the row the liveness sweep releases the fleet's slot through, so the
/// delete waits for the sweep or the lease's expiry. The single result row says
/// which of the three things happened, and its absence says there was no such
/// runner. `$1` runner, `$2` the revoked status, `$3` the active lease status.
pub const DELETE_RUNNER_IF_IN_STATE: &str = include_str!("delete_runner_if_in_state.sql");

/// Move one runner's status and append its audit event.
///
/// The locked row, update, and event live in one statement. The final select
/// returns the prior status even for an idempotent request, which lets the
/// service distinguish a harmless repeat from a forbidden attempt to move a
/// revoked runner without a read-then-write race. `$1` runner, `$2` target,
/// `$3` whether the target is revoked, `$4` the revoked status, `$5` event id,
/// `$6` event type, `$7`–`$9` the metadata keys, `$10` the actor.
pub const TRANSITION_RUNNER_STATUS: &str = include_str!("transition_runner_status.sql");

/// The policy mutation's locked status and reported capability.
pub const SELECT_RUNNER_PATCH_STATE: &str = include_str!("select_runner_patch_state.sql");

/// Re-assign policy, reconciled verdict, and audit event atomically.
///
/// The caller holds this row's lock in the surrounding transaction. The
/// distinctness guard makes an identical request write neither row nor event.
/// `$12` is the reconciled verdict's reason and `$13` the sandbox
/// configuration, rendered whole.
pub const PATCH_RUNNER_ASSIGNED_POLICY: &str = include_str!("patch_runner_assigned_policy.sql");

/// Record one outstanding self-test ask while preserving revocation terminality.
///
/// Returning the locked row even when the guard refuses the update lets the
/// caller distinguish a missing runner from a revoked one without a racy
/// follow-up read. `requested_at` is the stamp the update wrote, or NULL when
/// the guard refused it. `$1` runner, `$2` the revoked status.
pub const PATCH_RUNNER_SELFTEST_REQUEST: &str = include_str!("patch_runner_selftest_request.sql");

/// Replace one runner's labels, and no other column it carries.
///
/// Runs inside the edit's transaction, after [`SELECT_RUNNER_PATCH_STATE`]
/// has locked the row and proved it exists. The policy, verdict and status
/// columns are absent from the `SET` on purpose: a label edit can never move
/// the sandbox a host applies, and [`PATCH_RUNNER_ASSIGNED_POLICY`] names no
/// label column in turn. `updated_at` moves only when the set changed, so a
/// repeated edit leaves the row byte-identical.
pub const REPLACE_RUNNER_LABELS: &str = include_str!("replace_runner_labels.sql");
