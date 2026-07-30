//! GET /v1/runners/me — read-only runner self status.
//!
//! Authed by `runnerBearer` (the principal carries `runner_id`). A pure read:
//! it SELECTs the runner's own row and returns it, and — unlike the heartbeat —
//! does NOT bump `last_seen_at`. So `agentsfleet-runner status` inspecting a host can
//! never mask a dead runner's liveness (docs/AUTH.md — liveness is written by
//! the heartbeat handler only, not on every authed call).

const std = @import("std");
const httpz = @import("httpz");
const sql = @import("sql.zig");
const pg = @import("pg");

const hx_mod = @import("../hx.zig");
const common = @import("../common.zig");
const ec = @import("../../../errors/error_registry.zig");
const pg_query = @import("../../../db/pg_query.zig");
const policy_row = @import("assigned_policy_row.zig");
const protocol = @import("contract").protocol;

const Hx = hx_mod.Hx;
const PgQuery = pg_query.PgQuery;

pub fn innerRunnerSelf(hx: Hx, req: *httpz.Request) void {
    _ = req; // read-only; no request body.
    const runner_id = hx.principal.runner_id orelse {
        // runnerBearer guarantees this; defensive only.
        hx.fail(ec.ERR_RUN_INVALID_RUNNER_TOKEN, "runner identity required");
        return;
    };

    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer hx.ctx.pool.release(conn);

    var q = PgQuery.from(conn.query(sql.SELECT_RUNNER_SELF, .{runner_id}) catch {
        common.internalDbError(hx.res, hx.req_id);
        return;
    });
    defer q.deinit();

    const row = (q.next() catch {
        common.internalDbError(hx.res, hx.req_id);
        return;
    }) orelse {
        // The token authenticated but the row is gone (revoked + reaped) — fail
        // closed rather than 200 a phantom runner.
        hx.fail(ec.ERR_RUN_INVALID_RUNNER_TOKEN, "runner not found");
        return;
    };

    // Row slices borrow the result buffer — `hx.ok` serializes now, before the
    // deferred `q.deinit()` frees it. Registry/capability slices are re-parsed
    // into the request arena, so they outlive the row either way.
    const self_row = readRow(hx.alloc, row) catch {
        common.internalDbError(hx.res, hx.req_id);
        return;
    };
    hx.ok(.ok, self_row);
}

/// Map the SELECTed row to the wire shape (`row.get` is fallible — a shape
/// mismatch surfaces as a DB error rather than a panic). The `admin_state` column
/// (renamed from `status`) feeds the `status` self-view field unchanged — the
/// runner-facing label stays stable; the daemon's wire shape does not move.
/// `assigned_policy` resolves through the shared decoder: any missing or
/// unparseable policy column reads as null, and the host fails closed on it.
fn readRow(alloc: std.mem.Allocator, row: pg.Row) !protocol.SelfResponse {
    return .{
        .id = try row.get([]const u8, 0),
        .status = try row.get([]const u8, 1),
        .host_id = try row.get([]const u8, 2),
        .sandbox_tier = try row.get([]const u8, 3),
        .last_seen_at = try row.get(i64, 4),
        .assigned_policy = policy_row.decodePolicy(
            alloc,
            try row.get([]const u8, 3),
            try row.get(?[]const u8, 5),
            try row.get(?[]const u8, 6),
            try row.get(i32, 7),
        ),
        .achievable = policy_row.decodeCapability(alloc, try row.get(?[]const u8, 8)),
        .degraded = try row.get(bool, 9),
        .degraded_reason = try row.get(?[]const u8, 10),
    };
}
