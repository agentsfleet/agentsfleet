//! PATCH /v1/fleet/runners/{id} — platform-admin runner operator plane.

const std = @import("std");
const clock = @import("common").clock;
const httpz = @import("httpz");
const pg = @import("pg");
const logging = @import("log");

const common = @import("../common.zig");
const hx_mod = @import("../hx.zig");
const ec = @import("../../../errors/error_registry.zig");
const PgQuery = @import("../../../db/pg_query.zig").PgQuery;
const protocol = @import("contract").protocol;

const Hx = hx_mod.Hx;
const log = logging.scoped(.fleet_runner_patch);

const S_PATCH_BODY = "PATCH body must be {\"action\":\"cordon|drain|revoke\"}";
const S_RUNNER_NOT_FOUND = "Runner not found";
const S_REVOKED_IS_TERMINAL = "revoked runners cannot transition back to cordoned or draining";

pub fn innerPatchFleetRunner(hx: Hx, req: *httpz.Request, runner_id: []const u8) void {
    if (!common.requireUuidV7Id(hx.res, hx.req_id, runner_id, "runner_id")) return;
    const body = parseBody(hx, req) orelse return;
    const target = stateForAction(body.action);

    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer hx.ctx.pool.release(conn);

    const current = loadState(conn, runner_id) catch {
        common.internalDbError(hx.res, hx.req_id);
        return;
    } orelse {
        hx.fail(ec.ERR_RUNNER_NOT_FOUND, S_RUNNER_NOT_FOUND);
        return;
    };
    if (current == .revoked and target != .revoked) {
        hx.fail(ec.ERR_INVALID_REQUEST, S_REVOKED_IS_TERMINAL);
        return;
    }
    if (current == target) {
        return writeResponse(hx, runner_id, target);
    }

    updateState(conn, runner_id, target) catch |err| {
        switch (err) {
            error.RunnerGone => hx.fail(ec.ERR_RUNNER_NOT_FOUND, S_RUNNER_NOT_FOUND),
            error.RevokedRace => hx.fail(ec.ERR_INVALID_REQUEST, S_REVOKED_IS_TERMINAL),
            else => common.internalDbError(hx.res, hx.req_id),
        }
        return;
    };

    log.info("runner_admin_state_changed", .{ .runner_id = runner_id, .admin_state = @tagName(target) });
    writeResponse(hx, runner_id, target);
}

fn parseBody(hx: Hx, req: *httpz.Request) ?protocol.RunnerAdminPatchRequest {
    const raw = req.body() orelse {
        hx.fail(ec.ERR_INVALID_REQUEST, S_PATCH_BODY);
        return null;
    };
    if (raw.len == 0) {
        hx.fail(ec.ERR_INVALID_REQUEST, S_PATCH_BODY);
        return null;
    }
    if (!common.checkBodySize(req, hx.res, raw, hx.req_id)) return null;
    const parsed = std.json.parseFromSlice(protocol.RunnerAdminPatchRequest, hx.alloc, raw, .{}) catch {
        hx.fail(ec.ERR_INVALID_REQUEST, S_PATCH_BODY);
        return null;
    };
    defer parsed.deinit();
    return .{ .action = parsed.value.action };
}

fn stateForAction(action: protocol.RunnerAdminAction) protocol.AdminState {
    return switch (action) {
        .cordon => .cordoned,
        .drain => .draining,
        .revoke => .revoked,
    };
}

fn loadState(conn: *pg.Conn, runner_id: []const u8) !?protocol.AdminState {
    var q = PgQuery.from(conn.query(
        \\SELECT admin_state FROM fleet.runners WHERE id = $1::uuid
    , .{runner_id}) catch return error.DbError);
    defer q.deinit();

    const row = q.next() catch return error.DbError;
    if (row == null) return null;
    const raw = row.?.get([]u8, 0) catch return error.DbError;
    return std.meta.stringToEnum(protocol.AdminState, raw) orelse error.DbRowShape;
}

fn updateState(conn: *pg.Conn, runner_id: []const u8, target: protocol.AdminState) !void {
    const now_ms = clock.nowMillis();
    const allows_non_revoked = target == .revoked;
    var q = PgQuery.from(conn.query(
        \\UPDATE fleet.runners
        \\SET admin_state = $2, updated_at = $3
        \\WHERE id = $1::uuid AND ($4::bool OR admin_state <> $5)
        \\RETURNING id::text
    , .{ runner_id, @tagName(target), now_ms, allows_non_revoked, @tagName(protocol.AdminState.revoked) }) catch return error.DbError);
    defer q.deinit();

    const row = q.next() catch return error.DbError;
    if (row != null) return;
    const after = try loadState(conn, runner_id) orelse return error.RunnerGone;
    if (after == .revoked and target != .revoked) return error.RevokedRace;
    return error.DbError;
}

fn writeResponse(hx: Hx, runner_id: []const u8, admin_state: protocol.AdminState) void {
    hx.ok(.ok, protocol.RunnerAdminPatchResponse{ .id = runner_id, .admin_state = admin_state });
}
