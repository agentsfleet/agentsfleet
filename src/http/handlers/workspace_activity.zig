// M12_001: Workspace-wide activity stream — GET /v1/workspaces/{ws}/activity
//
// Backs the operator dashboard "Recent Activity" feed. Merges events from every
// zombie in the workspace, ordered newest-first. Cursor-based pagination via
// (created_at, id) composite — same cursor contract as the per-zombie stream.
//
// Auth: bearer policy; workspace scope verified by common.authorizeWorkspace.
// Data: core.activity_events via activity_stream.queryByWorkspaceOnConn (RULE CNX:
// single pool connection per request).

const std = @import("std");
const httpz = @import("httpz");

const common = @import("common.zig");
const hx_mod = @import("hx.zig");
const ec = @import("../../errors/error_registry.zig");
const id_format = @import("../../types/id_format.zig");
const activity_stream = @import("../../zombie/activity_stream.zig");

const log = std.log.scoped(.workspace_activity);

pub fn innerListWorkspaceActivity(hx: hx_mod.Hx, req: *httpz.Request, workspace_id: []const u8) void {
    if (!id_format.isSupportedWorkspaceId(workspace_id)) {
        hx.fail(ec.ERR_INVALID_REQUEST, ec.MSG_WORKSPACE_ID_REQUIRED);
        return;
    }

    const qs = req.query() catch null;
    const limit = if (qs) |q| parseLimitFromQs(q) else activity_stream.DEFAULT_ACTIVITY_PAGE_LIMIT;
    const cursor = if (qs) |q| q.get("cursor") else null;

    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer hx.ctx.pool.release(conn);

    if (!common.authorizeWorkspace(conn, hx.principal, workspace_id)) {
        hx.fail(ec.ERR_FORBIDDEN, "Workspace access denied");
        return;
    }

    const page = activity_stream.queryByWorkspaceOnConn(conn, hx.alloc, workspace_id, cursor, limit) catch |err| {
        if (err == error.InvalidCursor) {
            hx.fail(ec.ERR_INVALID_REQUEST, "Invalid cursor format");
            return;
        }
        log.err("workspace_activity.list_failed err={s} workspace_id={s} req_id={s}", .{ @errorName(err), workspace_id, hx.req_id });
        common.internalDbError(hx.res, hx.req_id);
        return;
    };
    defer page.deinit(hx.alloc);

    hx.ok(.ok, .{ .events = page.events, .next_cursor = page.next_cursor });
}

fn parseLimitFromQs(qs: anytype) u32 {
    const limit_str = qs.get("limit") orelse return activity_stream.DEFAULT_ACTIVITY_PAGE_LIMIT;
    const parsed = std.fmt.parseInt(u32, limit_str, 10) catch return activity_stream.DEFAULT_ACTIVITY_PAGE_LIMIT;
    if (parsed == 0) return activity_stream.DEFAULT_ACTIVITY_PAGE_LIMIT;
    return @min(parsed, activity_stream.MAX_ACTIVITY_PAGE_LIMIT);
}

// ── Unit tests ────────────────────────────────────────────────────────────
//
// parseLimitFromQs is generic over the query-string container (`anytype`) so
// we can test it with a minimal test-only shim. Integration coverage for the
// handler itself lives in workspace_activity_integration_test.zig.

const TestQs = struct {
    values: std.StringHashMap([]const u8),
    pub fn init(alloc: std.mem.Allocator) TestQs {
        return .{ .values = std.StringHashMap([]const u8).init(alloc) };
    }
    pub fn deinit(self: *TestQs) void {
        self.values.deinit();
    }
    pub fn put(self: *TestQs, k: []const u8, v: []const u8) !void {
        try self.values.put(k, v);
    }
    pub fn get(self: TestQs, k: []const u8) ?[]const u8 {
        return self.values.get(k);
    }
};

test "parseLimitFromQs: missing limit returns default" {
    var qs = TestQs.init(std.testing.allocator);
    defer qs.deinit();
    try std.testing.expectEqual(activity_stream.DEFAULT_ACTIVITY_PAGE_LIMIT, parseLimitFromQs(qs));
}

test "parseLimitFromQs: malformed limit falls back to default" {
    var qs = TestQs.init(std.testing.allocator);
    defer qs.deinit();
    try qs.put("limit", "not-a-number");
    try std.testing.expectEqual(activity_stream.DEFAULT_ACTIVITY_PAGE_LIMIT, parseLimitFromQs(qs));
}

test "parseLimitFromQs: zero falls back to default (silently, for GET idempotence)" {
    var qs = TestQs.init(std.testing.allocator);
    defer qs.deinit();
    try qs.put("limit", "0");
    try std.testing.expectEqual(activity_stream.DEFAULT_ACTIVITY_PAGE_LIMIT, parseLimitFromQs(qs));
}

test "parseLimitFromQs: within-range value is honoured" {
    var qs = TestQs.init(std.testing.allocator);
    defer qs.deinit();
    try qs.put("limit", "50");
    try std.testing.expectEqual(@as(u32, 50), parseLimitFromQs(qs));
}

test "parseLimitFromQs: over-cap value is clamped to MAX" {
    var qs = TestQs.init(std.testing.allocator);
    defer qs.deinit();
    try qs.put("limit", "999");
    try std.testing.expectEqual(activity_stream.MAX_ACTIVITY_PAGE_LIMIT, parseLimitFromQs(qs));
}

test "parseLimitFromQs: negative-looking value falls back (u32 parseInt rejects '-')" {
    var qs = TestQs.init(std.testing.allocator);
    defer qs.deinit();
    try qs.put("limit", "-5");
    try std.testing.expectEqual(activity_stream.DEFAULT_ACTIVITY_PAGE_LIMIT, parseLimitFromQs(qs));
}
