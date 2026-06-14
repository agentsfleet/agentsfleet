//! GET /_um/<key>/cap.json — public, unauthenticated model→cap catalogue plus the
//! global, non-secret client config (run/event rates, starter credit, free-trial window).
//!
//! Both the install-skill (platform-managed posture) and `agentsfleet provider set`
//! (self-managed posture) call this endpoint exactly once at provisioning time and pin
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
//! column — tenants pick provider via a user-named credential body and
//! `tenant provider set --credential <name>`.
//!
//! Per-token rates (input_nanos_per_mtok / output_nanos_per_mtok) accompany
//! each cap row. Rates are charged only under platform-managed posture; self-managed
//! pays a flat overhead and is billed by the tenant's own provider account.
//! Models that are self-managed-only at the platform tier carry zero rates; those
//! zeros never enter the cost path because self-managed uses the flat overhead.

const std = @import("std");
const httpz = @import("httpz");
const pg = @import("pg");

const PgQuery = @import("../../db/pg_query.zig").PgQuery;
const tenant_billing = @import("../../state/tenant_billing.zig");
const common = @import("common.zig");
const hx_mod = @import("hx.zig");

const Hx = hx_mod.Hx;

/// Cryptic path-key. Hard-coded; rotation is a coordinated release.
/// 128 bits of entropy — random scanning to discover this URL is
/// cost-prohibitive. NOT a secret: shipped in `agentsfleet`, the
/// install-skill, and this binary. Obscurity, not secrecy.
pub const MODEL_CAPS_PATH_KEY = "da5b6b3810543fe108d816ee972e4ff8"; // gitleaks:allow — public path obfuscator, not a credential

/// Full URL path. Constants used by the router and by tests.
pub const MODEL_CAPS_PATH = "/_um/" ++ MODEL_CAPS_PATH_KEY ++ "/cap.json";

const ModelCap = struct {
    id: []const u8,
    provider: []const u8,
    context_cap_tokens: i32,
    input_nanos_per_mtok: i64,
    cached_input_nanos_per_mtok: i64,
    output_nanos_per_mtok: i64,
};

const Rates = struct {
    run_nanos_per_sec: i64,
    event_nanos: i64,
};

const GlobalBilling = struct {
    starter_credit_nanos: i64,
    free_trial_end_ms: i64,
    free_trial_stage_nanos: i64,
};

const ResponseBody = struct {
    version: []const u8,
    models: []const ModelCap,
    rates: Rates,
    billing: GlobalBilling,
};

/// SELECT clause shared by both list-all and filter-by-model paths.
const SELECT_ALL =
    \\SELECT model_id, provider, context_cap_tokens,
    \\       input_nanos_per_mtok, cached_input_nanos_per_mtok, output_nanos_per_mtok,
    \\       updated_at_ms
    \\  FROM core.model_caps
    \\ ORDER BY model_id
;

const SELECT_ONE =
    \\SELECT model_id, provider, context_cap_tokens,
    \\       input_nanos_per_mtok, cached_input_nanos_per_mtok, output_nanos_per_mtok,
    \\       updated_at_ms
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
    var models: std.ArrayList(ModelCap) = .empty;
    errdefer models.deinit(alloc);

    var max_updated_ms: i64 = 0;

    if (model_filter) |m| {
        var q = PgQuery.from(try conn.query(SELECT_ONE, .{m}));
        defer q.deinit();
        while (try q.next()) |row| {
            try appendRow(alloc, &models, &max_updated_ms, row);
        }
    } else {
        var q = PgQuery.from(try conn.query(SELECT_ALL, .{}));
        defer q.deinit();
        while (try q.next()) |row| {
            try appendRow(alloc, &models, &max_updated_ms, row);
        }
    }

    const version = try formatVersion(alloc, max_updated_ms);
    const cfg = tenant_billing.publicConfig();
    return .{
        .version = version,
        .models = try models.toOwnedSlice(alloc),
        .rates = .{ .run_nanos_per_sec = cfg.run_nanos_per_sec, .event_nanos = cfg.event_nanos },
        .billing = .{
            .starter_credit_nanos = cfg.starter_credit_nanos,
            .free_trial_end_ms = cfg.free_trial_end_ms,
            .free_trial_stage_nanos = cfg.free_trial_stage_nanos,
        },
    };
}

fn appendRow(
    alloc: std.mem.Allocator,
    models: *std.ArrayList(ModelCap),
    max_updated_ms: *i64,
    row: anytype,
) !void {
    const id = try alloc.dupe(u8, try row.get([]const u8, 0));
    const provider = try alloc.dupe(u8, try row.get([]const u8, 1));
    const cap = try row.get(i32, 2);
    const in_rate = try row.get(i64, 3);
    const cached_rate = try row.get(i64, 4);
    const out_rate = try row.get(i64, 5);
    const updated = try row.get(i64, 6);
    try models.append(alloc, .{
        .id = id,
        .provider = provider,
        .context_cap_tokens = cap,
        .input_nanos_per_mtok = in_rate,
        .cached_input_nanos_per_mtok = cached_rate,
        .output_nanos_per_mtok = out_rate,
    });
    if (updated > max_updated_ms.*) max_updated_ms.* = updated;
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
    try std.testing.expect(std.mem.endsWith(u8, MODEL_CAPS_PATH, "/cap.json"));
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
