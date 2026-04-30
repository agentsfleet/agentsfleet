//! GET /_um/<key>/model-caps.json — public, unauthenticated model→cap catalogue.
//!
//! Both the install-skill (platform-managed posture) and `zombiectl provider set`
//! (BYOK posture) call this endpoint exactly once at provisioning time and pin
//! the cap into the right place. The runtime never reads it on the hot path.
//!
//! The cryptic path-key prefix is for opportunistic-crawler deflection, not
//! security. The catalogue is public information; the prefix just keeps random
//! `/health`-style probes off a hot, unauthenticated lookup. Rotation is a
//! coordinated CLI + skill + API release.
//!
//! Provider hosting is encoded in the model_id itself
//! (`accounts/fireworks/...` is Fireworks; bare `kimi-k2.6` is Moonshot;
//! `claude-*` is Anthropic; etc.). The catalogue does not carry a provider
//! column — operators pick provider via their `llm` credential body.

const std = @import("std");
const httpz = @import("httpz");
const pg = @import("pg");

const PgQuery = @import("../../db/pg_query.zig").PgQuery;
const common = @import("common.zig");
const hx_mod = @import("hx.zig");

const Hx = hx_mod.Hx;

/// Cryptic path-key. Hard-coded; rotation is a coordinated release.
/// 128 bits of entropy — random scanning to discover this URL is
/// cost-prohibitive. NOT a secret: shipped in `zombiectl`, the
/// install-skill, and this binary. Obscurity, not secrecy.
pub const MODEL_CAPS_PATH_KEY = "da5b6b3810543fe108d816ee972e4ff8"; // gitleaks:allow — public path obfuscator, not a credential

/// Full URL path. Constants used by the router and by tests.
pub const MODEL_CAPS_PATH = "/_um/" ++ MODEL_CAPS_PATH_KEY ++ "/model-caps.json";

const ModelCap = struct {
    id: []const u8,
    context_cap_tokens: i32,
};

const ResponseBody = struct {
    version: []const u8,
    models: []const ModelCap,
};

/// SELECT clause shared by both list-all and filter-by-model paths.
const SELECT_ALL =
    \\SELECT model_id, context_cap_tokens, updated_at_ms
    \\  FROM core.model_caps
    \\ ORDER BY model_id
;

const SELECT_ONE =
    \\SELECT model_id, context_cap_tokens, updated_at_ms
    \\  FROM core.model_caps
    \\ WHERE model_id = $1
;

pub fn innerGetModelCaps(hx: Hx, req: *httpz.Request) void {
    if (req.method != .GET) {
        common.respondMethodNotAllowed(hx.res);
        return;
    }

    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer hx.ctx.pool.release(conn);

    const filter_model = req.query() catch null;
    const model_param: ?[]const u8 = if (filter_model) |q| q.get("model") else null;

    const body = buildResponse(hx.alloc, conn, model_param) catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };

    if (body.models.len == 0 and model_param == null) {
        // Empty catalogue with no filter = migration didn't seed; treat as
        // unhealthy so the operator notices the install isn't complete.
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    }

    hx.ok(.ok, body);
}

fn buildResponse(
    alloc: std.mem.Allocator,
    conn: *pg.Conn,
    model_filter: ?[]const u8,
) !ResponseBody {
    var models: std.ArrayList(ModelCap) = .{};
    errdefer models.deinit(alloc);

    var max_updated_ms: i64 = 0;

    if (model_filter) |m| {
        var q = PgQuery.from(try conn.query(SELECT_ONE, .{m}));
        defer q.deinit();
        while (try q.next()) |row| {
            const id = try alloc.dupe(u8, try row.get([]const u8, 0));
            const cap = try row.get(i32, 1);
            const updated = try row.get(i64, 2);
            try models.append(alloc, .{ .id = id, .context_cap_tokens = cap });
            if (updated > max_updated_ms) max_updated_ms = updated;
        }
    } else {
        var q = PgQuery.from(try conn.query(SELECT_ALL, .{}));
        defer q.deinit();
        while (try q.next()) |row| {
            const id = try alloc.dupe(u8, try row.get([]const u8, 0));
            const cap = try row.get(i32, 1);
            const updated = try row.get(i64, 2);
            try models.append(alloc, .{ .id = id, .context_cap_tokens = cap });
            if (updated > max_updated_ms) max_updated_ms = updated;
        }
    }

    const version = try formatVersion(alloc, max_updated_ms);
    return .{
        .version = version,
        .models = try models.toOwnedSlice(alloc),
    };
}

/// Format the maximum updated_at_ms as YYYY-MM-DD (UTC). Empty result set
/// produces "1970-01-01" — the empty-catalogue branch in the handler 503s
/// before the response goes out, so the value is never actually visible.
fn formatVersion(alloc: std.mem.Allocator, max_updated_ms: i64) ![]const u8 {
    const seconds: i64 = @divTrunc(max_updated_ms, 1000);
    const epoch_seconds = std.time.epoch.EpochSeconds{ .secs = @intCast(@max(seconds, 0)) };
    const epoch_day = epoch_seconds.getEpochDay();
    const year_day = epoch_day.calculateYearDay();
    const month_day = year_day.calculateMonthDay();
    return std.fmt.allocPrint(alloc, "{d:0>4}-{d:0>2}-{d:0>2}", .{
        year_day.year,
        @intFromEnum(month_day.month),
        month_day.day_index + 1,
    });
}

// ── Tests ─────────────────────────────────────────────────────────────────

test "MODEL_CAPS_PATH constant is well-formed" {
    try std.testing.expect(std.mem.startsWith(u8, MODEL_CAPS_PATH, "/_um/"));
    try std.testing.expect(std.mem.endsWith(u8, MODEL_CAPS_PATH, "/model-caps.json"));
    try std.testing.expectEqual(@as(usize, 32), MODEL_CAPS_PATH_KEY.len);
}

test "formatVersion: epoch ms renders as YYYY-MM-DD UTC" {
    // 1745884800000 ms = 2025-04-29 00:00 UTC (the seed timestamp)
    const v = try formatVersion(std.testing.allocator, 1745884800000);
    defer std.testing.allocator.free(v);
    try std.testing.expectEqualStrings("2025-04-29", v);
}

test "formatVersion: zero / negative epoch clamps to 1970-01-01" {
    const v0 = try formatVersion(std.testing.allocator, 0);
    defer std.testing.allocator.free(v0);
    try std.testing.expectEqualStrings("1970-01-01", v0);

    const vn = try formatVersion(std.testing.allocator, -1);
    defer std.testing.allocator.free(vn);
    try std.testing.expectEqualStrings("1970-01-01", vn);
}
