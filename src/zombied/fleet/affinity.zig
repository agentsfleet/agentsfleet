//! fleet.runner_affinity — the per-zombie lease SLOT: the atomic claim, the
//! monotonic fencing source, and the sticky-routing hint, all on one row.
//!
//! `claim` is a single conditional UPSERT: it wins the zombie iff the slot is
//! free or its prior claim has expired (`leased_until < now`), bumping the
//! monotonic `fencing_seq` and recording the runner as the sticky hint. Exactly
//! one of N racing runners wins the row; losers get `.taken` and move on — and
//! crucially the claim precedes the event read, so a loser has consumed no
//! event (nothing is orphaned). `release` frees the slot at report, but is
//! token-guarded (`WHERE fencing_seq = token`) so a holder superseded by a
//! reclaim cannot free the current holder's slot; a dead runner never releases,
//! so its claim expires and another runner re-claims with a strictly higher
//! token. The report-time fence itself is a compare-and-swap in `service_report`
//! against this same `fencing_seq`.
//!
//! All functions run on a caller-supplied pooled connection (drained via
//! PgQuery / conn.exec).

const std = @import("std");
const pg = @import("pg");
const PgQuery = @import("../db/pg_query.zig").PgQuery;
const id_format = @import("../types/id_format.zig");

/// The won claim: the new monotonic fencing token + the expiry the slot (and
/// the issued lease row) carry.
pub const Won = struct {
    token: u64,
    leased_until: i64,
};

/// Outcome of a claim attempt.
pub const Claim = union(enum) {
    /// The slot was won.
    won: Won,
    /// A live runner still holds the slot — try another zombie. No event read.
    taken,
};

/// Atomically claim the zombie's lease slot for `runner_id`, valid for
/// `ttl_ms`. Wins iff the slot is unclaimed or its prior claim has expired;
/// bumps the monotonic fencing token and records the sticky hint. Returns
/// `.taken` when a live runner still holds it.
pub fn claim(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    zombie_id: []const u8,
    runner_id: []const u8,
    ttl_ms: i64,
) !Claim {
    const affinity_id = try id_format.generateRunnerAffinityId(alloc);
    defer alloc.free(affinity_id);
    const now_ms = std.time.milliTimestamp();
    const leased_until = now_ms + ttl_ms;
    var q = PgQuery.from(try conn.query(
        \\INSERT INTO fleet.runner_affinity
        \\  (id, zombie_id, last_runner_id, fencing_seq, leased_until, created_at, updated_at)
        \\VALUES ($1::uuid, $2::uuid, $3::uuid, 1, $4, $5, $5)
        \\ON CONFLICT (zombie_id) DO UPDATE
        \\  SET last_runner_id = EXCLUDED.last_runner_id,
        \\      fencing_seq    = fleet.runner_affinity.fencing_seq + 1,
        \\      leased_until   = EXCLUDED.leased_until,
        \\      updated_at     = EXCLUDED.updated_at
        \\  WHERE fleet.runner_affinity.leased_until < $5
        \\RETURNING fencing_seq
    , .{ affinity_id, zombie_id, runner_id, leased_until, now_ms }));
    defer q.deinit();
    const row = try q.next() orelse return .taken;
    return .{ .won = .{ .token = @intCast(try row.get(i64, 0)), .leased_until = leased_until } };
}

/// Free the slot (report / abandoned no-work claim) so the zombie's next event
/// is claimable — but only when `token` still equals the live `fencing_seq`, so
/// a holder superseded by a reclaim cannot free the current holder's slot.
/// Idempotent: a no-op if the row is gone or the token has been bumped.
pub fn release(conn: *pg.Conn, zombie_id: []const u8, token: u64) !void {
    const now_ms = std.time.milliTimestamp();
    _ = conn.exec(
        \\UPDATE fleet.runner_affinity SET leased_until = $2, updated_at = $2
        \\WHERE zombie_id = $1::uuid AND fencing_seq = $3
    , .{ zombie_id, now_ms, @as(i64, @intCast(token)) }) catch return error.AffinityReleaseFailed;
}
