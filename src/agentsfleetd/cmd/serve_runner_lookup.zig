//! DB-backed `LookupFn` for the `runnerBearer` middleware.
//!
//! `src/auth/middleware/` is portability-locked — it cannot reach into
//! `src/db/`. This module lives in `src/cmd/` (alongside the serve host that
//! wires it) and provides the concrete SHA-256-hex → `fleet.runners` lookup,
//! duplicating the kept `runner_id` into the caller's allocator. Read-only:
//! liveness (`last_seen_at`) is written by the heartbeat handler, not here.
//!
//! ## Every call reads the database, on purpose
//!
//! This verdict used to be memoized per process, which cost one indexed read per
//! runner per heartbeat interval instead of one per request. The memo is gone,
//! because an `agt_r` is the credential a cordon, drain, revoke, or delete has to
//! be able to STOP, and admin-state transitions have no other delivery channel —
//! the heartbeat reply is unconditionally `.ok`, so auth rejection is the only
//! way a runner learns it is out of service. A per-process memo made that
//! deterministic only on the machine that served the operator's write; every
//! sibling kept authenticating until its own entry expired.
//!
//! What that trades: one indexed single-row read per runner request, at the lease
//! poll's ~1/sec/worker. On a 100-row table with the `token_hash` index resident,
//! that is a few hundred index probes a second — measurably nothing — and in
//! exchange a revoked runner authenticates nowhere, immediately, with no window
//! to reason about and no state to reconcile between machines.
//!
//! It also means a Postgres outage now fails runner auth immediately rather than
//! being absorbed for up to one heartbeat. That surfaces as `503`
//! (`UZ-AUTH-004`), which the runner classifies as transport loss and backs off
//! from — NOT as an auth rejection, so an outage cannot trip the daemon's
//! consecutive-reject exit.

const std = @import("std");
const pg = @import("pg");

const db = @import("../db/pool.zig");
const PgQuery = @import("../db/pg_query.zig").PgQuery;
const runner_bearer = @import("../auth/middleware/runner_bearer.zig");
const protocol = @import("contract").protocol;

pub const LookupResult = runner_bearer.LookupResult;

/// Host context carrying the shared connection pool. A stable pointer to a
/// value of this type is passed as `host` in the `LookupFn` call.
pub const Ctx = struct {
    pool: *pg.Pool,
};

/// Resolve a SHA-256 hex digest to a `fleet.runners` row. Returns null when no
/// row matches. Allocates `runner_id` via `alloc`; the caller frees it — the
/// middleware on reject, the principal lifecycle on success.
pub fn lookup(
    host: *anyopaque,
    alloc: std.mem.Allocator,
    token_hash_hex: []const u8,
) anyerror!?LookupResult {
    const self: *Ctx = @ptrCast(@alignCast(host));
    const conn = self.pool.acquire() catch return error.DbUnavailable;
    defer self.pool.release(conn);

    // `degraded` rides the same indexed single-row read: the lease gate used
    // to re-read this exact row for that one flag, which doubled every idle
    // poll's Postgres cost. Same freshness — both values are re-read per
    // request, so revoke/cordon/degrade all land immediately.
    var q = PgQuery.from(conn.query(
        \\SELECT id::text, admin_state, degraded
        \\FROM fleet.runners
        \\WHERE token_hash = $1
        \\LIMIT 1
    , .{token_hash_hex}) catch return error.DbQueryFailed);
    defer q.deinit();

    const row = (q.next() catch return error.DbQueryFailed) orelse return null;
    return try copyRow(alloc, row);
}

fn copyRow(alloc: std.mem.Allocator, row: pg.Row) !LookupResult {
    const runner_id_raw = row.get([]u8, 0) catch return error.DbRowShape;
    const admin_state_raw = row.get([]u8, 1) catch return error.DbRowShape;
    const degraded = row.get(bool, 2) catch return error.DbRowShape;
    const active = std.mem.eql(u8, admin_state_raw, protocol.ADMIN_STATE_ACTIVE);

    const runner_id = try alloc.dupe(u8, runner_id_raw);
    return .{ .runner_id = runner_id, .active = active, .degraded = degraded };
}

// Referenced to silence "unused" warnings when the host isn't wired yet.
test {
    _ = db;
}
