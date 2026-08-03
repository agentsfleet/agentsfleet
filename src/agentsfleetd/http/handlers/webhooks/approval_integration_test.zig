//! The Slack approval callback's signature gate and its payload parser.
//!
//! This endpoint resolves approval gates — it is the surface where a button
//! click in Slack becomes an approved or denied decision on a fleet's pending
//! action. It is unauthenticated in the usual sense: there is no bearer token,
//! and the ONLY thing standing between a caller who knows the URL and the
//! ability to approve arbitrary gates is the Hash-based Message Authentication
//! Code (HMAC) check. That check was almost entirely unexecuted.
//!
//! The scheme is `HMAC-SHA256("v0:" ++ timestamp ++ ":" ++ body)` rendered as
//! `v0=<hex>`, carried in `X-Signature` alongside `X-Signature-Timestamp`.
//!
//! Every refusal below is a distinct branch, and the fail-CLOSED default is the
//! most important of them: with no signing secret configured the endpoint must
//! refuse everything rather than fall back to accepting unsigned callbacks. A
//! deployment that forgot to set the secret should approve nothing, not
//! everything.
//!
//! Requires TEST_DATABASE_URL — skipped gracefully otherwise.

const std = @import("std");

const harness_mod = @import("../../test_harness.zig");
const TestHarness = harness_mod.TestHarness;
const auth_mw = @import("../../../auth/middleware/mod.zig");
const clock = @import("common").clock;

const ALLOC = std.testing.allocator;

const HmacSha256 = std.crypto.auth.hmac.sha2.HmacSha256;

const SIGNING_SECRET = "test-approval-signing-secret";
const FLEET_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a7a01";
const HEADER_TIMESTAMP = "x-signature-timestamp";
const HEADER_SIGNATURE = "x-signature";

/// Well-formed and never issued, so `resolve()` reaches its not-found arm
/// without a seeded gate row.
const UNKNOWN_ACTION_ID = "act_0000000000000000000000000000";
const VALID_BODY = "{\"action_id\":\"" ++ UNKNOWN_ACTION_ID ++ "\",\"decision\":\"approve\"}";

/// Older than the five-minute replay window the handler enforces.
const REPLAY_AGE_SECONDS: i64 = 400;
const MS_PER_SECOND: i64 = 1000;

const STATUS_NOT_FOUND: u16 = @intFromEnum(std.http.Status.not_found);
const STATUS_BAD_REQUEST: u16 = @intFromEnum(std.http.Status.bad_request);

/// The secret lives in TWO places on this route and both must hold it: the
/// `webhookHmac` middleware verifies the signature before the handler runs, and
/// the handler re-verifies as defence in depth. Arming only one of them means a
/// correctly-signed request is still refused — by whichever half is unarmed.
fn configureSigned(reg: *auth_mw.MiddlewareRegistry, _: *TestHarness) anyerror!void {
    reg.webhook_hmac_mw.secret = SIGNING_SECRET;
}

/// The deployment that forgot to configure a secret at all.
fn configureUnsigned(_: *auth_mw.MiddlewareRegistry, _: *TestHarness) anyerror!void {}

fn makeHarness() !*TestHarness {
    const h = try TestHarness.start(ALLOC, .{ .configureRegistry = configureSigned });
    // Option-C convention documented on the harness: a secret-gated path is
    // armed by setting the field, never by setenv — the 0.16 environment
    // snapshot ignores a late setenv.
    h.ctx.approval_signing_secret = SIGNING_SECRET;
    return h;
}

fn callbackPath() ![]const u8 {
    return std.fmt.allocPrint(ALLOC, "/v1/webhooks/{s}/approval", .{FLEET_ID});
}

/// `v0=` ++ hex(HMAC-SHA256("v0:" ++ timestamp ++ ":" ++ body)). Caller must free.
fn sign(timestamp: []const u8, body: []const u8) ![]const u8 {
    var mac: [HmacSha256.mac_length]u8 = undefined;
    var mac_state = HmacSha256.init(SIGNING_SECRET);
    mac_state.update("v0:");
    mac_state.update(timestamp);
    mac_state.update(":");
    mac_state.update(body);
    mac_state.final(&mac);
    // bytesToHex, not a `{x}` format specifier: the latter does not hex-encode
    // a byte array here, and the resulting signature was rejected by the very
    // gate these tests exist to exercise.
    const hex = std.fmt.bytesToHex(mac, .lower);
    return std.fmt.allocPrint(ALLOC, "v0={s}", .{&hex});
}

fn nowSeconds() i64 {
    return @divTrunc(clock.nowMillis(), MS_PER_SECOND);
}

/// Sends a correctly-signed callback carrying `body`, and returns its status.
fn sendSigned(h: *TestHarness, body: []const u8) !u16 {
    const ts = try std.fmt.allocPrint(ALLOC, "{d}", .{nowSeconds()});
    defer ALLOC.free(ts);
    const sig = try sign(ts, body);
    defer ALLOC.free(sig);
    const path = try callbackPath();
    defer ALLOC.free(path);

    const r = try (try (try (try h.post(path)
        .header(HEADER_TIMESTAMP, ts))
        .header(HEADER_SIGNATURE, sig))
        .json(body)).send();
    defer r.deinit();
    return r.status;
}

test "integration: with no signing secret configured the callback approves nothing" {
    const h = TestHarness.start(ALLOC, .{ .configureRegistry = configureUnsigned }) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    // Deliberately NOT arming the secret: this is the deployment that forgot.

    const ts = try std.fmt.allocPrint(ALLOC, "{d}", .{nowSeconds()});
    defer ALLOC.free(ts);
    const sig = try sign(ts, VALID_BODY);
    defer ALLOC.free(sig);
    const path = try callbackPath();
    defer ALLOC.free(path);

    // Even a signature that WOULD be correct under the configured secret is
    // refused, because there is no secret to verify it against. Fail-closed.
    const r = try (try (try (try h.post(path)
        .header(HEADER_TIMESTAMP, ts))
        .header(HEADER_SIGNATURE, sig))
        .json(VALID_BODY)).send();
    defer r.deinit();
    try std.testing.expect(r.status >= 400);
}

test "integration: a callback missing either signature header is refused" {
    const h = makeHarness() catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const ts = try std.fmt.allocPrint(ALLOC, "{d}", .{nowSeconds()});
    defer ALLOC.free(ts);
    const sig = try sign(ts, VALID_BODY);
    defer ALLOC.free(sig);
    const path = try callbackPath();
    defer ALLOC.free(path);

    // Timestamp present, signature absent.
    const no_sig = try (try (try h.post(path)
        .header(HEADER_TIMESTAMP, ts))
        .json(VALID_BODY)).send();
    defer no_sig.deinit();
    try std.testing.expect(no_sig.status >= 400);

    // Signature present, timestamp absent — the timestamp is part of the signed
    // material, so accepting without it would defeat the replay window.
    const no_ts = try (try (try h.post(path)
        .header(HEADER_SIGNATURE, sig))
        .json(VALID_BODY)).send();
    defer no_ts.deinit();
    try std.testing.expect(no_ts.status >= 400);
}

test "integration: a timestamp that is unparseable or stale is refused" {
    const h = makeHarness() catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const path = try callbackPath();
    defer ALLOC.free(path);

    // Not a number at all.
    const garbage_sig = try sign("not-a-timestamp", VALID_BODY);
    defer ALLOC.free(garbage_sig);
    const garbage = try (try (try (try h.post(path)
        .header(HEADER_TIMESTAMP, "not-a-timestamp"))
        .header(HEADER_SIGNATURE, garbage_sig))
        .json(VALID_BODY)).send();
    defer garbage.deinit();
    try std.testing.expect(garbage.status >= 400);

    // Correctly signed, but replayed from outside the window. This is the
    // branch that stops a captured callback being resubmitted later.
    const old_ts = try std.fmt.allocPrint(ALLOC, "{d}", .{nowSeconds() - REPLAY_AGE_SECONDS});
    defer ALLOC.free(old_ts);
    const old_sig = try sign(old_ts, VALID_BODY);
    defer ALLOC.free(old_sig);
    const stale = try (try (try (try h.post(path)
        .header(HEADER_TIMESTAMP, old_ts))
        .header(HEADER_SIGNATURE, old_sig))
        .json(VALID_BODY)).send();
    defer stale.deinit();
    try std.testing.expect(stale.status >= 400);
}

test "integration: a signature of the wrong length and one of the wrong bytes are both refused" {
    const h = makeHarness() catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const ts = try std.fmt.allocPrint(ALLOC, "{d}", .{nowSeconds()});
    defer ALLOC.free(ts);
    const path = try callbackPath();
    defer ALLOC.free(path);

    // Short: caught by the length check that precedes the constant-time compare.
    const short = try (try (try (try h.post(path)
        .header(HEADER_TIMESTAMP, ts))
        .header(HEADER_SIGNATURE, "v0=abc"))
        .json(VALID_BODY)).send();
    defer short.deinit();
    try std.testing.expect(short.status >= 400);

    // Right length, wrong bytes — this is what the constant-time compare exists
    // for, and it is the branch an attacker actually probes.
    const good = try sign(ts, VALID_BODY);
    defer ALLOC.free(good);
    const forged = try ALLOC.dupe(u8, good);
    defer ALLOC.free(forged);
    forged[forged.len - 1] = if (forged[forged.len - 1] == 'a') 'b' else 'a';

    const wrong = try (try (try (try h.post(path)
        .header(HEADER_TIMESTAMP, ts))
        .header(HEADER_SIGNATURE, forged))
        .json(VALID_BODY)).send();
    defer wrong.deinit();
    try std.testing.expect(wrong.status >= 400);
}

test "integration: past the signature gate, a malformed payload is refused on its own terms" {
    const h = makeHarness() catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    // Each of these is correctly signed, so the refusal can only come from the
    // parser. Asserting the EXACT status matters: a bare `>= 400` is also
    // satisfied by the signature gate's 401, which would let a broken signing
    // helper turn every case below into a test that passes without ever
    // reaching the parser — that is exactly what happened while writing this.
    const bad_decision = "{\"action_id\":\"" ++ UNKNOWN_ACTION_ID ++ "\",\"decision\":\"maybe\"}";
    const cases = [_][]const u8{
        "not json at all",
        "{\"decision\":\"approve\"}",
        "{\"action_id\":\"\",\"decision\":\"approve\"}",
        bad_decision,
    };
    for (cases) |body| {
        try std.testing.expectEqual(STATUS_BAD_REQUEST, try sendSigned(h, body));
    }
}

test "integration: a correctly signed callback for an unknown action reaches the resolver and 404s" {
    const h = makeHarness() catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    // This is the one request in the file that passes every gate: valid
    // signature, valid payload, real database round trip. The action is
    // genuinely absent, so the resolver's not-found arm is the correct answer —
    // and proves the request got all the way there rather than being refused
    // earlier for the wrong reason.
    try std.testing.expectEqual(STATUS_NOT_FOUND, try sendSigned(h, VALID_BODY));
}
