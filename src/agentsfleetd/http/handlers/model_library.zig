//! GET /v1/models — the model library catalogue (core.model_library), served
//! to any authenticated tenant. The dashboard Models page is the only
//! consumer: its pickers fetch the catalogue once per session through a
//! token-minting Server Action. The catalogue prices the platform's billing
//! spine and has no anonymous consumer — reads require an authenticated tenant.
//!
//! Provider hosting is encoded in the model_id itself
//! (`accounts/fireworks/...` is Fireworks; bare `kimi-k2.6` is Moonshot;
//! `claude-*` is Anthropic; etc.). Tenants pick provider via a user-named
//! credential body and `tenant provider set --credential <name>`.
//!
//! Per-token rates (input_nanos_per_mtok / output_nanos_per_mtok) accompany
//! each row. Rates are charged only under platform-managed posture;
//! self-managed pays a flat overhead and is billed by the tenant's own
//! provider account. Models that are self-managed-only at the platform tier
//! carry zero rates; those zeros never enter the cost path because
//! self-managed uses the flat overhead.

const std = @import("std");
const pg = @import("pg");

const model_library_store = @import("../../state/model_library_store.zig");
const common = @import("common.zig");
const hx_mod = @import("hx.zig");

const Hx = hx_mod.Hx;

/// Route path — matched by the router and shared verbatim with the TypeScript
/// client (MODEL_LIBRARY_PATH in ui/packages/app/lib/api/model_library.ts).
pub const MODEL_LIBRARY_PATH = "/v1/models";

/// The per-model row shape — owned by model_library_store (model_id as `id`,
/// provider, cap, rates). The store's listForLibrary returns these directly.
const LibraryModel = model_library_store.LibraryRow;

const ResponseBody = struct {
    version: []const u8,
    models: []const LibraryModel,
};

/// Serve the catalogue. Bearer auth and the GET method check run in the route
/// table / invoke wrapper before this is reached.
pub fn innerGetModelLibrary(hx: Hx) void {
    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer hx.ctx.pool.release(conn);

    const body = buildResponse(hx.alloc, conn) catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };

    // An empty catalogue is a valid state: the table ships unseeded and
    // platform admins populate it through /v1/admin/models. Return 200 with an
    // empty `models` array — the dashboard renders "no models yet" rather than
    // treating provisioning as broken.
    hx.ok(.ok, body);
}

fn buildResponse(alloc: std.mem.Allocator, conn: *pg.Conn) !ResponseBody {
    const catalogue = try model_library_store.listForLibrary(alloc, conn);
    const version = try formatVersion(alloc, catalogue.max_updated_ms);
    return .{ .version = version, .models = catalogue.models };
}

/// Format the maximum updated_at_ms as YYYY-MM-DD (UTC). An empty catalogue
/// (the table ships unseeded — admins populate it via /admin/models) yields
/// max_updated_ms = 0 → "1970-01-01"; the handler returns that with a 200 and
/// an empty `models` array (a valid not-yet-provisioned state), never a 503.
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

test "MODEL_LIBRARY_PATH is the versioned models route" {
    // pin test: literal is the contract — the wire path the router and the
    // TypeScript client both key on.
    try std.testing.expectEqualStrings("/v1/models", MODEL_LIBRARY_PATH);
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
