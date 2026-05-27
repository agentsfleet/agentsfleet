//! Unit tests for svix_signature middleware (§5.1–5.6).

const std = @import("std");
const httpz = @import("httpz");
const testing = std.testing;

const chain = @import("chain.zig");
const auth_ctx = @import("auth_ctx.zig");
const errors = @import("errors.zig");
const svix = @import("svix_signature.zig");
const hs = @import("hmac_sig");

const SvixLookupResult = svix.SvixLookupResult;
const AuthCtx = auth_ctx.AuthCtx;

// ── Fixtures ─────────────────────────────────────────────────────────

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

const FIXED_NOW: i64 = 1_700_000_000;

fn fixedNow() i64 {
    return FIXED_NOW;
}

const TestLookupCtx = struct {
    return_secret: ?[]const u8 = null,
};

fn testLookup(ctx: *TestLookupCtx, _: []const u8, alloc: std.mem.Allocator) anyerror!?SvixLookupResult {
    const s: ?[]const u8 = if (ctx.return_secret) |r| try alloc.dupe(u8, r) else null;
    return .{ .secret = s };
}

const TestSvix = svix.SvixSignature(*TestLookupCtx);

fn runMw(mw: *TestSvix, ht: anytype) !chain.Outcome {
    var ctx = AuthCtx{
        .alloc = testing.allocator,
        .res = ht.res,
        .req_id = "req_test",
        .write_error = Fixtures.writeError,
        .webhook_zombie_id = "zombie-abc",
    };
    return mw.execute(&ctx, ht.req);
}

// ── Signing helpers ──────────────────────────────────────────────────

const RAW_KEY = "0123456789abcdef01234567"; // 24 bytes
const WHSEC_KEY = "whsec_MDEyMzQ1Njc4OWFiY2RlZjAxMjM0NTY3"; // base64("0123456789abcdef01234567")

const SVIX_ID = "msg_2abcXYZ";
const FRESH_TS = "1699999950"; // 50s before FIXED_NOW — inside 300s drift

fn signEntry(alloc: std.mem.Allocator, id: []const u8, ts: []const u8, body: []const u8) ![]u8 {
    const mac = hs.computeMac(RAW_KEY, &.{ id, ".", ts, ".", body });
    const Encoder = std.base64.standard.Encoder;
    const enc_len = Encoder.calcSize(mac.len);
    const out = try alloc.alloc(u8, 3 + enc_len); // "v1," + base64
    @memcpy(out[0..3], "v1,");
    _ = Encoder.encode(out[3..], &mac);
    return out;
}

// ── 5.1 single valid ─────────────────────────────────────────────────

test "Svix single valid v1 signature → .next" {
    Fixtures.reset();
    const body = "{\"event\":\"user.created\"}";
    const entry = try signEntry(testing.allocator, SVIX_ID, FRESH_TS, body);
    defer testing.allocator.free(entry);

    var lookup_ctx = TestLookupCtx{ .return_secret = WHSEC_KEY };
    var mw = TestSvix{
        .lookup_ctx = &lookup_ctx,
        .lookup_fn = testLookup,
        .now_seconds = fixedNow,
    };

    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    ht.header("svix-id", SVIX_ID);
    ht.header("svix-timestamp", FRESH_TS);
    ht.header("svix-signature", entry);
    ht.body(body);

    const outcome = try runMw(&mw, &ht);
    try testing.expectEqual(chain.Outcome.next, outcome);
    try testing.expectEqual(@as(usize, 0), Fixtures.write_count);
}

// ── 5.2 rotation — first invalid, second valid ───────────────────────

test "Svix rotation: second signature valid → .next" {
    Fixtures.reset();
    const body = "{\"event\":\"session.created\"}";
    const good = try signEntry(testing.allocator, SVIX_ID, FRESH_TS, body);
    defer testing.allocator.free(good);
    const combined = try std.fmt.allocPrint(testing.allocator, "v1,AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA= {s}", .{good});
    defer testing.allocator.free(combined);

    var lookup_ctx = TestLookupCtx{ .return_secret = WHSEC_KEY };
    var mw = TestSvix{
        .lookup_ctx = &lookup_ctx,
        .lookup_fn = testLookup,
        .now_seconds = fixedNow,
    };

    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    ht.header("svix-id", SVIX_ID);
    ht.header("svix-timestamp", FRESH_TS);
    ht.header("svix-signature", combined);
    ht.body(body);

    const outcome = try runMw(&mw, &ht);
    try testing.expectEqual(chain.Outcome.next, outcome);
}

// ── 5.3 tampered body ────────────────────────────────────────────────

test "Svix tampered body rejects with UZ-WH-010" {
    Fixtures.reset();
    const entry = try signEntry(testing.allocator, SVIX_ID, FRESH_TS, "original");
    defer testing.allocator.free(entry);

    var lookup_ctx = TestLookupCtx{ .return_secret = WHSEC_KEY };
    var mw = TestSvix{
        .lookup_ctx = &lookup_ctx,
        .lookup_fn = testLookup,
        .now_seconds = fixedNow,
    };

    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    ht.header("svix-id", SVIX_ID);
    ht.header("svix-timestamp", FRESH_TS);
    ht.header("svix-signature", entry);
    ht.body("tampered");

    const outcome = try runMw(&mw, &ht);
    try testing.expectEqual(chain.Outcome.short_circuit, outcome);
    try testing.expectEqualStrings(errors.ERR_WEBHOOK_SIG_INVALID, Fixtures.last_code);
}

// ── 5.4 stale timestamp ──────────────────────────────────────────────

test "Svix stale timestamp rejects with UZ-WH-011" {
    Fixtures.reset();
    const stale_ts = "1699999000"; // 1000s before FIXED_NOW — outside 300s drift
    const body = "{}";
    const entry = try signEntry(testing.allocator, SVIX_ID, stale_ts, body);
    defer testing.allocator.free(entry);

    var lookup_ctx = TestLookupCtx{ .return_secret = WHSEC_KEY };
    var mw = TestSvix{
        .lookup_ctx = &lookup_ctx,
        .lookup_fn = testLookup,
        .now_seconds = fixedNow,
    };

    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    ht.header("svix-id", SVIX_ID);
    ht.header("svix-timestamp", stale_ts);
    ht.header("svix-signature", entry);
    ht.body(body);

    const outcome = try runMw(&mw, &ht);
    try testing.expectEqual(chain.Outcome.short_circuit, outcome);
    try testing.expectEqualStrings(errors.ERR_WEBHOOK_TIMESTAMP_STALE, Fixtures.last_code);
}

// ── 5.5 missing svix-id ──────────────────────────────────────────────

test "Svix missing svix-id header rejects with UZ-WH-010" {
    Fixtures.reset();
    const body = "{}";
    const entry = try signEntry(testing.allocator, SVIX_ID, FRESH_TS, body);
    defer testing.allocator.free(entry);

    var lookup_ctx = TestLookupCtx{ .return_secret = WHSEC_KEY };
    var mw = TestSvix{
        .lookup_ctx = &lookup_ctx,
        .lookup_fn = testLookup,
        .now_seconds = fixedNow,
    };

    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    // intentionally no svix-id header
    ht.header("svix-timestamp", FRESH_TS);
    ht.header("svix-signature", entry);
    ht.body(body);

    const outcome = try runMw(&mw, &ht);
    try testing.expectEqual(chain.Outcome.short_circuit, outcome);
    try testing.expectEqualStrings(errors.ERR_WEBHOOK_SIG_INVALID, Fixtures.last_code);
}

// ── 5.6 missing whsec_ prefix ────────────────────────────────────────

test "Svix secret missing whsec_ prefix rejects with UZ-WH-010" {
    Fixtures.reset();
    const body = "{}";
    const entry = try signEntry(testing.allocator, SVIX_ID, FRESH_TS, body);
    defer testing.allocator.free(entry);

    // Secret without the `whsec_` prefix — misconfig.
    var lookup_ctx = TestLookupCtx{ .return_secret = "MDEyMzQ1Njc4OWFiY2RlZjAxMjM0NTY3" };
    var mw = TestSvix{
        .lookup_ctx = &lookup_ctx,
        .lookup_fn = testLookup,
        .now_seconds = fixedNow,
    };

    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    ht.header("svix-id", SVIX_ID);
    ht.header("svix-timestamp", FRESH_TS);
    ht.header("svix-signature", entry);
    ht.body(body);

    const outcome = try runMw(&mw, &ht);
    try testing.expectEqual(chain.Outcome.short_circuit, outcome);
    try testing.expectEqualStrings(errors.ERR_WEBHOOK_SIG_INVALID, Fixtures.last_code);
}

// ── Empty decoded secret (defense-in-depth) ──────────────────────────

test "Svix empty decoded secret rejects with UZ-WH-010" {
    Fixtures.reset();
    const body = "{}";
    const entry = try signEntry(testing.allocator, SVIX_ID, FRESH_TS, body);
    defer testing.allocator.free(entry);

    // `whsec_` prefix with empty base64 body — decodes to zero-length secret.
    // HMAC with an empty key is deterministic and attacker-computable; the
    // middleware must reject rather than trust a misconfigured vault entry.
    var lookup_ctx = TestLookupCtx{ .return_secret = "whsec_" };
    var mw = TestSvix{
        .lookup_ctx = &lookup_ctx,
        .lookup_fn = testLookup,
        .now_seconds = fixedNow,
    };

    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    ht.header("svix-id", SVIX_ID);
    ht.header("svix-timestamp", FRESH_TS);
    ht.header("svix-signature", entry);
    ht.body(body);

    const outcome = try runMw(&mw, &ht);
    try testing.expectEqual(chain.Outcome.short_circuit, outcome);
    try testing.expectEqualStrings(errors.ERR_WEBHOOK_SIG_INVALID, Fixtures.last_code);
}

// ── Regression: freshness boundary ───────────────────────────────────

test "Svix isTimestampFresh boundary inclusive at ±max_drift" {
    try testing.expect(svix.isTimestampFresh("1699999700", 1700000000, 300));
    try testing.expect(!svix.isTimestampFresh("1699999699", 1700000000, 300));
    try testing.expect(svix.isTimestampFresh("1700000300", 1700000000, 300));
    try testing.expect(!svix.isTimestampFresh("1700000301", 1700000000, 300));
    try testing.expect(!svix.isTimestampFresh("not-a-number", 1700000000, 300));
    try testing.expect(!svix.isTimestampFresh("0", 1700000000, 300));
}
