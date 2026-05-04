//! Zombie execution telemetry HTTP handler (customer-scoped).
//!
//! GET /v1/workspaces/{ws}/zombies/{id}/telemetry?limit=50&cursor=
//!
//! Auth: bearer policy; principal is set by the middleware chain;
//! workspace scope enforced via authorizeWorkspaceAndSetTenantContext.

const std = @import("std");
const httpz = @import("httpz");
const pg = @import("pg");

const common = @import("../common.zig");
const hx_mod = @import("../hx.zig");
const ec = @import("../../../errors/error_registry.zig");
const store = @import("../../../state/zombie_telemetry_store.zig");

const log = std.log.scoped(.http_zombie_telemetry);

pub const Context = common.Context;

const LIMIT_DEFAULT: u32 = 50;
const LIMIT_MAX_CUSTOMER: u32 = 200;

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

// ── Shared helpers ─────────────────────────────────────────────────────────

fn parseLimitFromQs(qs: anytype, max: u32, default: u32) !u32 {
    const raw = qs.get("limit") orelse return default;
    const n = std.fmt.parseInt(u32, raw, 10) catch return error.InvalidLimit;
    if (n == 0 or n > max) return error.InvalidLimit;
    return n;
}
