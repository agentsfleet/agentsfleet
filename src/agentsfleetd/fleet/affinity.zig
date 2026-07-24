//! fleet.runner_affinity — the per-fleet lease SLOT: the atomic claim, the
//! monotonic fencing source, and the sticky-routing hint, all on one row.
//!
//! `claim` is a single conditional UPSERT: it wins the fleet iff the slot is
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
const sql = @import("sql.zig");
const clock = @import("common").clock;
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
    /// A live runner still holds the slot — try another fleet. No event read.
    taken,
};

/// Atomically claim the fleet's lease slot for `runner_id`, valid for
/// `ttl_ms`. Wins iff the slot is unclaimed or its prior claim has expired;
/// bumps the monotonic fencing token and records the sticky hint. Returns
/// `.taken` when a live runner still holds it.
///
/// The durable metering cursor is seeded `0`/now on a brand-new slot and is
/// deliberately ABSENT from the `ON CONFLICT` SET — so it is preserved across a
/// reclaim (the re-leased run meters forward from the dead holder's progress).
/// A fresh event resets it at lease issue; the renewal CTE advances it.
pub fn claim(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    fleet_id: []const u8,
    runner_id: []const u8,
    ttl_ms: i64,
) !Claim {
    const affinity_id = try id_format.generateRunnerAffinityId(alloc);
    defer alloc.free(affinity_id);
    const now_ms = clock.nowMillis();
    const leased_until = now_ms + ttl_ms;
    var q = PgQuery.from(try conn.query(sql.CLAIM_AFFINITY_SLOT, .{ affinity_id, fleet_id, runner_id, leased_until, now_ms }));
    defer q.deinit();
    const row = try q.next() orelse return .taken;
    return .{ .won = .{ .token = @intCast(try row.get(i64, 0)), .leased_until = leased_until } };
}

/// Reset the per-fleet metering cursor to 0/now — called at FRESH lease issue
/// so a new event meters from zero even when the slot was reused from a prior
/// (completed) run whose cursor the claim's `ON CONFLICT` preserved. A reclaim
/// does NOT call this: the slot must keep the dead holder's progress so the
/// re-leased run meters forward from where it stopped. The renewal CTE reads
/// this cursor for each slice's Δ, so a stale value here would over-charge the
/// first renewal — hence the reset is fail-closed (a reset error fails lease
/// issue rather than risk an over-charge). `meter_slice_seq` resets too so the
/// new event's breakdown numbering restarts at 1.
pub fn resetCursor(conn: *pg.Conn, fleet_id: []const u8, now_ms: i64) !void {
    _ = conn.exec(sql.RESET_AFFINITY_METERS, .{ fleet_id, now_ms }) catch return error.AffinityCursorResetFailed;
}

/// Free the slot (report / abandoned no-work claim) so the fleet's next event
/// is claimable — but only when `token` still equals the live `fencing_seq`, so
/// a holder superseded by a reclaim cannot free the current holder's slot.
/// Idempotent: a no-op if the row is gone or the token has been bumped.
pub fn release(conn: *pg.Conn, fleet_id: []const u8, token: u64) !void {
    const now_ms = clock.nowMillis();
    _ = conn.exec(sql.RELEASE_AFFINITY_SLOT, .{ fleet_id, now_ms, @as(i64, @intCast(token)) }) catch return error.AffinityReleaseFailed;
}
