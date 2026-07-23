//! SQL statement text for the runner control-plane handlers (RULE SQLMOD —
//! query text lives here, grepable in one place).
//!
//! Every read here is runner-scoped: a runner principal authorizes only verbs
//! about itself, so each statement carries `runner_id` in its predicate and can
//! never resolve another runner's row.

/// Enrol a runner and record the enrolment event atomically, so a registered
/// runner always has the audit row that explains where it came from.
pub const INSERT_RUNNER_WITH_EVENT =
    \\WITH inserted AS (
    \\  INSERT INTO fleet.runners
    \\  (id, host_id, token_hash, sandbox_tier, admin_state, labels, tenant_id,
    \\   last_seen_at, created_at, updated_at)
    \\VALUES ($1::uuid, $2::text, $3::text, $4::text, $5::text, $6::jsonb, NULL, $7::bigint, $8::bigint, $8::bigint)
    \\  RETURNING id
    \\)
    \\INSERT INTO fleet.runner_events
    \\  (id, runner_id, event_type, occurred_at, metadata, dedup_key, created_at)
    \\SELECT $9::uuid, id, $10::text, $8::bigint,
    \\       jsonb_build_object($11::text, $2::text, $12::text, $4::text),
    \\       NULL, $8::bigint
    \\FROM inserted
;

/// `GET /v1/runners/me`. Deliberately omits `token_hash` — the self read is
/// used by the operator CLI's `status`, and a credential must never round-trip.
pub const SELECT_RUNNER_SELF =
    \\SELECT id::text, admin_state, host_id, sandbox_tier, last_seen_at
    \\FROM fleet.runners WHERE id = $1::uuid
;

/// Resolve a live lease's billing scope before minting a credential for it.
/// The status and expiry predicates are the authorization: an expired or
/// released lease resolves nothing, so no credential can be minted against it.
pub const SELECT_LEASE_SCOPE_FOR_MINT =
    \\SELECT workspace_id::text, fleet_id::text
    \\FROM fleet.runner_leases
    \\WHERE id = $1::uuid AND runner_id = $2::uuid
    \\  AND status = $3 AND lease_expires_at > $4
;

/// Heartbeat: bump liveness, and emit a `runner_online` event only on a real
/// transition.
///
/// `FOR UPDATE` serialises concurrent heartbeats from the same host so the
/// pre-bump `last_seen_at` the event tests is the true previous value. The
/// trailing WHERE is what keeps the event stream quiet: an event lands only
/// when the runner was never seen, or was stale past the threshold — a steady
/// heartbeat writes liveness without writing history.
pub const HEARTBEAT_WITH_TRANSITION_EVENT =
    \\WITH locked AS (
    \\  SELECT id, last_seen_at FROM fleet.runners WHERE id = $1::uuid FOR UPDATE
    \\), bumped AS (
    \\  UPDATE fleet.runners r
    \\  SET last_seen_at = $2::bigint, updated_at = $2::bigint
    \\  FROM locked
    \\  WHERE r.id = locked.id
    \\  RETURNING locked.last_seen_at
    \\)
    \\INSERT INTO fleet.runner_events
    \\  (id, runner_id, event_type, occurred_at, metadata, dedup_key, created_at)
    \\SELECT $3::uuid, $1::uuid, $4::text, $2::bigint,
    \\       jsonb_build_object($5::text, last_seen_at), NULL, $2::bigint
    \\FROM bumped
    \\WHERE last_seen_at = $6::bigint OR ($2::bigint - last_seen_at) > $7::bigint
;

/// Liveness-only bump, for the paths that must not emit history.
pub const TOUCH_RUNNER_LAST_SEEN =
    \\UPDATE fleet.runners SET last_seen_at = $2, updated_at = $2 WHERE id = $1::uuid
;

// ── Memory fencing ──────────────────────────────────────────────────────────
// Both reads answer "what fencing token is currently valid for this runner on
// this fleet". `COALESCE(a.fencing_seq, l.fencing_token)` prefers the affinity
// slot's sequence, which advances on every re-claim, and falls back to the
// lease's own token when no slot row exists — so a superseded holder presenting
// an old token is rejected either way.

/// Newest live lease for a (runner, fleet) pair.
pub const SELECT_LIVE_FENCE_BY_FLEET =
    \\SELECT COALESCE(a.fencing_seq, l.fencing_token) AS live_seq
    \\FROM fleet.runner_leases l
    \\LEFT JOIN fleet.runner_affinity a ON a.fleet_id = l.fleet_id
    \\WHERE l.runner_id = $1::uuid AND l.fleet_id = $2::uuid
    \\  AND l.status = $3 AND l.lease_expires_at > $4
    \\ORDER BY l.created_at DESC
    \\LIMIT 1
;

/// The same fence, addressed by lease id when the caller already holds one.
pub const SELECT_LIVE_FENCE_BY_LEASE =
    \\SELECT COALESCE(a.fencing_seq, l.fencing_token) AS live_seq
    \\FROM fleet.runner_leases l
    \\LEFT JOIN fleet.runner_affinity a ON a.fleet_id = l.fleet_id
    \\WHERE l.id = $1::uuid AND l.runner_id = $2::uuid AND l.fleet_id = $3::uuid
    \\  AND l.status = $4 AND l.lease_expires_at > $5
    \\LIMIT 1
;
