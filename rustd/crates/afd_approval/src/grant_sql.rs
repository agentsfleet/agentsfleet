//! Every statement the GRANT half of this crate runs, and nothing else.
//!
//! Split from [`super::sql`] for the reason `afd_gate` already splits its own
//! pair: `core.fleet_approval_gates` records a human answering about ONE event
//! and `core.integration_grants` records a human answering about a fleet's
//! relationship with a third party, once, for every event after it. Two
//! authorities, two statement sets, so a reader looking for the grant half does
//! not read past the gate half to find it.
//!
//! Text is byte-identical to `integration_grants/workspace.zig`. Row-equivalence
//! is the cutover invariant, so a statement is copied rather than re-derived;
//! where a `$n` order looks odd, it is odd in the original too.

/// Whether `$1` is a fleet that `$2` holds.
///
/// The port of `common.getFleetWorkspaceId` plus the equality check that
/// follows every one of its call sites: the Zig fetches the fleet's workspace
/// and compares it in the handler, which is one round trip's worth of row to
/// answer a yes-or-no the predicate can answer itself.
///
/// The two TENANT-FACING verbs run it FIRST, because both must tell "no such
/// fleet here" from their own absent row, and the two carry different codes. A
/// fleet in another workspace answers no rows — never a 403 — so the endpoint
/// cannot be an oracle for which fleet identifiers are real.
///
/// [`REQUEST_GRANT`] deliberately does NOT run it, and the reason is the caller
/// rather than the statement: both of its callers derive the workspace and the
/// fleet from ONE trusted row — the install from the fleet it just wrote into
/// the authenticated workspace, the park from the `Acquired` lease it is
/// serving — so there is no untrusted pair to reject, and the park path would
/// pay the round trip once per second to re-answer a question its own lease
/// row already settled. No endpoint takes a caller-supplied `(workspace,
/// fleet)` pair into that statement; the precondition on
/// `IntegrationGrants::request` is what keeps that true, and a third caller
/// that cannot honour it must run this check itself before asking.
pub(crate) const SELECT_FLEET_IN_WORKSPACE: &str = "\
SELECT 1 FROM core.fleets WHERE id = $1::uuid AND workspace_id = $2::uuid";

/// Every grant a fleet holds, newest first.
///
/// Copied from `integration_grants/workspace.zig`'s `innerListGrants`. Unpaged
/// and unfiltered: a fleet holds at most one grant per service — the unique
/// constraint on `(fleet_id, service)` says so — and the supported-service
/// count is what bounds the page. `requested_reason` is the wire's `reason`.
///
/// `$1` fleet.
pub(crate) const SELECT_FLEET_GRANTS: &str = "\
SELECT id::text, service, status, created_at, approved_at, revoked_at, requested_reason
FROM core.integration_grants
WHERE fleet_id = $1::uuid
ORDER BY created_at DESC";

/// Revokes one grant, scoped to the workspace that holds its fleet.
///
/// Copied from `integration_grants/workspace.zig`'s `innerRevokeGrant`,
/// including the join to `core.fleets` the handler had already made redundant.
/// That redundancy is the point and it is load-bearing: if the fleet-scope read
/// above is ever dropped from this crate, the statement still refuses a
/// cross-workspace revoke, and `workspace.zig`'s own integration test runs this
/// exact text with a foreign workspace to prove it.
///
/// `g.status != $1` is what makes a second revoke report nothing rather than
/// re-stamping `revoked_at`, so the caller can tell "I revoked it" from
/// "it was already gone" without a read-then-write.
///
/// `$1` revoked status, `$2` now, `$3` grant, `$4` fleet, `$5` workspace.
pub(crate) const REVOKE_GRANT: &str = "\
UPDATE core.integration_grants g
SET status = $1, revoked_at = $2
FROM core.fleets z
WHERE g.id = $3::uuid
  AND g.fleet_id = $4::uuid
  AND z.id = g.fleet_id
  AND z.workspace_id = $5::uuid
  AND g.status != $1
RETURNING g.id";

/// Ensures the grant exists before the card statement takes its snapshot.
/// Both statements run in one transaction: failure to raise rolls back the grant.
pub(crate) const ENSURE_GRANT: &str = "\
INSERT INTO core.integration_grants
  (id, fleet_id, service, status, requested_reason, created_at)
VALUES ($1::uuid, $2::uuid, $3, $4, $5, $6)
ON CONFLICT (fleet_id, service) DO NOTHING";

/// Raises a card using the existing grant's identity, including when a concurrent
/// insert won. Locking the grant serializes requests with its approval/revocation;
/// the unique active reference arbitrates cards without a status-dependent index.
/// A NULL `event_id` prevents approval from enqueueing duplicate delivery work.
///
/// $1 fleet, $2 service, $3 grant status, $4 now, $5 card, $6 workspace,
/// $7 action, $8 tool, $9 action name, $10 kind, $11 proposal, $12 evidence,
/// $13 radius, $14 deadline, $15 card status.
pub(crate) const REQUEST_GRANT: &str = "\
WITH requested AS (
  SELECT id, status FROM core.integration_grants
  WHERE fleet_id = $1::uuid AND service = $2
  FOR UPDATE
), raised AS (
  INSERT INTO core.fleet_approval_gates
    (id, fleet_id, workspace_id, action_id, tool_name, action_name,
     gate_kind, proposed_action, evidence, blast_radius, timeout_at,
     resolved_by, status, detail, created_at, event_id, stated_binding,
     spend_count, spend_ceiling, active_grant_id)
  SELECT $5::uuid, $1::uuid, $6::uuid, $7, $8, $9,
         $10, $11, $12::jsonb, $13, $14,
         '', $15, '', $4, NULL, NULL, NULL, NULL, requested.id
  FROM requested
  WHERE requested.status = $3 AND NOT EXISTS (
    SELECT 1 FROM core.fleet_approval_gates g
    WHERE g.active_grant_id = requested.id
  )
  ON CONFLICT (active_grant_id) DO NOTHING
  RETURNING id
)
SELECT (SELECT COUNT(*) FROM raised), requested.status FROM requested";

/// Writes the grant an install already answers, and reports the status that stands.
///
/// One statement rather than an insert followed by an update: a row that lands
/// `pending` and is approved a moment later has a window in which a delivery is
/// refused for a grant nobody was going to be asked about.
///
/// `DO NOTHING` rather than an upsert, because a re-install must not un-revoke
/// a grant a person revoked — [`REVOKE_GRANT`] is meant to outlive the next
/// install. The `UNION ALL` arm reports whichever status survived, so a caller
/// can tell the grant it just wrote from the one that was already there.
///
/// `approved_at` and `created_at` take the same instant deliberately: nobody
/// was asked, so the moment the row was written IS the moment it was answered.
///
/// `$1` grant, `$2` fleet, `$3` service, `$4` approved status, `$5` reason,
/// `$6` now.
pub(crate) const GRANT_AT_INSTALL: &str = "\
WITH written AS (
  INSERT INTO core.integration_grants
    (id, fleet_id, service, status, requested_reason, approved_at, created_at)
  VALUES ($1::uuid, $2::uuid, $3, $4, $5, $6, $6)
  ON CONFLICT (fleet_id, service) DO NOTHING
  RETURNING status
)
SELECT status FROM written
UNION ALL
SELECT g.status FROM core.integration_grants g
WHERE g.fleet_id = $2::uuid AND g.service = $3
  AND NOT EXISTS (SELECT 1 FROM written)";
