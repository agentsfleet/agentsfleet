//! fleet — the `renew` operation: atomically extend a live lease's deadline.
//!
//! Decouples lease liveness from execution duration. A runner that is actively
//! executing an agent calls `POST /v1/runners/me/leases/{id}/renew` inside the
//! renewal window; this pushes the kill deadline forward so a legitimate >30s
//! run is never reclaimed mid-flight.
//!
//! The hard part is that reclaimability is driven by `runner_affinity.leased_until`
//! (the slot `affinity.claim` checks), a SEPARATE row from `runner_leases.
//! lease_expires_at` (the child kill deadline). Renewing one but not the other
//! still gets a healthy run reclaimed at the TTL. So `renew` extends BOTH rows
//! in ONE writable-CTE statement, to the SAME clamped value, guarded by the same
//! live fence `service_report` uses (`fencing_token >= fencing_seq`) plus
//! `status = 'active'`. The check and the two writes share one snapshot — a
//! concurrent reclaim cannot split them. That single-statement atomicity is the
//! whole correctness story: the rows can never diverge.
//!
//! The deadline is clamped to the hard cap `created_at + MAX_RUNTIME_MS` — a
//! wedged-but-emitting agent still terminates. Three terminal verdicts are
//! distinguished for the caller (credit-gating lives in the handler, before this):
//!   - `renewed`     → both rows advanced; the new deadline is returned.
//!   - `max_runtime` → the lease is still ours but the hard cap is reached (010).
//!   - `lost`        → the lease is no longer active/ours (reclaimed or fenced) (011).
//!
//! Runs on a caller-supplied pooled connection (drained via PgQuery).

const pg = @import("pg");
const PgQuery = @import("../db/pg_query.zig").PgQuery;
const constants = @import("common");
const protocol = @import("contract").protocol;

/// The verdict of a renewal attempt. A tagged union so the handler can map each
/// case to its own wire code without re-deriving context (UFS/type-design rule).
pub const RenewOutcome = union(enum) {
    /// Both rows advanced to this `lease_expires_at` (epoch ms).
    renewed: i64,
    /// Still the live holder, but `created_at + MAX_RUNTIME_MS` is reached — the
    /// run must terminate (UZ-RUN-010). Carries the cap for logging.
    max_runtime: i64,
    /// The lease is no longer `active` or no longer ours (reclaimed/fenced) —
    /// the runner must kill its child (UZ-RUN-011).
    lost,
};

/// Atomically extend both the lease row and its affinity slot to
/// `min(now + LEASE_TTL_MS, created_at + MAX_RUNTIME_MS)`, guarded by
/// `status = 'active'` AND the presenting runner still being the live fencing
/// holder. One writable-CTE statement: `probe` reads the lease+slot under the
/// runner scope; `guard` survives only if the fence holds and the cap is not yet
/// reached; `ext_lease`/`ext_aff` fire iff `guard` is non-empty, so both rows
/// advance or neither does. The trailing SELECT returns enough to disambiguate
/// renewed / max_runtime / lost without a second round-trip.
pub fn renew(
    conn: *pg.Conn,
    lease_id: []const u8,
    runner_id: []const u8,
    now_ms: i64,
) !RenewOutcome {
    const want_until = now_ms + constants.LEASE_TTL_MS;
    var q = PgQuery.from(try conn.query(
        \\WITH probe AS (
        \\    SELECT l.id, l.zombie_id, l.created_at, l.fencing_token, a.fencing_seq,
        \\           LEAST($3::bigint, l.created_at + $4::bigint) AS capped
        \\    FROM fleet.runner_leases l
        \\    JOIN fleet.runner_affinity a ON a.zombie_id = l.zombie_id
        \\    WHERE l.id = $1::uuid AND l.runner_id = $2::uuid AND l.status = $5
        \\), guard AS (
        \\    SELECT id, zombie_id, capped FROM probe
        \\    WHERE fencing_token >= fencing_seq AND capped > $6::bigint
        \\), ext_lease AS (
        \\    UPDATE fleet.runner_leases l SET lease_expires_at = g.capped, updated_at = $6
        \\    FROM guard g WHERE l.id = g.id
        \\    RETURNING g.capped
        \\), ext_aff AS (
        \\    UPDATE fleet.runner_affinity a SET leased_until = g.capped, updated_at = $6
        \\    FROM guard g WHERE a.zombie_id = g.zombie_id
        \\    RETURNING a.zombie_id
        \\)
        \\SELECT
        \\    (SELECT count(*) FROM probe)::bigint                    AS probe_found,
        \\    (SELECT capped FROM ext_lease)                          AS new_until,
        \\    (SELECT created_at + $4::bigint FROM probe)             AS hard_cap
    , .{
        lease_id,
        runner_id,
        want_until,
        constants.MAX_RUNTIME_MS,
        protocol.RUNNER_LEASE_STATUS_ACTIVE,
        now_ms,
    }));
    defer q.deinit();
    const row = try q.next() orelse return .lost;
    const probe_found = try row.get(i64, 0);
    const new_until = try row.get(?i64, 1);
    const hard_cap = try row.get(?i64, 2);

    if (new_until) |until| return .{ .renewed = until };
    // No extension happened. If the lease isn't active/ours at all → lost.
    if (probe_found == 0) return .lost;
    // It IS still ours+active, so the only reason the guard failed is the cap
    // (capped <= now) or a stale fence. The cap is the deterministic, reported
    // case; a stale fence means a reclaim already won → also lost.
    if (hard_cap) |cap| if (cap <= now_ms) return .{ .max_runtime = cap };
    return .lost;
}
