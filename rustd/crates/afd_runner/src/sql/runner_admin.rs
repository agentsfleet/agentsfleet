//! The statements an operator's mutation runs.
//!
//! Split from [`super::runner`] by CALLER, the way [`super::sweep`] already is:
//! nothing here runs on a runner's own request path. Every one is reached from
//! [`crate::admin`], each carries the runner id in its predicate, and each
//! returns the prior state alongside its verdict, so the service tells a
//! harmless repeat from a refused attempt without a read-then-write race.

/// Retire one runner's record, but only from the terminal state and only once
/// no lease of its is still active.
///
/// One statement rather than a SELECT then a DELETE, so an operator revoking
/// concurrently cannot slip between the check and the write. The row's
/// leases and events cascade and its affinity clears, all declared in schema;
/// the daemon deletes nothing else by hand. A lease still active would take
/// with it the row the liveness sweep releases the fleet's slot through, so
/// the delete waits for the sweep or the lease's expiry. The single result row
/// says which of the three things happened, and its absence says there was no
/// such runner.
pub const DELETE_RUNNER_IF_IN_STATE: &str = "\
WITH current_row AS (
  SELECT id, admin_state,
         EXISTS (
           SELECT 1 FROM fleet.runner_leases l
           WHERE l.runner_id = fleet.runners.id AND l.status = $3::text
         ) AS leased
  FROM fleet.runners WHERE id = $1::uuid
), deleted AS (
  DELETE FROM fleet.runners r
  USING current_row c
  WHERE r.id = c.id AND c.admin_state = $2::text AND NOT c.leased
  RETURNING r.id::text
)
SELECT d.id, TRUE AS changed, FALSE AS leased FROM deleted d
UNION ALL
SELECT c.id::text, FALSE AS changed, c.leased FROM current_row c
WHERE NOT EXISTS (SELECT 1 FROM deleted)
LIMIT 1";

/// Move one runner's administrative state and append its audit event.
///
/// The locked row, update, and event live in one statement. The final select
/// returns the prior state even for an idempotent request, which lets the
/// service distinguish a harmless repeat from a forbidden attempt to move a
/// revoked runner without a read-then-write race.
pub const TRANSITION_RUNNER_ADMIN_STATE: &str = "\
WITH current_state AS (
  SELECT id, admin_state AS from_admin_state
  FROM fleet.runners
  WHERE id = $1::uuid
  FOR UPDATE
), updated AS (
  UPDATE fleet.runners r
  SET admin_state = $2::text, updated_at = $3::bigint
  FROM current_state c
  WHERE r.id = c.id
    AND ($4::bool OR c.from_admin_state <> 'revoked')
    AND c.from_admin_state <> $2::text
  RETURNING r.id
), event AS (
  INSERT INTO fleet.runner_events
    (id, runner_id, event_type, metadata, dedup_key, created_at)
  SELECT $5::uuid, id, $6::text,
         jsonb_build_object(
           $7::text, from_admin_state,
           $8::text, $2::text,
           $9::text, $10::text
         ),
         NULL, $3::bigint
  FROM current_state
  WHERE EXISTS (SELECT 1 FROM updated)
)
SELECT c.from_admin_state, EXISTS (SELECT 1 FROM updated) AS changed
FROM current_state c";

/// The policy mutation's locked state and reported capability.
pub const SELECT_RUNNER_PATCH_STATE: &str = "\
SELECT admin_state, capability_report::text
FROM fleet.runners WHERE id = $1::uuid FOR UPDATE";

/// Re-assign policy, reconciled verdict, and audit event atomically.
///
/// The caller holds this row's lock in the surrounding transaction. The
/// distinctness guard makes an identical request write neither row nor event.
pub const PATCH_RUNNER_ASSIGNED_POLICY: &str = "\
WITH updated AS (
  UPDATE fleet.runners
  SET sandbox_tier = $2::text, network_policy = $3::text,
      registry_allowlist = $4::jsonb, worker_count = $5::int,
      updated_at = $6::bigint, degraded = $13::bool,
      degraded_reason = $14::text, extra_binds = $15::jsonb
  WHERE id = $1::uuid
    AND (sandbox_tier IS DISTINCT FROM $2::text
      OR network_policy IS DISTINCT FROM $3::text
      OR registry_allowlist IS DISTINCT FROM $4::jsonb
      OR worker_count IS DISTINCT FROM $5::int
      OR extra_binds IS DISTINCT FROM $15::jsonb)
  RETURNING id
), event AS (
  INSERT INTO fleet.runner_events
    (id, runner_id, event_type, metadata, dedup_key, created_at)
  SELECT $7::uuid, id, $8::text,
         jsonb_build_object($9::text, $2::text, $10::text, $3::text,
                            $11::text, $4::jsonb, $12::text, $5::int),
         NULL, $6::bigint
  FROM updated
)
SELECT id::text FROM updated";

/// Record one outstanding self-test ask while preserving revocation terminality.
///
/// Returning the locked row even when the guard refuses the update lets the
/// caller distinguish a missing runner from a revoked one without a racy
/// follow-up read.
pub const PATCH_RUNNER_SELFTEST_REQUEST: &str = "\
WITH current_state AS (
  SELECT id, admin_state FROM fleet.runners WHERE id = $1::uuid FOR UPDATE
), updated AS (
  UPDATE fleet.runners r
  SET selftest_requested_at = $2::bigint, updated_at = $2::bigint
  FROM current_state c
  WHERE r.id = c.id AND c.admin_state <> 'revoked'
  RETURNING r.id
)
SELECT c.admin_state, EXISTS (SELECT 1 FROM updated) AS changed
FROM current_state c";
