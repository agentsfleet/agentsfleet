// External-fleet memory API — workspace-scoped /memories resource.
//
//   GET    /v1/workspaces/{ws}/fleets/{zid}/memories        → innerListMemories
//                                                              query: query?, category?, limit?
//   DELETE /v1/workspaces/{ws}/fleets/{zid}/memories/{key}  → innerDeleteMemory
//
// Memory is *stored* only by the runner-plane capture push (the single fenced
// write path, src/agentsfleetd/http/handlers/runner/memory.zig) — a tenant
// cannot author a memory. It can forget one: the operator who sees a fleet
// carrying a wrong lesson needs a way to remove it, so the tenant plane owns
// the DELETE. Collection POST stays retired (405) with no compat shim.
//
// Auth: bearer (workspace-scoped); the DELETE takes `fleet:write` — forgetting
// mutates fleet state, but it is not a lifecycle transition, so not
// `fleet:admin`. The path's workspace_id is the source of truth —
// `resolveFleetInWorkspace` verifies the principal can access it and the fleet
// belongs to it before the memory_runtime SET ROLE.
//
// RULE FLS: all conn.query() calls use PgQuery with defer deinit().
// RULE NSQ: schema-qualified SQL (memory.memory_entries / core.fleets).

const std = @import("std");
const sql = @import("sql.zig");
const httpz = @import("httpz");
const logging = @import("log");
const PgQuery = @import("../../../db/pg_query.zig").PgQuery;
const common = @import("../common.zig");
const ec = @import("../../../errors/error_registry.zig");
const fleet_memory = @import("../../../memory/fleet_memory.zig");
const metrics_memory = @import("../../../observability/metrics_memory.zig");

const h = @import("helpers.zig");
const Hx = h.Hx;
const MemoryEntry = h.MemoryEntry;

const log = logging.scoped(.memory_http);

pub const Context = common.Context;

const S_MEMORY_SEARCH_FAILED = "Failed to process the memory search";
const S_MEMORY_BACKEND_ROLE_SWITCH_FAILED = "memory backend role switch failed";
const S_MEMORY_LIST_FAILED = "memory list failed";
const S_MEMORY_ENTRY_NOT_FOUND = "No memory entry with that key";
const S_MEMORY_KEY_LENGTH_INVALID = "memory key must be 1..255 chars";

// ── List / Search ─────────────────────────────────────────────────────────
// `?query=...` flips behaviour from list-most-recent to fuzzy LIKE search
// across both key and content. `?category=...` filters by category in the
// list path.

pub fn innerListMemories(
    hx: Hx,
    req: *httpz.Request,
    workspace_id: []const u8,
    fleet_id: []const u8,
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

    const fleet_scope = h.resolveFleetInWorkspace(hx, conn, workspace_id, fleet_id) orelse return;

    if (!h.setMemoryRole(conn)) {
        hx.fail(ec.ERR_MEM_UNAVAILABLE, S_MEMORY_BACKEND_ROLE_SWITCH_FAILED);
        return;
    }
    defer h.resetRole(conn);

    var entries: std.ArrayList(MemoryEntry) = .empty;
    defer entries.deinit(hx.alloc);

    if (query_text) |qt| {
        const escaped = h.escapeLikePattern(hx.alloc, qt) catch {
            common.internalOperationError(hx.res, S_MEMORY_SEARCH_FAILED, hx.req_id);
            return;
        };
        const like_pat = std.fmt.allocPrint(hx.alloc, "%{s}%", .{escaped}) catch {
            common.internalOperationError(hx.res, S_MEMORY_SEARCH_FAILED, hx.req_id);
            return;
        };
        var q = PgQuery.from(conn.query(sql.SEARCH_ENTRIES, .{ fleet_scope, like_pat, limit }) catch {
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
        var q = PgQuery.from(conn.query(sql.SELECT_ENTRIES_IN_CATEGORY, .{ fleet_scope, cat, limit }) catch {
            hx.fail(ec.ERR_MEM_UNAVAILABLE, S_MEMORY_LIST_FAILED);
            return;
        });
        defer q.deinit();
        _ = h.collectEntries(hx.alloc, &q, &entries);
    } else {
        var q = PgQuery.from(conn.query(sql.SELECT_RECENT_ENTRIES, .{ fleet_scope, limit }) catch {
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

// ── Forget ────────────────────────────────────────────────────────────────
// The operator's correction path: a fleet learned a convention wrong, and the
// entry has to go before the next hydrate seeds it into another run.

pub fn innerDeleteMemory(
    hx: Hx,
    workspace_id: []const u8,
    fleet_id: []const u8,
    key: []const u8,
) void {
    var key_scratch: [h.MAX_KEY_LEN]u8 = undefined;
    const decoded_key = decodePathSegment(&key_scratch, key) catch |err| {
        switch (err) {
            error.InvalidEscapeSequence => hx.fail(ec.ERR_INVALID_REQUEST, "memory key has invalid URL encoding"),
            error.KeyTooLong => hx.fail(ec.ERR_INVALID_REQUEST, S_MEMORY_KEY_LENGTH_INVALID),
        }
        return;
    };
    if (decoded_key.len == 0) {
        hx.fail(ec.ERR_INVALID_REQUEST, S_MEMORY_KEY_LENGTH_INVALID);
        return;
    }

    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer hx.ctx.pool.release(conn);

    // Ownership first: a fleet in another workspace 404s here, before the role
    // switch — so no cross-workspace caller ever reaches the memory schema.
    const fleet_scope = h.resolveFleetInWorkspace(hx, conn, workspace_id, fleet_id) orelse return;

    if (!h.setMemoryRole(conn)) {
        hx.fail(ec.ERR_MEM_UNAVAILABLE, S_MEMORY_BACKEND_ROLE_SWITCH_FAILED);
        return;
    }
    defer h.resetRole(conn);

    const deleted = fleet_memory.deleteEntry(conn, fleet_scope, decoded_key) catch |err| {
        log.err("memory_forget_failed", .{ .error_code = ec.ERR_MEM_UNAVAILABLE, .err = @errorName(err), .req_id = hx.req_id });
        hx.fail(ec.ERR_MEM_UNAVAILABLE, "memory forget failed");
        return;
    };

    if (!deleted) {
        // A key that was never there is a 404, not a silent 204: an operator
        // who mistypes a key learns the entry is still in the fleet's head.
        log.info("memory_forget_missing_key", .{ .error_code = ec.ERR_MEM_ENTRY_NOT_FOUND, .fleet_id = fleet_id, .req_id = hx.req_id });
        hx.fail(ec.ERR_MEM_ENTRY_NOT_FOUND, S_MEMORY_ENTRY_NOT_FOUND);
        return;
    }

    log.debug("memory_forgotten", .{ .fleet_id = fleet_id, .req_id = hx.req_id });
    hx.noContent();
}

/// Decode one URL path segment. Unlike form/query decoding, a raw `+` in a
/// path is literal; only `%XX` escapes are transformed.
fn decodePathSegment(out: []u8, encoded: []const u8) error{ InvalidEscapeSequence, KeyTooLong }![]const u8 {
    var read: usize = 0;
    var written: usize = 0;
    while (read < encoded.len) {
        if (written >= out.len) return error.KeyTooLong;
        if (encoded[read] == '%') {
            if (read + 2 >= encoded.len) return error.InvalidEscapeSequence;
            out[written] = std.fmt.parseInt(u8, encoded[read + 1 .. read + 3], 16) catch
                return error.InvalidEscapeSequence;
            read += 3;
        } else {
            out[written] = encoded[read];
            read += 1;
        }
        written += 1;
    }
    return out[0..written];
}

test "decodePathSegment accepts exact capacity and rejects overflow" {
    var out: [3]u8 = undefined;
    try std.testing.expectEqualStrings("abc", try decodePathSegment(&out, "abc"));
    try std.testing.expectError(error.KeyTooLong, decodePathSegment(&out, "abcd"));
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
