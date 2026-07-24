//! SQL statement text for the webhook handler domain (RULE SQLMOD — query text
//! lives here, grepable in one place).
//!
//! Webhook callers are authenticated by signature, not by a session, so every
//! statement here re-derives its scope from the database rather than trusting
//! anything in the request body.

/// Resolve a fleet's owning workspace. The grant-approval flow needs it to
/// scope the mutation that follows, and takes it from here rather than from the
/// signed payload.
pub const SELECT_FLEET_WORKSPACE =
    \\SELECT workspace_id::text FROM core.fleets WHERE id = $1::uuid LIMIT 1
;

pub const SELECT_FLEET_WORKSPACE_AND_STATUS =
    \\SELECT workspace_id::text, status FROM core.fleets WHERE id = $1::uuid
;

/// Approve a pending grant. The `status = $5` guard makes the decision
/// single-shot: a replayed approval webhook matches no row, so a grant already
/// revoked cannot be flipped back by a stale delivery.
pub const APPROVE_GRANT =
    \\UPDATE core.integration_grants
    \\SET status = $1, approved_at = $2
    \\WHERE grant_id = $3 AND fleet_id = $4::uuid AND status = $5
;

/// Revoke a pending grant, same single-shot guard.
pub const REVOKE_GRANT =
    \\UPDATE core.integration_grants
    \\SET status = $1, revoked_at = $2
    \\WHERE grant_id = $3 AND fleet_id = $4::uuid AND status = $5
;
