// M14_001: Shared helpers for memory HTTP handlers.
// Split from memory_http.zig for RULE FLL (350-line gate).

const std = @import("std");
const pg = @import("pg");
const PgQuery = @import("../../db/pg_query.zig").PgQuery;
const common = @import("common.zig");
const hx_mod = @import("hx.zig");
const ec = @import("../../errors/error_registry.zig");
const id_format = @import("../../types/id_format.zig");

pub const Hx = hx_mod.Hx;

pub const MAX_KEY_LEN: usize = 255;
pub const MAX_CONTENT_LEN: usize = 16 * 1024; // 16KB
pub const MAX_RECALL_LIMIT: i64 = 100;
pub const DEFAULT_RECALL_LIMIT: i64 = 20;
pub const DEFAULT_LIST_LIMIT: i64 = 100;

pub const MemoryEntry = struct {
    key: []const u8,
    content: []const u8,
    category: []const u8,
    updated_at: []const u8,
};

/// Verify zombie_id exists and belongs to the caller's workspace.
/// Runs under api_runtime (has access to core.*), before any SET ROLE.
/// Returns instance_id ("zmb:{zombie_id}") allocated in alloc, or null on failure.
pub fn resolveInstanceId(
    hx: Hx,
    conn: *pg.Conn,
    zombie_id: []const u8,
) ?[]const u8 {
    if (!id_format.isUuidV7(zombie_id)) {
        hx.fail(ec.ERR_INVALID_REQUEST, "zombie_id must be a valid UUIDv7");
        return null;
    }
    var q = PgQuery.from(conn.query(
        "SELECT workspace_id::text FROM core.zombies WHERE id = $1::uuid",
        .{zombie_id},
    ) catch {
        common.internalDbError(hx.res, hx.req_id);
        return null;
    });
    defer q.deinit();

    const row = (q.next() catch {
        common.internalDbError(hx.res, hx.req_id);
        return null;
    }) orelse {
        hx.fail(ec.ERR_MEM_ZOMBIE_NOT_FOUND, "zombie not found");
        return null;
    };

    const workspace_id = row.get([]const u8, 0) catch {
        common.internalDbError(hx.res, hx.req_id);
        return null;
    };

    if (hx.principal.workspace_scope_id) |scope| {
        if (!std.mem.eql(u8, scope, workspace_id)) {
            hx.fail(ec.ERR_MEM_SCOPE, "zombie belongs to a different workspace");
            return null;
        }
    }

    return std.fmt.allocPrint(hx.alloc, "zmb:{s}", .{zombie_id}) catch {
        common.internalOperationError(hx.res, "OOM building instance_id", hx.req_id);
        return null;
    };
}

/// SET ROLE memory_runtime. Returns false and caller must abort on failure.
pub fn setMemoryRole(conn: *pg.Conn) bool {
    _ = conn.exec("SET ROLE memory_runtime", .{}) catch return false;
    return true;
}

/// RESET ROLE. Call via defer after setMemoryRole succeeds.
pub fn resetRole(conn: *pg.Conn) void {
    _ = conn.exec("RESET ROLE", .{}) catch {};
}

/// Current Unix timestamp as a decimal string, arena-allocated.
pub fn nowTs(alloc: std.mem.Allocator) []const u8 {
    return std.fmt.allocPrint(alloc, "{d}", .{std.time.timestamp()}) catch "0";
}

/// Generate a NullClaw-compatible memory entry ID.
pub fn genId(alloc: std.mem.Allocator) []const u8 {
    const ts = std.time.nanoTimestamp();
    var buf: [16]u8 = undefined;
    std.crypto.random.bytes(&buf);
    const hi = std.mem.readInt(u64, buf[0..8], .little);
    const lo = std.mem.readInt(u64, buf[8..16], .little);
    return std.fmt.allocPrint(alloc, "{d}-{x}-{x}", .{ ts, hi, lo }) catch "fallback-id";
}

/// Drain a PgQuery result into an ArrayList(MemoryEntry), arena-allocated.
/// Stops on the first drain or OOM error — partial results are fine.
pub fn collectEntries(
    alloc: std.mem.Allocator,
    q: *PgQuery,
    entries: *std.ArrayList(MemoryEntry),
) void {
    collect: while (true) {
        const row = q.next() catch break :collect;
        const r = row orelse break :collect;
        const key = alloc.dupe(u8, r.get([]const u8, 0) catch continue) catch break :collect;
        const content = alloc.dupe(u8, r.get([]const u8, 1) catch continue) catch break :collect;
        const category = alloc.dupe(u8, r.get([]const u8, 2) catch continue) catch break :collect;
        const updated_at = alloc.dupe(u8, r.get([]const u8, 3) catch continue) catch break :collect;
        entries.append(alloc, .{
            .key = key,
            .content = content,
            .category = category,
            .updated_at = updated_at,
        }) catch break :collect;
    }
}
