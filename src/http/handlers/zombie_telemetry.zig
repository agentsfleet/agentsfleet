//! M18_001: Zombie execution telemetry HTTP handlers.
//!
//! Customer:  GET /v1/workspaces/{ws}/zombies/{id}/telemetry?limit=50&cursor=
//! Operator:  GET /internal/v1/telemetry?workspace_id=&zombie_id=&after=&limit=100
//!
//! Auth: both use common.authenticate (api_key or JWT). Customer enforces
//! workspace scope via authorizeWorkspaceAndSetTenantContext.

const std = @import("std");
const httpz = @import("httpz");
const pg = @import("pg");

const common = @import("common.zig");
const ec = @import("../../errors/error_registry.zig");
const store = @import("../../state/zombie_telemetry_store.zig");

const log = std.log.scoped(.http_zombie_telemetry);

pub const Context = common.Context;

const LIMIT_DEFAULT: u32 = 50;
const LIMIT_MAX_CUSTOMER: u32 = 200;
const LIMIT_MAX_OPERATOR: u32 = 500;

// ── Customer endpoint ──────────────────────────────────────────────────────

pub fn handleZombieTelemetry(
    ctx: *Context,
    req: *httpz.Request,
    res: *httpz.Response,
    workspace_id: []const u8,
    zombie_id: []const u8,
) void {
    var arena = std.heap.ArenaAllocator.init(ctx.alloc);
    defer arena.deinit();
    const alloc = arena.allocator();
    const req_id = common.requestId(alloc);

    const principal = common.authenticate(alloc, req, ctx) catch |err| {
        common.writeAuthError(ctx, res, req_id, err);
        return;
    };

    const qs = req.query() catch {
        common.errorResponse(res, ec.ERR_INVALID_REQUEST, "malformed query string", req_id);
        return;
    };

    const limit = parseLimitFromQs(qs, LIMIT_MAX_CUSTOMER, LIMIT_DEFAULT) catch {
        common.errorResponse(res, ec.ERR_INVALID_REQUEST, "limit must be between 1 and 200", req_id);
        return;
    };
    const cursor = qs.get("cursor");

    const conn = ctx.pool.acquire() catch {
        common.internalDbUnavailable(res, req_id);
        return;
    };
    defer ctx.pool.release(conn);

    if (!common.authorizeWorkspaceAndSetTenantContext(conn, principal, workspace_id)) {
        common.errorResponse(res, ec.ERR_WORKSPACE_NOT_FOUND, "Workspace not found or access denied", req_id);
        return;
    }

    const rows = store.listTelemetryForZombie(conn, alloc, workspace_id, zombie_id, limit, cursor) catch {
        common.internalDbError(res, req_id);
        return;
    };

    const next_cursor: ?[]u8 = if (rows.len == limit and rows.len > 0)
        store.makeCursor(alloc, rows[rows.len - 1]) catch null
    else
        null;

    common.writeJson(res, .ok, .{ .items = rows, .cursor = next_cursor });
}

// ── Operator endpoint ──────────────────────────────────────────────────────

pub fn handleInternalTelemetry(
    ctx: *Context,
    req: *httpz.Request,
    res: *httpz.Response,
) void {
    var arena = std.heap.ArenaAllocator.init(ctx.alloc);
    defer arena.deinit();
    const alloc = arena.allocator();
    const req_id = common.requestId(alloc);

    _ = common.authenticate(alloc, req, ctx) catch |err| {
        common.writeAuthError(ctx, res, req_id, err);
        return;
    };

    const qs = req.query() catch {
        common.errorResponse(res, ec.ERR_INVALID_REQUEST, "malformed query string", req_id);
        return;
    };

    const limit = parseLimitFromQs(qs, LIMIT_MAX_OPERATOR, LIMIT_DEFAULT) catch {
        common.errorResponse(res, ec.ERR_INVALID_REQUEST, "limit must be between 1 and 500", req_id);
        return;
    };
    const workspace_id = qs.get("workspace_id");
    const zombie_id = qs.get("zombie_id");
    const after_ms: ?i64 = blk: {
        const raw = qs.get("after") orelse break :blk null;
        break :blk std.fmt.parseInt(i64, raw, 10) catch null;
    };

    const conn = ctx.pool.acquire() catch {
        common.internalDbUnavailable(res, req_id);
        return;
    };
    defer ctx.pool.release(conn);

    const rows = store.listTelemetryAll(conn, alloc, workspace_id, zombie_id, after_ms, limit) catch {
        common.internalDbError(res, req_id);
        return;
    };

    common.writeJson(res, .ok, .{ .items = rows });
}

// ── Shared helpers ─────────────────────────────────────────────────────────

fn parseLimitFromQs(qs: anytype, max: u32, default: u32) !u32 {
    const raw = qs.get("limit") orelse return default;
    const n = std.fmt.parseInt(u32, raw, 10) catch return error.InvalidLimit;
    if (n == 0 or n > max) return error.InvalidLimit;
    return n;
}
