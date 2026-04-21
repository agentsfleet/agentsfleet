//! M18_001: Zombie execution telemetry HTTP handlers.
//!
//! Customer:  GET /v1/workspaces/{ws}/zombies/{id}/telemetry?limit=50&cursor=
//! Operator:  GET /internal/v1/telemetry?workspace_id=&zombie_id=&after=&limit=100
//!
//! Auth: customer uses bearer policy; operator uses admin policy. Principal is
//! set by the middleware chain; customer additionally enforces workspace scope
//! via authorizeWorkspaceAndSetTenantContext.

const std = @import("std");
const httpz = @import("httpz");
const pg = @import("pg");

const common = @import("../common.zig");
const hx_mod = @import("../hx.zig");
const ec = @import("../../../errors/error_registry.zig");
const store = @import("../../../state/zombie_telemetry_store.zig");
const id_format = @import("../../../types/id_format.zig");

const log = std.log.scoped(.http_zombie_telemetry);

pub const Context = common.Context;

const LIMIT_DEFAULT: u32 = 50;
const LIMIT_MAX_CUSTOMER: u32 = 200;
const LIMIT_MAX_OPERATOR: u32 = 500;

// ── Customer endpoint ──────────────────────────────────────────────────────

pub fn innerZombieTelemetry(
    hx: hx_mod.Hx,
    req: *httpz.Request,
    workspace_id: []const u8,
    zombie_id: []const u8,
) void {
    const qs = req.query() catch {
        hx.fail(ec.ERR_INVALID_REQUEST, "malformed query string");
        return;
    };

    const limit = parseLimitFromQs(qs, LIMIT_MAX_CUSTOMER, LIMIT_DEFAULT) catch {
        hx.fail(ec.ERR_INVALID_REQUEST, "limit must be between 1 and 200");
        return;
    };
    const cursor = qs.get("cursor");

    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer hx.ctx.pool.release(conn);

    if (!common.authorizeWorkspaceAndSetTenantContext(conn, hx.principal, workspace_id)) {
        hx.fail(ec.ERR_FORBIDDEN, "Workspace access denied");
        return;
    }

    const rows = store.listTelemetryForZombie(conn, hx.alloc, workspace_id, zombie_id, limit, cursor) catch |err| {
        if (err == error.InvalidCursor) {
            hx.fail(ec.ERR_INVALID_REQUEST, "invalid cursor");
            return;
        }
        log.err("listTelemetryForZombie failed err={s}", .{@errorName(err)});
        common.internalDbError(hx.res, hx.req_id);
        return;
    };

    const next_cursor: ?[]u8 = if (rows.len == limit and rows.len > 0) blk: {
        break :blk store.makeCursor(hx.alloc, rows[rows.len - 1]) catch {
            common.internalDbError(hx.res, hx.req_id);
            return;
        };
    } else null;

    hx.ok(.ok, .{ .items = rows, .cursor = next_cursor });
}

// ── Operator endpoint ──────────────────────────────────────────────────────

pub fn innerInternalTelemetry(hx: hx_mod.Hx, req: *httpz.Request) void {
    const qs = req.query() catch {
        hx.fail(ec.ERR_INVALID_REQUEST, "malformed query string");
        return;
    };

    const limit = parseLimitFromQs(qs, LIMIT_MAX_OPERATOR, LIMIT_DEFAULT) catch {
        hx.fail(ec.ERR_INVALID_REQUEST, "limit must be between 1 and 500");
        return;
    };
    const workspace_id = qs.get("workspace_id");
    const zombie_id = qs.get("zombie_id");
    if (workspace_id) |wid| {
        if (!id_format.isSupportedWorkspaceId(wid)) {
            hx.fail(ec.ERR_INVALID_REQUEST, "workspace_id must be a valid UUIDv7");
            return;
        }
    }
    if (zombie_id) |zid| {
        if (!id_format.isSupportedWorkspaceId(zid)) {
            hx.fail(ec.ERR_INVALID_REQUEST, "zombie_id must be a valid UUIDv7");
            return;
        }
    }
    const after_ms: ?i64 = blk: {
        const raw = qs.get("after") orelse break :blk null;
        const parsed = std.fmt.parseInt(i64, raw, 10) catch {
            hx.fail(ec.ERR_INVALID_REQUEST, "after must be epoch ms (integer)");
            return;
        };
        break :blk parsed;
    };

    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer hx.ctx.pool.release(conn);

    const rows = store.listTelemetryAll(conn, hx.alloc, workspace_id, zombie_id, after_ms, limit) catch |err| {
        log.err("listTelemetryAll failed err={s}", .{@errorName(err)});
        common.internalDbError(hx.res, hx.req_id);
        return;
    };

    hx.ok(.ok, .{ .items = rows });
}

// ── Shared helpers ─────────────────────────────────────────────────────────

fn parseLimitFromQs(qs: anytype, max: u32, default: u32) !u32 {
    const raw = qs.get("limit") orelse return default;
    const n = std.fmt.parseInt(u32, raw, 10) catch return error.InvalidLimit;
    if (n == 0 or n > max) return error.InvalidLimit;
    return n;
}
