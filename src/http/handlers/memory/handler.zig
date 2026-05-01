// External-agent memory API — workspace-scoped /memories collection.
//
//   POST   /v1/workspaces/{ws}/zombies/{zid}/memories          → innerStoreMemory
//                                                                body: {key, content, category?}
//   GET    /v1/workspaces/{ws}/zombies/{zid}/memories          → innerListMemories
//                                                                query: query?, category?, limit?
//   DELETE /v1/workspaces/{ws}/zombies/{zid}/memories/{key}    → innerDeleteMemory
//                                                                idempotent 204; key is path-derived
//
// Auth: bearer (workspace-scoped). The path's workspace_id is the source of
// truth — `resolveZombieInWorkspace` verifies the principal can access it and
// the zombie belongs to it before the memory_runtime SET ROLE.
//
// RULE FLS: all conn.query() calls use PgQuery with defer deinit().
// RULE NSQ: schema-qualified SQL (memory.memory_entries / core.zombies).

const std = @import("std");
const httpz = @import("httpz");
const pg = @import("pg");
const PgQuery = @import("../../../db/pg_query.zig").PgQuery;
const common = @import("../common.zig");
const hx_mod = @import("../hx.zig");
const ec = @import("../../../errors/error_registry.zig");

const h = @import("helpers.zig");
const Hx = h.Hx;
const MemoryEntry = h.MemoryEntry;

const log = std.log.scoped(.memory_http);
pub const Context = common.Context;

// ── Store ─────────────────────────────────────────────────────────────────

const StoreBody = struct {
    key: []const u8,
    content: []const u8,
    category: []const u8 = "core",
};

pub fn innerStoreMemory(
    hx: Hx,
    req: *httpz.Request,
    workspace_id: []const u8,
    zombie_id: []const u8,
) void {
    const body_raw = req.body() orelse {
        hx.fail(ec.ERR_INVALID_REQUEST, "request body required");
        return;
    };
    const parsed = std.json.parseFromSlice(StoreBody, hx.alloc, body_raw, .{ .ignore_unknown_fields = true }) catch {
        hx.fail(ec.ERR_INVALID_REQUEST, ec.MSG_MALFORMED_JSON);
        return;
    };
    const b = parsed.value;
    if (b.key.len == 0 or b.key.len > h.MAX_KEY_LEN) {
        hx.fail(ec.ERR_INVALID_REQUEST, "key must be 1-255 bytes");
        return;
    }
    if (b.content.len == 0 or b.content.len > h.MAX_CONTENT_LEN) {
        hx.fail(ec.ERR_INVALID_REQUEST, "content must be 1-16384 bytes");
        return;
    }

    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer hx.ctx.pool.release(conn);

    const instance_id = h.resolveZombieInWorkspace(hx, conn, workspace_id, zombie_id) orelse return;

    if (!h.setMemoryRole(conn)) {
        hx.fail(ec.ERR_MEM_UNAVAILABLE, "memory backend role switch failed");
        return;
    }
    defer h.resetRole(conn);

    const ts = h.nowTs(hx.alloc);
    const entry_id = h.genId(hx.alloc);
    _ = conn.exec(
        \\INSERT INTO memory.memory_entries
        \\  (id, key, content, category, instance_id, session_id, created_at, updated_at)
        \\VALUES ($1, $2, $3, $4, $5, NULL, $6, $6)
        \\ON CONFLICT (key, instance_id) DO UPDATE
        \\  SET content = EXCLUDED.content,
        \\      category = EXCLUDED.category,
        \\      updated_at = EXCLUDED.updated_at
    , .{ entry_id, b.key, b.content, b.category, instance_id, ts }) catch {
        log.warn("memory_store.failed zombie_id={s} key={s}", .{ zombie_id, b.key });
        hx.fail(ec.ERR_MEM_UNAVAILABLE, "memory store failed");
        return;
    };

    hx.ok(.created, .{
        .key = b.key,
        .category = b.category,
        .request_id = hx.req_id,
    });
}

// ── List / Search ─────────────────────────────────────────────────────────
// `?query=...` flips behaviour from list-most-recent to fuzzy LIKE search
// across both key and content. `?category=...` filters by category in the
// list path.

pub fn innerListMemories(
    hx: Hx,
    req: *httpz.Request,
    workspace_id: []const u8,
    zombie_id: []const u8,
) void {
    const qs = req.query() catch {
        hx.fail(ec.ERR_INVALID_REQUEST, "malformed query string");
        return;
    };
    const query_text: ?[]const u8 = blk: {
        const q = qs.get("query") orelse break :blk null;
        if (q.len == 0) break :blk null;
        break :blk q;
    };
    const category_opt: ?[]const u8 = blk: {
        const c = qs.get("category") orelse break :blk null;
        if (c.len == 0) break :blk null;
        break :blk c;
    };
    const default_limit: i64 = if (query_text != null) h.DEFAULT_RECALL_LIMIT else h.DEFAULT_LIST_LIMIT;
    const limit_raw = parseLimitQs(qs.get("limit"), default_limit) catch {
        hx.fail(ec.ERR_INVALID_REQUEST, "limit must be a positive integer");
        return;
    };
    const limit = @min(limit_raw, h.MAX_RECALL_LIMIT);

    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer hx.ctx.pool.release(conn);

    const instance_id = h.resolveZombieInWorkspace(hx, conn, workspace_id, zombie_id) orelse return;

    if (!h.setMemoryRole(conn)) {
        hx.fail(ec.ERR_MEM_UNAVAILABLE, "memory backend role switch failed");
        return;
    }
    defer h.resetRole(conn);

    var entries: std.ArrayList(MemoryEntry) = .{};
    defer entries.deinit(hx.alloc);

    if (query_text) |qt| {
        const escaped = h.escapeLikePattern(hx.alloc, qt) catch {
            common.internalOperationError(hx.res, "OOM", hx.req_id);
            return;
        };
        const like_pat = std.fmt.allocPrint(hx.alloc, "%{s}%", .{escaped}) catch {
            common.internalOperationError(hx.res, "OOM", hx.req_id);
            return;
        };
        var q = PgQuery.from(conn.query(
            \\SELECT key, content, category, updated_at
            \\FROM memory.memory_entries
            \\WHERE instance_id = $1
            \\  AND (key ILIKE $2 ESCAPE '\' OR content ILIKE $2 ESCAPE '\')
            \\ORDER BY updated_at DESC
            \\LIMIT $3
        , .{ instance_id, like_pat, limit }) catch {
            hx.fail(ec.ERR_MEM_UNAVAILABLE, "memory search failed");
            return;
        });
        defer q.deinit();
        h.collectEntries(hx.alloc, &q, &entries);
    } else if (category_opt) |cat| {
        var q = PgQuery.from(conn.query(
            \\SELECT key, content, category, updated_at
            \\FROM memory.memory_entries
            \\WHERE instance_id = $1 AND category = $2
            \\ORDER BY updated_at DESC LIMIT $3
        , .{ instance_id, cat, limit }) catch {
            hx.fail(ec.ERR_MEM_UNAVAILABLE, "memory list failed");
            return;
        });
        defer q.deinit();
        h.collectEntries(hx.alloc, &q, &entries);
    } else {
        var q = PgQuery.from(conn.query(
            \\SELECT key, content, category, updated_at
            \\FROM memory.memory_entries
            \\WHERE instance_id = $1
            \\ORDER BY updated_at DESC LIMIT $2
        , .{ instance_id, limit }) catch {
            hx.fail(ec.ERR_MEM_UNAVAILABLE, "memory list failed");
            return;
        });
        defer q.deinit();
        h.collectEntries(hx.alloc, &q, &entries);
    }

    hx.ok(.ok, .{
        .items = entries.items,
        .total = entries.items.len,
        .request_id = hx.req_id,
    });
}

// ── Delete (idempotent 204) ───────────────────────────────────────────────

pub fn innerDeleteMemory(
    hx: Hx,
    workspace_id: []const u8,
    zombie_id: []const u8,
    key: []const u8,
) void {
    if (key.len == 0 or key.len > h.MAX_KEY_LEN) {
        hx.fail(ec.ERR_INVALID_REQUEST, "key must be 1-255 bytes");
        return;
    }

    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer hx.ctx.pool.release(conn);

    const instance_id = h.resolveZombieInWorkspace(hx, conn, workspace_id, zombie_id) orelse return;

    if (!h.setMemoryRole(conn)) {
        hx.fail(ec.ERR_MEM_UNAVAILABLE, "memory backend role switch failed");
        return;
    }
    defer h.resetRole(conn);

    _ = conn.exec(
        "DELETE FROM memory.memory_entries WHERE key = $1 AND instance_id = $2",
        .{ key, instance_id },
    ) catch {
        hx.fail(ec.ERR_MEM_UNAVAILABLE, "memory delete failed");
        return;
    };

    hx.noContent();
}

// ── parseLimitQs ──────────────────────────────────────────────────────────

const ParseLimitError = error{ InvalidLimit, OutOfRange };

fn parseLimitQs(raw: ?[]const u8, default_value: i64) ParseLimitError!i64 {
    const s = raw orelse return default_value;
    if (s.len == 0) return default_value;
    const n = std.fmt.parseInt(i64, s, 10) catch return ParseLimitError.InvalidLimit;
    if (n < 1) return ParseLimitError.OutOfRange;
    return n;
}

test "parseLimitQs: null returns default" {
    try std.testing.expectEqual(@as(i64, 10), try parseLimitQs(null, 10));
}
test "parseLimitQs: empty string returns default" {
    try std.testing.expectEqual(@as(i64, 10), try parseLimitQs("", 10));
}
test "parseLimitQs: '25' returns 25" {
    try std.testing.expectEqual(@as(i64, 25), try parseLimitQs("25", 10));
}
test "parseLimitQs: '0' returns OutOfRange" {
    try std.testing.expectError(ParseLimitError.OutOfRange, parseLimitQs("0", 10));
}
test "parseLimitQs: '-5' returns OutOfRange" {
    try std.testing.expectError(ParseLimitError.OutOfRange, parseLimitQs("-5", 10));
}
test "parseLimitQs: 'abc' returns InvalidLimit" {
    try std.testing.expectError(ParseLimitError.InvalidLimit, parseLimitQs("abc", 10));
}
