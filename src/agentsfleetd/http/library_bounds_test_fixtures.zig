//! Shared seed and identifiers for the §3 bounds suites.
//!
//! Split out when `library_read_bounds_integration_test.zig` crossed the 350-line
//! cap (RULE FLL). The two suites that use it ask different questions — one
//! measures the budget a read stays inside, the other proves an over-ceiling
//! response refuses rather than truncates — but they need the SAME tenant state
//! to ask them, and two copies of a seed is how the two suites quietly stop
//! testing the same page.
//!
//! Not a test file: it registers nothing and asserts nothing on its own.

const std = @import("std");

const base = @import("secrets_json_integration_test.zig");
const harness_mod = @import("test_harness.zig");

pub const MODELS_PATH = "/v1/tenants/me/models";

/// One credential backing every seeded entry. The page is a metadata read, so
/// which credential the rows name does not matter — only that one EXISTS, which
/// keeps the projection on its normal path instead of the degraded
/// `custom_secret` branch that skips the metadata batch's interesting half.
pub const SECRET_NAME = "bounds-probe-key";

/// Three entries and a page of two: enough that the page is genuinely truncated
/// (so `next_cursor` is produced and the result tally is bounded by `limit`
/// rather than by how many rows happen to exist), and few enough that seeding
/// stays three requests.
pub const SEEDED_ENTRIES: usize = 3;
pub const SMALL_PAGE: usize = 2;

/// Create the shared credential the seeded entries name.
pub fn seedCredential(
    alloc: std.mem.Allocator,
    h: *harness_mod.TestHarness,
    api_key: []const u8,
) !void {
    const secrets_path = try std.fmt.allocPrint(alloc, "/v1/workspaces/{s}/secrets", .{base.TEST_WS_ID});
    defer alloc.free(secrets_path);
    const body = try std.fmt.allocPrint(
        alloc,
        "{{\"name\":\"" ++ SECRET_NAME ++ "\",\"data\":{{\"provider\":\"anthropic\",\"api_key\":\"{s}\"}}}}",
        .{api_key},
    );
    defer alloc.free(body);
    const r = try (try (try h.post(secrets_path).bearer(base.TOKEN_OPERATOR)).json(body)).send();
    defer r.deinit();
    try r.expectStatus(.created);
}

/// The credential plus `SEEDED_ENTRIES` registry rows that name it.
pub fn seedEntries(alloc: std.mem.Allocator, h: *harness_mod.TestHarness) !void {
    try seedCredential(alloc, h, "sk-ant-bounds-probe");

    var n: usize = 0;
    while (n < SEEDED_ENTRIES) : (n += 1) {
        const body = try std.fmt.allocPrint(
            alloc,
            "{{\"model_id\":\"claude-bounds-probe-{d}\",\"secret_ref\":\"" ++ SECRET_NAME ++ "\"}}",
            .{n},
        );
        defer alloc.free(body);
        const r = try (try (try h.post(MODELS_PATH).bearer(base.TOKEN_OPERATOR)).json(body)).send();
        defer r.deinit();
        try r.expectStatus(.created);
    }
}

/// Drop whatever a sibling suite left on the shared tenant. Tolerates a pool
/// failure so a `defer` cleanup cannot mask the failure that preceded it.
pub fn cleanup(h: *harness_mod.TestHarness, what: []const u8) void {
    if (h.acquireConn()) |conn| {
        base.cleanupRows(conn);
        h.releaseConn(conn);
    } else |err| {
        std.log.warn("{s} cleanup skipped: {s}", .{ what, @errorName(err) });
    }
}
