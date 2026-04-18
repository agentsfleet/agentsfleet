//! Unit tests for webhook_sig middleware (M28_001).

const std = @import("std");
const httpz = @import("httpz");
const testing = std.testing;

const chain = @import("chain.zig");
const auth_ctx = @import("auth_ctx.zig");
const errors = @import("errors.zig");
const webhook_sig = @import("webhook_sig.zig");

const LookupResult = webhook_sig.LookupResult;

// ── Test fixtures ─────────────────────────────────────────────────────

const Fixtures = struct {
    var last_code: []const u8 = "";
    var write_count: usize = 0;

    fn reset() void {
        last_code = "";
        write_count = 0;
    }

    fn writeError(_: *httpz.Response, code: []const u8, _: []const u8, _: []const u8) void {
        last_code = code;
        write_count += 1;
    }
};

/// Concrete test context — replaces *anyopaque with a typed struct.
const TestLookupCtx = struct {
    return_secret: ?[]const u8 = null,
    return_token: ?[]const u8 = null,
    should_fail: bool = false,
    should_return_null: bool = false,
};

fn testLookup(ctx: *TestLookupCtx, _: []const u8, alloc: std.mem.Allocator) anyerror!?LookupResult {
    if (ctx.should_fail) return error.Unavailable;
    if (ctx.should_return_null) return null;
    const secret: ?[]const u8 = if (ctx.return_secret) |s| try alloc.dupe(u8, s) else null;
    errdefer if (secret) |s| alloc.free(s);
    const token: ?[]const u8 = if (ctx.return_token) |t| try alloc.dupe(u8, t) else null;
    return .{ .expected_secret = secret, .expected_token = token };
}

const TestWebhookSig = webhook_sig.WebhookSig(*TestLookupCtx);

fn runMw(
    mw: *TestWebhookSig,
    ht: anytype,
    zombie_id: ?[]const u8,
    provided_secret: ?[]const u8,
) !chain.Outcome {
    var ctx = auth_ctx.AuthCtx{
        .alloc = testing.allocator,
        .res = ht.res,
        .req_id = "req_test",
        .write_error = Fixtures.writeError,
        .webhook_zombie_id = zombie_id,
        .webhook_provided_secret = provided_secret,
    };
    return mw.execute(&ctx, ht.req);
}

// ── §1 — URL secret tests ────────────────────────────────────────────

test "M28_001 1.1: URL secret match returns .next" {
    Fixtures.reset();
    var lookup_ctx = TestLookupCtx{ .return_secret = "correct-secret" };
    var mw = TestWebhookSig{ .lookup_ctx = &lookup_ctx, .lookup_fn = testLookup };

    var ht = httpz.testing.init(.{});
    defer ht.deinit();

    const outcome = try runMw(&mw, &ht, "zombie-abc", "correct-secret");
    try testing.expectEqual(chain.Outcome.next, outcome);
    try testing.expectEqual(@as(usize, 0), Fixtures.write_count);
}

test "M28_001 1.2: URL secret mismatch returns .short_circuit + 401" {
    Fixtures.reset();
    var lookup_ctx = TestLookupCtx{ .return_secret = "correct-secret" };
    var mw = TestWebhookSig{ .lookup_ctx = &lookup_ctx, .lookup_fn = testLookup };

    var ht = httpz.testing.init(.{});
    defer ht.deinit();

    const outcome = try runMw(&mw, &ht, "zombie-abc", "wrong-secret");
    try testing.expectEqual(chain.Outcome.short_circuit, outcome);
    try testing.expectEqualStrings(errors.ERR_UNAUTHORIZED, Fixtures.last_code);
}

// ── §1 — Bearer token fallback tests ─────────────────────────────────

test "M28_001 1.3: Bearer token fallback valid returns .next" {
    Fixtures.reset();
    var lookup_ctx = TestLookupCtx{ .return_token = "valid-token" };
    var mw = TestWebhookSig{ .lookup_ctx = &lookup_ctx, .lookup_fn = testLookup };

    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    ht.header("authorization", "Bearer valid-token");

    const outcome = try runMw(&mw, &ht, "zombie-abc", null);
    try testing.expectEqual(chain.Outcome.next, outcome);
    try testing.expectEqual(@as(usize, 0), Fixtures.write_count);
}

test "M28_001 1.4: Bearer token fallback invalid returns .short_circuit" {
    Fixtures.reset();
    var lookup_ctx = TestLookupCtx{ .return_token = "valid-token" };
    var mw = TestWebhookSig{ .lookup_ctx = &lookup_ctx, .lookup_fn = testLookup };

    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    ht.header("authorization", "Bearer wrong-token");

    const outcome = try runMw(&mw, &ht, "zombie-abc", null);
    try testing.expectEqual(chain.Outcome.short_circuit, outcome);
    try testing.expectEqualStrings(errors.ERR_UNAUTHORIZED, Fixtures.last_code);
}

// ── Negative tests ───────────────────────────────────────────────────

test "M28_001: no zombie_id returns .short_circuit" {
    Fixtures.reset();
    var lookup_ctx = TestLookupCtx{};
    var mw = TestWebhookSig{ .lookup_ctx = &lookup_ctx, .lookup_fn = testLookup };

    var ht = httpz.testing.init(.{});
    defer ht.deinit();

    const outcome = try runMw(&mw, &ht, null, null);
    try testing.expectEqual(chain.Outcome.short_circuit, outcome);
    try testing.expectEqualStrings(errors.ERR_UNAUTHORIZED, Fixtures.last_code);
}

test "M28_001: lookup returns null (zombie not found) → .short_circuit" {
    Fixtures.reset();
    var lookup_ctx = TestLookupCtx{ .should_return_null = true };
    var mw = TestWebhookSig{ .lookup_ctx = &lookup_ctx, .lookup_fn = testLookup };

    var ht = httpz.testing.init(.{});
    defer ht.deinit();

    const outcome = try runMw(&mw, &ht, "zombie-xyz", "any-secret");
    try testing.expectEqual(chain.Outcome.short_circuit, outcome);
    try testing.expectEqualStrings(errors.ERR_UNAUTHORIZED, Fixtures.last_code);
}

test "M28_001: lookup error returns .short_circuit" {
    Fixtures.reset();
    var lookup_ctx = TestLookupCtx{ .should_fail = true };
    var mw = TestWebhookSig{ .lookup_ctx = &lookup_ctx, .lookup_fn = testLookup };

    var ht = httpz.testing.init(.{});
    defer ht.deinit();

    const outcome = try runMw(&mw, &ht, "zombie-abc", "any");
    try testing.expectEqual(chain.Outcome.short_circuit, outcome);
    try testing.expectEqualStrings(errors.ERR_UNAUTHORIZED, Fixtures.last_code);
}

test "M28_001: no URL secret, no Bearer header → .short_circuit" {
    Fixtures.reset();
    var lookup_ctx = TestLookupCtx{ .return_token = "some-token" };
    var mw = TestWebhookSig{ .lookup_ctx = &lookup_ctx, .lookup_fn = testLookup };

    var ht = httpz.testing.init(.{});
    defer ht.deinit();

    const outcome = try runMw(&mw, &ht, "zombie-abc", null);
    try testing.expectEqual(chain.Outcome.short_circuit, outcome);
}

test "M28_001: no URL secret, no expected_token in config → .short_circuit" {
    Fixtures.reset();
    var lookup_ctx = TestLookupCtx{};
    var mw = TestWebhookSig{ .lookup_ctx = &lookup_ctx, .lookup_fn = testLookup };

    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    ht.header("authorization", "Bearer some-token");

    const outcome = try runMw(&mw, &ht, "zombie-abc", null);
    try testing.expectEqual(chain.Outcome.short_circuit, outcome);
}

// ── Edge case tests ──────────────────────────────────────────────────

test "M28_001: empty vault secret rejects non-empty provided" {
    Fixtures.reset();
    var lookup_ctx = TestLookupCtx{ .return_secret = "" };
    var mw = TestWebhookSig{ .lookup_ctx = &lookup_ctx, .lookup_fn = testLookup };

    var ht = httpz.testing.init(.{});
    defer ht.deinit();

    const outcome = try runMw(&mw, &ht, "zombie-abc", "non-empty");
    try testing.expectEqual(chain.Outcome.short_circuit, outcome);
}

test "M28_001: Bearer prefix without token body → .short_circuit" {
    Fixtures.reset();
    var lookup_ctx = TestLookupCtx{ .return_token = "valid" };
    var mw = TestWebhookSig{ .lookup_ctx = &lookup_ctx, .lookup_fn = testLookup };

    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    ht.header("authorization", "Bearer ");

    const outcome = try runMw(&mw, &ht, "zombie-abc", null);
    try testing.expectEqual(chain.Outcome.short_circuit, outcome);
}

test "M28_001: non-Bearer auth scheme → .short_circuit" {
    Fixtures.reset();
    var lookup_ctx = TestLookupCtx{ .return_token = "valid" };
    var mw = TestWebhookSig{ .lookup_ctx = &lookup_ctx, .lookup_fn = testLookup };

    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    ht.header("authorization", "Basic dXNlcjpwYXNz");

    const outcome = try runMw(&mw, &ht, "zombie-abc", null);
    try testing.expectEqual(chain.Outcome.short_circuit, outcome);
}

// constantTimeEq is tested indirectly via the match/mismatch tests above.
// Direct unit tests live in webhook_sig.zig (same-file access to private fn).
