// M14_001: External-agent memory API — Path B HTTP endpoints.
//
// POST /v1/memory/store   → handleMemoryStore
// POST /v1/memory/recall  → handleMemoryRecall
// POST /v1/memory/list    → handleMemoryList
// POST /v1/memory/forget  → handleMemoryForget
//
// Auth: Bearer token (API key or JWT with workspace scope).
// Scope: zombie_id in body must belong to the caller's workspace.
// Role: queries run under memory_runtime (SET ROLE) for RULE CTX isolation.
//   Zombie ownership check runs first, under api_runtime (core.* access).
//   api_runtime must be a member of memory_runtime — see schema/026.
//
// Helpers extracted to memory_http_helpers.zig for RULE FLL.

const std = @import("std");
const httpz = @import("httpz");
const pg = @import("pg");
const PgQuery = @import("../../db/pg_query.zig").PgQuery;
const common = @import("common.zig");
const hx_mod = @import("hx.zig");
const ec = @import("../../errors/error_registry.zig");

const h = @import("memory_http_helpers.zig");
const Hx = h.Hx;
const MemoryEntry = h.MemoryEntry;

const log = std.log.scoped(.memory_http);
pub const Context = common.Context;

// ── Store ─────────────────────────────────────────────────────────────────────

const StoreBody = struct {
    zombie_id: []const u8,
    key: []const u8,
    content: []const u8,
    category: []const u8 = "core",
};

fn innerMemoryStore(hx: Hx, req: *httpz.Request) void {
    const body_raw = req.body() orelse {
        hx.fail(ec.ERR_INVALID_REQUEST, "request body required");
        return;
    };
    const parsed = std.json.parseFromSlice(StoreBody, hx.alloc, body_raw, .{ .ignore_unknown_fields = true }) catch {
        hx.fail(ec.ERR_INVALID_REQUEST, "malformed JSON");
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

    const instance_id = h.resolveInstanceId(hx, conn, b.zombie_id) orelse return;

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
        log.warn("memory_store.failed zombie_id={s} key={s}", .{ b.zombie_id, b.key });
        hx.fail(ec.ERR_MEM_UNAVAILABLE, "memory store failed");
        return;
    };

    hx.ok(.created, .{
        .zombie_id = b.zombie_id,
        .key = b.key,
        .category = b.category,
        .request_id = hx.req_id,
    });
}

pub const handleMemoryStore = hx_mod.authenticated(innerMemoryStore);

// ── Recall ────────────────────────────────────────────────────────────────────

const RecallBody = struct {
    zombie_id: []const u8,
    query: []const u8,
    limit: ?i64 = null,
};

fn innerMemoryRecall(hx: Hx, req: *httpz.Request) void {
    const body_raw = req.body() orelse {
        hx.fail(ec.ERR_INVALID_REQUEST, "request body required");
        return;
    };
    const parsed = std.json.parseFromSlice(RecallBody, hx.alloc, body_raw, .{ .ignore_unknown_fields = true }) catch {
        hx.fail(ec.ERR_INVALID_REQUEST, "malformed JSON");
        return;
    };
    const b = parsed.value;
    if (b.query.len == 0) {
        hx.fail(ec.ERR_INVALID_REQUEST, "query must be non-empty");
        return;
    }
    const limit = @min(b.limit orelse h.DEFAULT_RECALL_LIMIT, h.MAX_RECALL_LIMIT);
    const like_pat = std.fmt.allocPrint(hx.alloc, "%{s}%", .{b.query}) catch {
        common.internalOperationError(hx.res, "OOM", hx.req_id);
        return;
    };

    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer hx.ctx.pool.release(conn);

    const instance_id = h.resolveInstanceId(hx, conn, b.zombie_id) orelse return;

    if (!h.setMemoryRole(conn)) {
        hx.fail(ec.ERR_MEM_UNAVAILABLE, "memory backend role switch failed");
        return;
    }
    defer h.resetRole(conn);

    var q = PgQuery.from(conn.query(
        \\SELECT key, content, category, updated_at
        \\FROM memory.memory_entries
        \\WHERE instance_id = $1
        \\  AND (key ILIKE $2 OR content ILIKE $2)
        \\ORDER BY updated_at DESC
        \\LIMIT $3
    , .{ instance_id, like_pat, limit }) catch {
        hx.fail(ec.ERR_MEM_UNAVAILABLE, "memory recall failed");
        return;
    });
    defer q.deinit();

    var entries: std.ArrayList(MemoryEntry) = .{};
    defer entries.deinit(hx.alloc);
    h.collectEntries(hx.alloc, &q, &entries);

    hx.ok(.ok, .{
        .zombie_id = b.zombie_id,
        .entries = entries.items,
        .count = entries.items.len,
        .request_id = hx.req_id,
    });
}

pub const handleMemoryRecall = hx_mod.authenticated(innerMemoryRecall);

// ── List ──────────────────────────────────────────────────────────────────────

const ListBody = struct {
    zombie_id: []const u8,
    category: ?[]const u8 = null,
    limit: ?i64 = null,
};

fn innerMemoryList(hx: Hx, req: *httpz.Request) void {
    const body_raw = req.body() orelse {
        hx.fail(ec.ERR_INVALID_REQUEST, "request body required");
        return;
    };
    const parsed = std.json.parseFromSlice(ListBody, hx.alloc, body_raw, .{ .ignore_unknown_fields = true }) catch {
        hx.fail(ec.ERR_INVALID_REQUEST, "malformed JSON");
        return;
    };
    const b = parsed.value;
    const limit = @min(b.limit orelse h.DEFAULT_LIST_LIMIT, h.MAX_RECALL_LIMIT);

    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer hx.ctx.pool.release(conn);

    const instance_id = h.resolveInstanceId(hx, conn, b.zombie_id) orelse return;

    if (!h.setMemoryRole(conn)) {
        hx.fail(ec.ERR_MEM_UNAVAILABLE, "memory backend role switch failed");
        return;
    }
    defer h.resetRole(conn);

    const has_category = b.category != null and b.category.?.len > 0;
    var q = if (has_category)
        PgQuery.from(conn.query(
            \\SELECT key, content, category, updated_at
            \\FROM memory.memory_entries
            \\WHERE instance_id = $1 AND category = $2
            \\ORDER BY updated_at DESC LIMIT $3
        , .{ instance_id, b.category.?, limit }) catch {
            hx.fail(ec.ERR_MEM_UNAVAILABLE, "memory list failed");
            return;
        })
    else
        PgQuery.from(conn.query(
            \\SELECT key, content, category, updated_at
            \\FROM memory.memory_entries
            \\WHERE instance_id = $1
            \\ORDER BY updated_at DESC LIMIT $2
        , .{ instance_id, limit }) catch {
            hx.fail(ec.ERR_MEM_UNAVAILABLE, "memory list failed");
            return;
        });
    defer q.deinit();

    var entries: std.ArrayList(MemoryEntry) = .{};
    defer entries.deinit(hx.alloc);
    h.collectEntries(hx.alloc, &q, &entries);

    hx.ok(.ok, .{
        .zombie_id = b.zombie_id,
        .entries = entries.items,
        .count = entries.items.len,
        .request_id = hx.req_id,
    });
}

pub const handleMemoryList = hx_mod.authenticated(innerMemoryList);

// ── Forget ────────────────────────────────────────────────────────────────────

const ForgetBody = struct {
    zombie_id: []const u8,
    key: []const u8,
};

fn innerMemoryForget(hx: Hx, req: *httpz.Request) void {
    const body_raw = req.body() orelse {
        hx.fail(ec.ERR_INVALID_REQUEST, "request body required");
        return;
    };
    const parsed = std.json.parseFromSlice(ForgetBody, hx.alloc, body_raw, .{ .ignore_unknown_fields = true }) catch {
        hx.fail(ec.ERR_INVALID_REQUEST, "malformed JSON");
        return;
    };
    const b = parsed.value;
    if (b.key.len == 0 or b.key.len > h.MAX_KEY_LEN) {
        hx.fail(ec.ERR_INVALID_REQUEST, "key must be 1-255 bytes");
        return;
    }

    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer hx.ctx.pool.release(conn);

    const instance_id = h.resolveInstanceId(hx, conn, b.zombie_id) orelse return;

    if (!h.setMemoryRole(conn)) {
        hx.fail(ec.ERR_MEM_UNAVAILABLE, "memory backend role switch failed");
        return;
    }
    defer h.resetRole(conn);

    // RULE FLS: DELETE ... RETURNING uses conn.query (returns rows), not conn.exec.
    var q = PgQuery.from(conn.query(
        "DELETE FROM memory.memory_entries WHERE key = $1 AND instance_id = $2 RETURNING id",
        .{ b.key, instance_id },
    ) catch {
        hx.fail(ec.ERR_MEM_UNAVAILABLE, "memory forget failed");
        return;
    });
    defer q.deinit();

    var deleted = false;
    if (q.next() catch null) |_| deleted = true;

    hx.ok(.ok, .{
        .zombie_id = b.zombie_id,
        .key = b.key,
        .deleted = deleted,
        .request_id = hx.req_id,
    });
}

pub const handleMemoryForget = hx_mod.authenticated(innerMemoryForget);
