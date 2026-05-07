// Shared helpers for memory HTTP handlers.
// Split from memory_http.zig for RULE FLL (350-line gate).

const std = @import("std");
const logging = @import("log");
const pg = @import("pg");
const PgQuery = @import("../../../db/pg_query.zig").PgQuery;
const common = @import("../common.zig");
const hx_mod = @import("../hx.zig");
const ec = @import("../../../errors/error_registry.zig");
const id_format = @import("../../../types/id_format.zig");

const log = logging.scoped(.memory_http_helpers);

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

/// Verify the principal can access the path workspace and the zombie
/// belongs to that workspace, then build the instance_id used by the
/// memory storage layer ("zmb:{zombie_id}", arena-allocated).
///
/// Runs under api_runtime (core.* access) before any SET ROLE.
pub fn resolveZombieInWorkspace(
    hx: Hx,
    conn: *pg.Conn,
    workspace_id: []const u8,
    zombie_id: []const u8,
) ?[]const u8 {
    if (!id_format.isUuidV7(zombie_id)) {
        hx.fail(ec.ERR_INVALID_REQUEST, "zombie_id must be a valid UUIDv7");
        return null;
    }
    if (!common.authorizeWorkspace(conn, hx.principal, workspace_id)) {
        hx.fail(ec.ERR_FORBIDDEN, "Workspace access denied");
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

    const zombie_ws = row.get([]const u8, 0) catch {
        common.internalDbError(hx.res, hx.req_id);
        return null;
    };
    if (!std.mem.eql(u8, workspace_id, zombie_ws)) {
        // 404 not 403 — don't leak existence across workspaces.
        hx.fail(ec.ERR_MEM_ZOMBIE_NOT_FOUND, "zombie not found");
        return null;
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
/// On failure, the connection is unusable (still running as memory_runtime) —
/// log.err so operators can see pool poisoning in logs before the pool reaps it.
pub fn resetRole(conn: *pg.Conn) void {
    _ = conn.exec("RESET ROLE", .{}) catch |err| {
        log.err("reset_role_failed", .{
            .error_code = ec.ERR_INTERNAL_DB_QUERY,
            .err = @errorName(err),
            .hint = "connection_will_be_discarded_by_pool",
        });
    };
}

/// Escape LIKE metacharacters (%, _, \) with backslash for use with ESCAPE '\'.
/// Returns an arena-allocated string safe for use in ILIKE $1 ESCAPE '\'.
pub fn escapeLikePattern(alloc: std.mem.Allocator, input: []const u8) error{OutOfMemory}![]const u8 {
    // Count metacharacters to size the output buffer.
    var extra: usize = 0;
    for (input) |c| if (c == '%' or c == '_' or c == '\\') {
        extra += 1;
    };
    const out = try alloc.alloc(u8, input.len + extra);
    var i: usize = 0;
    for (input) |c| {
        if (c == '%' or c == '_' or c == '\\') {
            out[i] = '\\';
            i += 1;
        }
        out[i] = c;
        i += 1;
    }
    return out;
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
/// Stops on the first OOM error — partial results returned with a warn log so
/// operators can detect silent truncation (HTTP 200 with fewer entries than available).
pub fn collectEntries(
    alloc: std.mem.Allocator,
    q: *PgQuery,
    entries: *std.ArrayList(MemoryEntry),
) void {
    collect: while (true) {
        const row = q.next() catch break :collect;
        const r = row orelse break :collect;
        const key = alloc.dupe(u8, r.get([]const u8, 0) catch continue) catch {
            log.warn("collect_truncated", .{ .error_code = ec.ERR_INTERNAL_OPERATION_FAILED, .reason = "oom_key", .collected = entries.items.len });
            break :collect;
        };
        const content = alloc.dupe(u8, r.get([]const u8, 1) catch continue) catch {
            log.warn("collect_truncated", .{ .error_code = ec.ERR_INTERNAL_OPERATION_FAILED, .reason = "oom_content", .collected = entries.items.len });
            break :collect;
        };
        const category = alloc.dupe(u8, r.get([]const u8, 2) catch continue) catch {
            log.warn("collect_truncated", .{ .error_code = ec.ERR_INTERNAL_OPERATION_FAILED, .reason = "oom_category", .collected = entries.items.len });
            break :collect;
        };
        const updated_at = alloc.dupe(u8, r.get([]const u8, 3) catch continue) catch {
            log.warn("collect_truncated", .{ .error_code = ec.ERR_INTERNAL_OPERATION_FAILED, .reason = "oom_updated_at", .collected = entries.items.len });
            break :collect;
        };
        entries.append(alloc, .{
            .key = key,
            .content = content,
            .category = category,
            .updated_at = updated_at,
        }) catch {
            log.warn("collect_truncated", .{ .error_code = ec.ERR_INTERNAL_OPERATION_FAILED, .reason = "oom_append", .collected = entries.items.len });
            break :collect;
        };
    }
}
