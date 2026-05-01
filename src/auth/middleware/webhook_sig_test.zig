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
    return_scheme: ?SignatureScheme = null,
    return_signature_secret: ?[]const u8 = null,
    should_fail: bool = false,
    should_return_null: bool = false,
};

fn testLookup(ctx: *TestLookupCtx, _: []const u8, alloc: std.mem.Allocator) anyerror!?LookupResult {
    if (ctx.should_fail) return error.Unavailable;
    if (ctx.should_return_null) return null;
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

// ── Fail-closed: missing scheme / credential / lookup ────────────────

test "no zombie_id slot → UZ-WH-020 .short_circuit" {
    Fixtures.reset();
    var lookup_ctx = TestLookupCtx{};
    var mw = TestWebhookSig{ .lookup_ctx = &lookup_ctx, .lookup_fn = testLookup };

    var ht = httpz.testing.init(.{});
    defer ht.deinit();

    const outcome = try runMw(&mw, &ht, null);
    try testing.expectEqual(chain.Outcome.short_circuit, outcome);
    try testing.expectEqualStrings(errors.ERR_WEBHOOK_CREDENTIAL_NOT_CONFIGURED, Fixtures.last_code);
}

test "lookup returned null (zombie not found) → UZ-WH-020" {
    Fixtures.reset();
    var lookup_ctx = TestLookupCtx{ .should_return_null = true };
    var mw = TestWebhookSig{ .lookup_ctx = &lookup_ctx, .lookup_fn = testLookup };

    var ht = httpz.testing.init(.{});
    defer ht.deinit();

    const outcome = try runMw(&mw, &ht, "zombie-xyz");
    try testing.expectEqual(chain.Outcome.short_circuit, outcome);
    try testing.expectEqualStrings(errors.ERR_WEBHOOK_CREDENTIAL_NOT_CONFIGURED, Fixtures.last_code);
}

test "lookup errored (DB unavailable) → UZ-WH-020" {
    Fixtures.reset();
    var lookup_ctx = TestLookupCtx{ .should_fail = true };
    var mw = TestWebhookSig{ .lookup_ctx = &lookup_ctx, .lookup_fn = testLookup };

    var ht = httpz.testing.init(.{});
    defer ht.deinit();

    const outcome = try runMw(&mw, &ht, "zombie-abc");
    try testing.expectEqual(chain.Outcome.short_circuit, outcome);
    try testing.expectEqualStrings(errors.ERR_WEBHOOK_CREDENTIAL_NOT_CONFIGURED, Fixtures.last_code);
}

test "no signature scheme configured → UZ-WH-020 (Bearer ignored)" {
    // Resolver returns a result with no scheme (provider not recognized) and
    // no secret. Sending a Bearer token used to silently authenticate via the
    // dropped Strategy 2 fallback — that path is gone. Now: fail closed.
    Fixtures.reset();
    var lookup_ctx = TestLookupCtx{};
    var mw = TestWebhookSig{ .lookup_ctx = &lookup_ctx, .lookup_fn = testLookup };

    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    ht.header("authorization", "Bearer some-token");

    const outcome = try runMw(&mw, &ht, "zombie-abc");
    try testing.expectEqual(chain.Outcome.short_circuit, outcome);
    try testing.expectEqualStrings(errors.ERR_WEBHOOK_CREDENTIAL_NOT_CONFIGURED, Fixtures.last_code);
}

// ── §3 — HMAC tests ──────────────────────────────────────────────────

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

// ── Fail-closed: scheme declared but secret/header missing ────

test "scheme configured + secret null → UZ-WH-020 .short_circuit" {
    // Simulates a vault transient failure: serve_webhook_lookup returns
    // signature_scheme populated but signature_secret = null. Middleware
    // emits UZ-WH-020 (recoverable misconfig), not UZ-WH-010 (real auth fail).
    Fixtures.reset();
    var lookup_ctx = TestLookupCtx{
        .return_scheme = JIRA_SCHEME,
        .return_signature_secret = null,
    };
    var mw = TestWebhookSig{ .lookup_ctx = &lookup_ctx, .lookup_fn = testLookup };

    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    // Authorization header is intentionally NOT set — the middleware no
    // longer reads it on webhook routes. (Pre-Bearer-removal, sending one
    // here would have silently downgraded auth.)
    ht.body(JIRA_BODY);

    const outcome = try runMw(&mw, &ht, "zombie-abc");
    try testing.expectEqual(chain.Outcome.short_circuit, outcome);
    try testing.expectEqualStrings(errors.ERR_WEBHOOK_CREDENTIAL_NOT_CONFIGURED, Fixtures.last_code);
}

test "scheme configured + sig_header missing → UZ-WH-010 .short_circuit" {
    // Provider + secret both present but the request omits the signature
    // header. Pre-cleanup this silently fell through to Bearer auth — a
    // header-omission downgrade vector. Now fails closed with UZ-WH-010
    // (signature invalid).
    Fixtures.reset();
    var lookup_ctx = TestLookupCtx{
        .return_scheme = JIRA_SCHEME,
        .return_signature_secret = JIRA_SECRET,
    };
    var mw = TestWebhookSig{ .lookup_ctx = &lookup_ctx, .lookup_fn = testLookup };

    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    // No x-jira-hook-signature header. Authorization header would have been
    // a downgrade vector under the old Strategy 2; verify it has no effect.
    ht.header("authorization", "Bearer would-have-worked-pre-cleanup");
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
