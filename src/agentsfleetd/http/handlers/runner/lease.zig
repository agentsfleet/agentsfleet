//! POST /v1/runners/me/leases — long-poll for the next event.
//!
//! Thin wrapper over the control-plane service. Identity is the runner token
//! (`runnerBearer` populates `hx.principal.runner_id`); the service claims the
//! runner's one assigned agent, bills the event, persists a
//! `fleet.runner_leases` row, and returns 200 `{ lease | null, retry_after_ms }`
//! — never a 204. A DEGRADED runner is issued nothing: its assignment names
//! isolation the host cannot deliver, so a lease would run outside the
//! assigned boundary. Fail closed: an unreadable verdict also issues nothing.

const httpz = @import("httpz");
const constants = @import("common");
const hx_mod = @import("../hx.zig");
const sql = @import("sql.zig");
const service = @import("../../../fleet/service.zig");
const PgQuery = @import("../../../db/pg_query.zig").PgQuery;
const protocol = @import("contract").protocol;

const Hx = hx_mod.Hx;

pub fn innerRunnerLease(hx: Hx, req: *httpz.Request) void {
    _ = req; // S0 lease request body is empty; the long-poll is server-side.
    if (runnerDegraded(hx)) {
        hx.ok(.ok, protocol.LeaseResponse{ .lease = null, .retry_after_ms = constants.NO_WORK_RETRY_AFTER_MS });
        return;
    }
    service.leaseNext(hx);
}

/// The row's reconciled verdict — true also on any read failure (no verdict,
/// no lease). The connection is released before the service acquires its own.
fn runnerDegraded(hx: Hx) bool {
    const runner_id = hx.principal.runner_id orelse return true;
    const conn = hx.ctx.pool.acquire() catch return true;
    defer hx.ctx.pool.release(conn);
    var q = PgQuery.from(conn.query(sql.SELECT_RUNNER_DEGRADED, .{runner_id}) catch return true);
    defer q.deinit();
    const row = (q.next() catch return true) orelse return true;
    return row.get(bool, 0) catch true;
}
