//! Unit tests for webhook_sig middleware.

const std = @import("std");
const httpz = @import("httpz");
const testing = std.testing;

const chain = @import("chain.zig");
const auth_ctx = @import("auth_ctx.zig");
const errors = @import("errors.zig");
const webhook_sig = @import("webhook_sig.zig");

const LookupResult = webhook_sig.LookupResult;
const SignatureScheme = webhook_sig.SignatureScheme;
const hs = @import("hmac_sig");

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
    return_token: ?[]const u8 = null,
    return_scheme: ?SignatureScheme = null,
    return_signature_secret: ?[]const u8 = null,
    should_fail: bool = false,
    should_return_null: bool = false,
};

fn testLookup(ctx: *TestLookupCtx, _: []const u8, alloc: std.mem.Allocator) anyerror!?LookupResult {
    if (ctx.should_fail) return error.Unavailable;
    if (ctx.should_return_null) return null;
    const token: ?[]const u8 = if (ctx.return_token) |t| try alloc.dupe(u8, t) else null;
    errdefer if (token) |t| alloc.free(t);
    const scheme: ?SignatureScheme = if (ctx.return_scheme) |s| .{
        .sig_header = try alloc.dupe(u8, s.sig_header),
        .prefix = try alloc.dupe(u8, s.prefix),
        .ts_header = if (s.ts_header) |t| try alloc.dupe(u8, t) else null,
        .hmac_version = try alloc.dupe(u8, s.hmac_version),
        .includes_timestamp = s.includes_timestamp,
        .max_ts_drift_seconds = s.max_ts_drift_seconds,
    } else null;
    errdefer if (scheme) |s| {
        alloc.free(s.sig_header);
        alloc.free(s.prefix);
        if (s.ts_header) |t| alloc.free(t);
        alloc.free(s.hmac_version);
    };
    const sig_secret: ?[]const u8 = if (ctx.return_signature_secret) |s| try alloc.dupe(u8, s) else null;
    return .{
        .expected_token = token,
        .signature_scheme = scheme,
        .signature_secret = sig_secret,
    };
}

const TestWebhookSig = webhook_sig.WebhookSig(*TestLookupCtx);

fn runMw(
    mw: *TestWebhookSig,
    ht: anytype,
    zombie_id: ?[]const u8,
) !chain.Outcome {
    var ctx = auth_ctx.AuthCtx{
        .alloc = testing.allocator,
        .res = ht.res,
        .req_id = "req_test",
        .write_error = Fixtures.writeError,
        .webhook_zombie_id = zombie_id,
    };
    return mw.execute(&ctx, ht.req);
}

// ── Bearer token fallback tests ──────────────────────────────────────

test "Bearer token fallback valid returns .next" {
    Fixtures.reset();
    var lookup_ctx = TestLookupCtx{ .return_token = "valid-token" };
    var mw = TestWebhookSig{ .lookup_ctx = &lookup_ctx, .lookup_fn = testLookup };

    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    ht.header("authorization", "Bearer valid-token");

    const outcome = try runMw(&mw, &ht, "zombie-abc");
    try testing.expectEqual(chain.Outcome.next, outcome);
    try testing.expectEqual(@as(usize, 0), Fixtures.write_count);
}

test "Bearer token fallback invalid returns .short_circuit" {
    Fixtures.reset();
    var lookup_ctx = TestLookupCtx{ .return_token = "valid-token" };
    var mw = TestWebhookSig{ .lookup_ctx = &lookup_ctx, .lookup_fn = testLookup };

    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    ht.header("authorization", "Bearer wrong-token");

    const outcome = try runMw(&mw, &ht, "zombie-abc");
    try testing.expectEqual(chain.Outcome.short_circuit, outcome);
    try testing.expectEqualStrings(errors.ERR_UNAUTHORIZED, Fixtures.last_code);
}

// ── Negative tests ───────────────────────────────────────────────────

test "no zombie_id returns .short_circuit" {
    Fixtures.reset();
    var lookup_ctx = TestLookupCtx{};
    var mw = TestWebhookSig{ .lookup_ctx = &lookup_ctx, .lookup_fn = testLookup };

    var ht = httpz.testing.init(.{});
    defer ht.deinit();

    const outcome = try runMw(&mw, &ht, null);
    try testing.expectEqual(chain.Outcome.short_circuit, outcome);
    try testing.expectEqualStrings(errors.ERR_UNAUTHORIZED, Fixtures.last_code);
}

test "lookup returns null (zombie not found) → .short_circuit" {
    Fixtures.reset();
    var lookup_ctx = TestLookupCtx{ .should_return_null = true };
    var mw = TestWebhookSig{ .lookup_ctx = &lookup_ctx, .lookup_fn = testLookup };

    var ht = httpz.testing.init(.{});
    defer ht.deinit();

    const outcome = try runMw(&mw, &ht, "zombie-xyz");
    try testing.expectEqual(chain.Outcome.short_circuit, outcome);
    try testing.expectEqualStrings(errors.ERR_UNAUTHORIZED, Fixtures.last_code);
}

test "lookup error returns .short_circuit" {
    Fixtures.reset();
    var lookup_ctx = TestLookupCtx{ .should_fail = true };
    var mw = TestWebhookSig{ .lookup_ctx = &lookup_ctx, .lookup_fn = testLookup };

    var ht = httpz.testing.init(.{});
    defer ht.deinit();

    const outcome = try runMw(&mw, &ht, "zombie-abc");
    try testing.expectEqual(chain.Outcome.short_circuit, outcome);
    try testing.expectEqualStrings(errors.ERR_UNAUTHORIZED, Fixtures.last_code);
}

test "no Bearer header configured → .short_circuit" {
    Fixtures.reset();
    var lookup_ctx = TestLookupCtx{ .return_token = "some-token" };
    var mw = TestWebhookSig{ .lookup_ctx = &lookup_ctx, .lookup_fn = testLookup };

    var ht = httpz.testing.init(.{});
    defer ht.deinit();

    const outcome = try runMw(&mw, &ht, "zombie-abc");
    try testing.expectEqual(chain.Outcome.short_circuit, outcome);
}

test "no expected_token in config → .short_circuit" {
    Fixtures.reset();
    var lookup_ctx = TestLookupCtx{};
    var mw = TestWebhookSig{ .lookup_ctx = &lookup_ctx, .lookup_fn = testLookup };

    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    ht.header("authorization", "Bearer some-token");

    const outcome = try runMw(&mw, &ht, "zombie-abc");
    try testing.expectEqual(chain.Outcome.short_circuit, outcome);
}

// ── Edge case tests ──────────────────────────────────────────────────

test "Bearer prefix without token body → .short_circuit" {
    Fixtures.reset();
    var lookup_ctx = TestLookupCtx{ .return_token = "valid" };
    var mw = TestWebhookSig{ .lookup_ctx = &lookup_ctx, .lookup_fn = testLookup };

    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    ht.header("authorization", "Bearer ");

    const outcome = try runMw(&mw, &ht, "zombie-abc");
    try testing.expectEqual(chain.Outcome.short_circuit, outcome);
}

test "non-Bearer auth scheme → .short_circuit" {
    Fixtures.reset();
    var lookup_ctx = TestLookupCtx{ .return_token = "valid" };
    var mw = TestWebhookSig{ .lookup_ctx = &lookup_ctx, .lookup_fn = testLookup };

    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    ht.header("authorization", "Basic dXNlcjpwYXNz");

    const outcome = try runMw(&mw, &ht, "zombie-abc");
    try testing.expectEqual(chain.Outcome.short_circuit, outcome);
}

// constantTimeEql canonical tests live in src/crypto/hmac_sig_test.zig.

// ── §3 — HMAC strategy 2 tests ───────────────────────────────────────

/// Jira-style scheme (not in PROVIDER_REGISTRY) — custom header, no timestamp.
const JIRA_SCHEME = SignatureScheme{
    .sig_header = "x-jira-hook-signature",
    .prefix = "sha256=",
    .ts_header = null,
    .hmac_version = "",
    .includes_timestamp = false,
    .max_ts_drift_seconds = 300,
};

const JIRA_SECRET = "jira_test_secret_not_real_00";
const JIRA_BODY = "{\"issue\":{\"key\":\"BUG-1\"}}";

fn hmacHex(buf: []u8, secret: []const u8, parts: []const []const u8, prefix: []const u8) []const u8 {
    const mac = hs.computeMac(secret, parts);
    return hs.encodeMacHex(buf, prefix, mac);
}

test "Jira valid HMAC → .next" {
    Fixtures.reset();
    var sig_buf: [64 + 16]u8 = undefined;
    const sig = hmacHex(&sig_buf, JIRA_SECRET, &.{JIRA_BODY}, "sha256=");

    var lookup_ctx = TestLookupCtx{
        .return_scheme = JIRA_SCHEME,
        .return_signature_secret = JIRA_SECRET,
    };
    var mw = TestWebhookSig{ .lookup_ctx = &lookup_ctx, .lookup_fn = testLookup };

    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    ht.header("x-jira-hook-signature", sig);
    ht.body(JIRA_BODY);

    const outcome = try runMw(&mw, &ht, "zombie-abc");
    try testing.expectEqual(chain.Outcome.next, outcome);
    try testing.expectEqual(@as(usize, 0), Fixtures.write_count);
}

test "tampered body → UZ-WH-010 .short_circuit" {
    Fixtures.reset();
    var sig_buf: [64 + 16]u8 = undefined;
    const sig = hmacHex(&sig_buf, JIRA_SECRET, &.{JIRA_BODY}, "sha256=");

    var lookup_ctx = TestLookupCtx{
        .return_scheme = JIRA_SCHEME,
        .return_signature_secret = JIRA_SECRET,
    };
    var mw = TestWebhookSig{ .lookup_ctx = &lookup_ctx, .lookup_fn = testLookup };

    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    ht.header("x-jira-hook-signature", sig);
    ht.body("{\"issue\":{\"key\":\"TAMPERED\"}}");

    const outcome = try runMw(&mw, &ht, "zombie-abc");
    try testing.expectEqual(chain.Outcome.short_circuit, outcome);
    try testing.expectEqualStrings(errors.ERR_WEBHOOK_SIG_INVALID, Fixtures.last_code);
}

test "stale timestamp → UZ-WH-011 .short_circuit" {
    Fixtures.reset();
    // Scheme with ts_header set (like Svix — we use it here to exercise the
    // freshness branch via webhook_sig rather than svix_signature).
    const ts_scheme = SignatureScheme{
        .sig_header = "x-custom-sig",
        .prefix = "v0=",
        .ts_header = "x-custom-ts",
        .hmac_version = "v0",
        .includes_timestamp = true,
        .max_ts_drift_seconds = 300,
    };
    const stale_ts = "1000000000"; // year 2001 — way outside the 300s window.
    var sig_buf: [64 + 16]u8 = undefined;
    const sig = hmacHex(&sig_buf, "secret", &.{ "v0", ":", stale_ts, ":", "body" }, "v0=");

    var lookup_ctx = TestLookupCtx{
        .return_scheme = ts_scheme,
        .return_signature_secret = "secret",
    };
    var mw = TestWebhookSig{ .lookup_ctx = &lookup_ctx, .lookup_fn = testLookup };

    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    ht.header("x-custom-sig", sig);
    ht.header("x-custom-ts", stale_ts);
    ht.body("body");

    const outcome = try runMw(&mw, &ht, "zombie-abc");
    try testing.expectEqual(chain.Outcome.short_circuit, outcome);
    try testing.expectEqualStrings(errors.ERR_WEBHOOK_TIMESTAMP_STALE, Fixtures.last_code);
}

// ── Fail-closed when HMAC scheme is declared (no Bearer downgrade) ────

test "HMAC scheme configured + secret null → .short_circuit (no Bearer fallthrough)" {
    // Simulates a vault transient failure: serve_webhook_lookup returns
    // signature_scheme populated but signature_secret = null. Middleware
    // must fail closed, NOT fall through to Bearer auth.
    Fixtures.reset();
    var lookup_ctx = TestLookupCtx{
        .return_scheme = JIRA_SCHEME,
        .return_signature_secret = null,
        .return_token = "valid-token", // would-be Bearer fallback (must NOT be used)
    };
    var mw = TestWebhookSig{ .lookup_ctx = &lookup_ctx, .lookup_fn = testLookup };

    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    // Include a valid Bearer so the downgrade path, if taken, would succeed.
    ht.header("authorization", "Bearer valid-token");
    ht.body(JIRA_BODY);

    const outcome = try runMw(&mw, &ht, "zombie-abc");
    try testing.expectEqual(chain.Outcome.short_circuit, outcome);
    try testing.expectEqualStrings(errors.ERR_WEBHOOK_SIG_INVALID, Fixtures.last_code);
}

test "HMAC scheme configured + sig_header missing → .short_circuit (no Bearer fallthrough)" {
    // Scheme + secret both present, but request omits the sig_header. Prior
    // behavior silently fell through to Bearer auth — a header-omission
    // downgrade. Now: fail closed.
    Fixtures.reset();
    var lookup_ctx = TestLookupCtx{
        .return_scheme = JIRA_SCHEME,
        .return_signature_secret = JIRA_SECRET,
        .return_token = "valid-token", // would-be Bearer fallback (must NOT be used)
    };
    var mw = TestWebhookSig{ .lookup_ctx = &lookup_ctx, .lookup_fn = testLookup };

    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    // No x-jira-hook-signature header; valid Bearer that would auth via fallback.
    ht.header("authorization", "Bearer valid-token");
    ht.body(JIRA_BODY);

    const outcome = try runMw(&mw, &ht, "zombie-abc");
    try testing.expectEqual(chain.Outcome.short_circuit, outcome);
    try testing.expectEqualStrings(errors.ERR_WEBHOOK_SIG_INVALID, Fixtures.last_code);
}

test "HMAC empty secret (attacker-computable) → .short_circuit" {
    // Defense-in-depth: vault misconfig stores an empty secret. hs.computeMac("", ...)
    // is deterministic and an attacker who knows the basestring can forge a valid
    // MAC. The middleware must reject this at the verify boundary rather than
    // trusting the vault. Mirrors the same guard in svix_signature.zig.
    Fixtures.reset();
    var sig_buf: [64 + 16]u8 = undefined;
    const sig = hmacHex(&sig_buf, "", &.{JIRA_BODY}, "sha256=");

    var lookup_ctx = TestLookupCtx{
        .return_scheme = JIRA_SCHEME,
        .return_signature_secret = "", // empty vault entry
    };
    var mw = TestWebhookSig{ .lookup_ctx = &lookup_ctx, .lookup_fn = testLookup };

    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    ht.header("x-jira-hook-signature", sig);
    ht.body(JIRA_BODY);

    const outcome = try runMw(&mw, &ht, "zombie-abc");
    try testing.expectEqual(chain.Outcome.short_circuit, outcome);
    try testing.expectEqualStrings(errors.ERR_WEBHOOK_SIG_INVALID, Fixtures.last_code);
}
