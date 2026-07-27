//! DB-backed `LookupFn` for the `runnerBearer` middleware.
//!
//! `src/auth/middleware/` is portability-locked — it cannot reach into
//! `src/db/`. This module lives in `src/cmd/` (alongside the serve host that
//! wires it) and provides the concrete SHA-256-hex → `fleet.runners` lookup,
//! duplicating the kept `runner_id` into the caller's allocator. Read-only:
//! liveness (`last_seen_at`) is written by the heartbeat handler, not here.

const std = @import("std");
const pg = @import("pg");

const db = @import("../db/pool.zig");
const PgQuery = @import("../db/pg_query.zig").PgQuery;
const runner_bearer = @import("../auth/middleware/runner_bearer.zig");
const token_cache = @import("../auth/runner_token_cache.zig");
const clock = @import("common").clock;
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
    const now_ms = clock.nowMillis();
    // The steady state: an idle runner heartbeating and polling costs no
    // Postgres read at all. The entry expires within one heartbeat interval and
    // the operator plane drops it outright on an admin-state change or a delete,
    // so a runner taken out of service stops authenticating without waiting for
    // the pool. See `auth/runner_token_cache.zig` for the window this trades.
    if (token_cache.get(token_hash_hex, now_ms)) |hit| {
        return .{ .runner_id = try alloc.dupe(u8, hit.runnerId()), .active = hit.active };
    }

    // Read BEFORE the Postgres lookup: if an operator invalidates this runner
    // while the query below is in flight, the row in hand predates the change
    // and `put` must refuse it rather than resurrect a revoked verdict for a
    // full window. Read AFTER the hit check above, where it is never consumed —
    // the hit path is every steady-state request, and a generation read there
    // was a second acquisition of the same mutex for nothing.
    const seen_generation = token_cache.generation();
    const conn = self.pool.acquire() catch return error.DbUnavailable;
    defer self.pool.release(conn);

    var q = PgQuery.from(conn.query(
        \\SELECT id::text, admin_state
        \\FROM fleet.runners
        \\WHERE token_hash = $1
        \\LIMIT 1
    , .{token_hash_hex}) catch return error.DbQueryFailed);
    defer q.deinit();

    // A miss is deliberately NOT memoized. Nothing is gained — a token minted
    // later is freshly random and was never asked about — and memoizing would
    // hand an unauthenticated caller a way to evict live runners from the table
    // by presenting garbage. Unknown tokens keep costing exactly what they cost
    // today.
    const row = (q.next() catch return error.DbQueryFailed) orelse return null;
    const result = try copyRow(alloc, row);
    token_cache.put(token_hash_hex, result.runner_id, result.active, now_ms, seen_generation);
    return result;
}

fn copyRow(alloc: std.mem.Allocator, row: pg.Row) !LookupResult {
    const runner_id_raw = row.get([]u8, 0) catch return error.DbRowShape;
    const admin_state_raw = row.get([]u8, 1) catch return error.DbRowShape;
    const active = std.mem.eql(u8, admin_state_raw, protocol.ADMIN_STATE_ACTIVE);

    const runner_id = try alloc.dupe(u8, runner_id_raw);
    return .{ .runner_id = runner_id, .active = active };
}

// Referenced to silence "unused" warnings when the host isn't wired yet.
test {
    _ = db;
}
