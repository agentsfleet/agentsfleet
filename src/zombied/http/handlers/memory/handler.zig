// External-agent memory API — workspace-scoped /memories collection (READ-ONLY).
//
//   GET    /v1/workspaces/{ws}/zombies/{zid}/memories          → innerListMemories
//                                                                query: query?, category?, limit?
//
// The write verbs (POST store / DELETE by-key) are retired: durable memory is
// written only by the runner-plane capture push (the single fenced write path,
// src/zombied/http/handlers/runner/memory.zig). Retired verbs answer 405
// (collection POST) / 404 (by-key DELETE) with no compat shim.
//
// Auth: bearer (workspace-scoped). The path's workspace_id is the source of
// truth — `resolveZombieInWorkspace` verifies the principal can access it and
// the zombie belongs to it before the memory_runtime SET ROLE.
//
// RULE FLS: all conn.query() calls use PgQuery with defer deinit().
// RULE NSQ: schema-qualified SQL (memory.memory_entries / core.zombies).

const std = @import("std");
const httpz = @import("httpz");
const PgQuery = @import("../../../db/pg_query.zig").PgQuery;
const common = @import("../common.zig");
const ec = @import("../../../errors/error_registry.zig");
const metrics_memory = @import("../../../observability/metrics_memory.zig");

const h = @import("helpers.zig");
const Hx = h.Hx;
const MemoryEntry = h.MemoryEntry;

pub const Context = common.Context;

const S_OOM = "OOM";
const S_MEMORY_BACKEND_ROLE_SWITCH_FAILED = "memory backend role switch failed";
const S_MEMORY_LIST_FAILED = "memory list failed";

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

    const zombie_scope = h.resolveZombieInWorkspace(hx, conn, workspace_id, zombie_id) orelse return;

    if (!h.setMemoryRole(conn)) {
        hx.fail(ec.ERR_MEM_UNAVAILABLE, S_MEMORY_BACKEND_ROLE_SWITCH_FAILED);
        return;
    }
    defer h.resetRole(conn);

    var entries: std.ArrayList(MemoryEntry) = .empty;
    defer entries.deinit(hx.alloc);

    if (query_text) |qt| {
        const escaped = h.escapeLikePattern(hx.alloc, qt) catch {
            common.internalOperationError(hx.res, S_OOM, hx.req_id);
            return;
        };
        const like_pat = std.fmt.allocPrint(hx.alloc, "%{s}%", .{escaped}) catch {
            common.internalOperationError(hx.res, S_OOM, hx.req_id);
            return;
        };
        var q = PgQuery.from(conn.query(
            \\SELECT key, content, category, updated_at
            \\FROM memory.memory_entries
            \\WHERE zombie_id = $1::uuid
            \\  AND (key ILIKE $2 ESCAPE '\' OR content ILIKE $2 ESCAPE '\')
            \\ORDER BY updated_at DESC, id DESC
            \\LIMIT $3
        , .{ zombie_scope, like_pat, limit }) catch {
            hx.fail(ec.ERR_MEM_UNAVAILABLE, "memory search failed");
            return;
        });
        defer q.deinit();
        const clean = h.collectEntries(hx.alloc, &q, &entries);
        // A search that matched nothing is a recall-miss signal (substring
        // search came up dry) — the list/category paths never count here, and
        // neither does a truncated collect: a database blip or OOM must not
        // fabricate recall-miss evidence.
        if (clean and entries.items.len == 0) metrics_memory.incSearchZeroHit();
    } else if (category_opt) |cat| {
        var q = PgQuery.from(conn.query(
            \\SELECT key, content, category, updated_at
            \\FROM memory.memory_entries
            \\WHERE zombie_id = $1::uuid AND category = $2
            \\ORDER BY updated_at DESC, id DESC LIMIT $3
        , .{ zombie_scope, cat, limit }) catch {
            hx.fail(ec.ERR_MEM_UNAVAILABLE, S_MEMORY_LIST_FAILED);
            return;
        });
        defer q.deinit();
        _ = h.collectEntries(hx.alloc, &q, &entries);
    } else {
        var q = PgQuery.from(conn.query(
            \\SELECT key, content, category, updated_at
            \\FROM memory.memory_entries
            \\WHERE zombie_id = $1::uuid
            \\ORDER BY updated_at DESC, id DESC LIMIT $2
        , .{ zombie_scope, limit }) catch {
            hx.fail(ec.ERR_MEM_UNAVAILABLE, S_MEMORY_LIST_FAILED);
            return;
        });
        defer q.deinit();
        _ = h.collectEntries(hx.alloc, &q, &entries);
    }

    hx.ok(.ok, .{
        .items = entries.items,
        .total = entries.items.len,
        .request_id = hx.req_id,
    });
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
