//! Signature and payload proofs for the Slack approval callback.
//!
//! This endpoint turns an unauthenticated HTTP request into an approve/deny on
//! a gated action, so the signature check is the whole access boundary — the
//! file's own header calls it defense in depth behind the `webhookHmac`
//! middleware, which is exactly the kind of second gate that rots unnoticed
//! because the first one is doing the work.
//!
//! The signing string is rebuilt here from the documented format rather than
//! borrowed from the implementation, so a change to what gets signed (dropping
//! the timestamp, dropping the body, changing the `v0:` prefix) fails these
//! tests instead of passing them in lockstep.

const std = @import("std");
const httpz = @import("httpz");
const testing = std.testing;
const constants = @import("common");

const approval = @import("approval.zig");
const hx_mod = @import("../hx.zig");
const common = @import("../common.zig");
const ec = @import("../../../errors/error_registry.zig");

const Hx = hx_mod.Hx;
const HmacSha256 = std.crypto.auth.hmac.sha2.HmacSha256;

const REQ_ID = "req-approval-1";
const FLEET_ID = "01932b7c-0000-7000-8000-0000000000d1";
const SIGNING_SECRET = "approval-callback-test-secret";
const HDR_TIMESTAMP = "x-signature-timestamp";
const HDR_SIGNATURE = "x-signature";
const K_ERROR_CODE = "error_code";
const K_DETAIL = "detail";

/// Beyond the 300-second replay window the handler enforces.
const STALE_OFFSET_S: i64 = 400;
/// The signature timestamp is seconds; the clock reads milliseconds.
const MILLIS_PER_SECOND: i64 = 1_000;
const SIG_PREFIX = "v0=";
const SIGNED_PREFIX = "v0:";
const SIGNED_SEP = ":";

const VALID_BODY =
    \\{"action_id":"act_001","decision":"approve"}
;

fn buildCtx(secret: ?[]const u8) common.Context {
    // SAFETY: the signature gate and the body parser read only this field; every
    // path under test returns before anything else on the context is touched.
    var ctx: common.Context = undefined;
    ctx.approval_signing_secret = secret;
    return ctx;
}

fn buildHx(res: *httpz.Response, ctx: *common.Context, alloc: std.mem.Allocator) Hx {
    return Hx{
        .alloc = alloc,
        // SAFETY: a Slack callback carries no principal — that is why the
        // signature is the access boundary.
        .principal = undefined,
        .req_id = REQ_ID,
        .ctx = ctx,
        .res = res,
    };
}

fn nowSeconds() i64 {
    return @divTrunc(constants.clock.nowMillis(), MILLIS_PER_SECOND);
}

/// Rebuild the signature independently of the implementation: HMAC-SHA256 over
/// `v0:<timestamp>:<body>`, rendered as `v0=<lowercase hex>`.
fn sign(out: *[SIG_PREFIX.len + HmacSha256.mac_length * 2]u8, secret: []const u8, timestamp: []const u8, body: []const u8) []const u8 {
    var mac: [HmacSha256.mac_length]u8 = undefined;
    var h = HmacSha256.init(secret);
    h.update(SIGNED_PREFIX);
    h.update(timestamp);
    h.update(SIGNED_SEP);
    h.update(body);
    h.final(&mac);

    @memcpy(out[0..SIG_PREFIX.len], SIG_PREFIX);
    const hex = std.fmt.bytesToHex(mac, .lower);
    @memcpy(out[SIG_PREFIX.len..], &hex);
    return out;
}

fn expectSignatureRefused(ht: *httpz.testing.Testing, detail: []const u8) !void {
    const json = try ht.getJson();
    try testing.expectEqualStrings(ec.ERR_APPROVAL_INVALID_SIGNATURE, json.object.get(K_ERROR_CODE).?.string);
    try testing.expectEqualStrings(detail, json.object.get(K_DETAIL).?.string);
}

// ── the signature gate ───────────────────────────────────────────────────

test "should refuse every callback when no signing secret is configured" {
    var arena = std.heap.ArenaAllocator.init(testing.allocator);
    defer arena.deinit();
    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    var ctx = buildCtx(null);
    ht.body(VALID_BODY);

    // Fail-closed: an unconfigured deployment must reject, never wave the
    // callback through unverified.
    approval.innerApprovalCallback(buildHx(ht.res, &ctx, arena.allocator()), ht.req, FLEET_ID);

    try expectSignatureRefused(&ht, "Signing secret not configured");
}

test "should refuse a callback that presents no timestamp" {
    var arena = std.heap.ArenaAllocator.init(testing.allocator);
    defer arena.deinit();
    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    var ctx = buildCtx(SIGNING_SECRET);
    ht.header(HDR_SIGNATURE, "v0=00");
    ht.body(VALID_BODY);

    approval.innerApprovalCallback(buildHx(ht.res, &ctx, arena.allocator()), ht.req, FLEET_ID);

    try expectSignatureRefused(&ht, "Missing signature timestamp");
}

test "should refuse a callback that presents no signature" {
    var arena = std.heap.ArenaAllocator.init(testing.allocator);
    defer arena.deinit();
    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    var ctx = buildCtx(SIGNING_SECRET);
    var ts_buf: [24]u8 = undefined;
    ht.header(HDR_TIMESTAMP, try std.fmt.bufPrint(&ts_buf, "{d}", .{nowSeconds()}));
    ht.body(VALID_BODY);

    approval.innerApprovalCallback(buildHx(ht.res, &ctx, arena.allocator()), ht.req, FLEET_ID);

    try expectSignatureRefused(&ht, "Missing signature");
}

test "should refuse a timestamp that is not a number" {
    var arena = std.heap.ArenaAllocator.init(testing.allocator);
    defer arena.deinit();
    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    var ctx = buildCtx(SIGNING_SECRET);
    ht.header(HDR_TIMESTAMP, "not-a-timestamp");
    ht.header(HDR_SIGNATURE, "v0=00");
    ht.body(VALID_BODY);

    approval.innerApprovalCallback(buildHx(ht.res, &ctx, arena.allocator()), ht.req, FLEET_ID);

    try expectSignatureRefused(&ht, "Invalid timestamp");
}

test "should refuse a correctly signed callback replayed outside the window" {
    var arena = std.heap.ArenaAllocator.init(testing.allocator);
    defer arena.deinit();
    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    var ctx = buildCtx(SIGNING_SECRET);

    var ts_buf: [24]u8 = undefined;
    const stale = try std.fmt.bufPrint(&ts_buf, "{d}", .{nowSeconds() - STALE_OFFSET_S});
    var sig_buf: [SIG_PREFIX.len + HmacSha256.mac_length * 2]u8 = undefined;
    // The signature itself is valid — only its age is not. A captured callback
    // must not stay replayable forever.
    ht.header(HDR_TIMESTAMP, stale);
    ht.header(HDR_SIGNATURE, sign(&sig_buf, SIGNING_SECRET, stale, VALID_BODY));
    ht.body(VALID_BODY);

    approval.innerApprovalCallback(buildHx(ht.res, &ctx, arena.allocator()), ht.req, FLEET_ID);

    try expectSignatureRefused(&ht, "Timestamp too old");
}

test "should refuse a timestamp far in the future, not only a stale one" {
    var arena = std.heap.ArenaAllocator.init(testing.allocator);
    defer arena.deinit();
    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    var ctx = buildCtx(SIGNING_SECRET);

    var ts_buf: [24]u8 = undefined;
    // A one-sided check would let a sender park a signature far ahead of now and
    // replay it for as long as it takes the clock to catch up.
    const ahead = try std.fmt.bufPrint(&ts_buf, "{d}", .{nowSeconds() + STALE_OFFSET_S});
    var sig_buf: [SIG_PREFIX.len + HmacSha256.mac_length * 2]u8 = undefined;
    ht.header(HDR_TIMESTAMP, ahead);
    ht.header(HDR_SIGNATURE, sign(&sig_buf, SIGNING_SECRET, ahead, VALID_BODY));
    ht.body(VALID_BODY);

    approval.innerApprovalCallback(buildHx(ht.res, &ctx, arena.allocator()), ht.req, FLEET_ID);

    try expectSignatureRefused(&ht, "Timestamp too old");
}

test "should refuse a signature of the wrong length before comparing it" {
    var arena = std.heap.ArenaAllocator.init(testing.allocator);
    defer arena.deinit();
    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    var ctx = buildCtx(SIGNING_SECRET);
    var ts_buf: [24]u8 = undefined;
    ht.header(HDR_TIMESTAMP, try std.fmt.bufPrint(&ts_buf, "{d}", .{nowSeconds()}));
    ht.header(HDR_SIGNATURE, "v0=deadbeef");
    ht.body(VALID_BODY);

    approval.innerApprovalCallback(buildHx(ht.res, &ctx, arena.allocator()), ht.req, FLEET_ID);

    try expectSignatureRefused(&ht, "Invalid signature");
}

test "should refuse a signature minted for a different body" {
    var arena = std.heap.ArenaAllocator.init(testing.allocator);
    defer arena.deinit();
    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    var ctx = buildCtx(SIGNING_SECRET);

    var ts_buf: [24]u8 = undefined;
    const ts = try std.fmt.bufPrint(&ts_buf, "{d}", .{nowSeconds()});
    var sig_buf: [SIG_PREFIX.len + HmacSha256.mac_length * 2]u8 = undefined;
    // Signed an approve, delivered a deny: the body must be inside the MAC or a
    // captured approval can be flipped in flight.
    ht.header(HDR_TIMESTAMP, ts);
    ht.header(HDR_SIGNATURE, sign(&sig_buf, SIGNING_SECRET, ts, VALID_BODY));
    ht.body(
        \\{"action_id":"act_001","decision":"deny"}
    );

    approval.innerApprovalCallback(buildHx(ht.res, &ctx, arena.allocator()), ht.req, FLEET_ID);

    try expectSignatureRefused(&ht, "Invalid signature");
}

test "should refuse a signature minted under a different secret" {
    var arena = std.heap.ArenaAllocator.init(testing.allocator);
    defer arena.deinit();
    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    var ctx = buildCtx(SIGNING_SECRET);

    var ts_buf: [24]u8 = undefined;
    const ts = try std.fmt.bufPrint(&ts_buf, "{d}", .{nowSeconds()});
    var sig_buf: [SIG_PREFIX.len + HmacSha256.mac_length * 2]u8 = undefined;
    ht.header(HDR_TIMESTAMP, ts);
    ht.header(HDR_SIGNATURE, sign(&sig_buf, "a-different-secret", ts, VALID_BODY));
    ht.body(VALID_BODY);

    approval.innerApprovalCallback(buildHx(ht.res, &ctx, arena.allocator()), ht.req, FLEET_ID);

    try expectSignatureRefused(&ht, "Invalid signature");
}

test "should refuse a signature minted for a different timestamp" {
    var arena = std.heap.ArenaAllocator.init(testing.allocator);
    defer arena.deinit();
    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    var ctx = buildCtx(SIGNING_SECRET);

    var ts_buf: [24]u8 = undefined;
    var other_buf: [24]u8 = undefined;
    const ts = try std.fmt.bufPrint(&ts_buf, "{d}", .{nowSeconds()});
    // Both inside the freshness window, so only the MAC can tell them apart —
    // which it can only do if the timestamp is signed.
    const other = try std.fmt.bufPrint(&other_buf, "{d}", .{nowSeconds() - 1});
    var sig_buf: [SIG_PREFIX.len + HmacSha256.mac_length * 2]u8 = undefined;
    ht.header(HDR_TIMESTAMP, ts);
    ht.header(HDR_SIGNATURE, sign(&sig_buf, SIGNING_SECRET, other, VALID_BODY));
    ht.body(VALID_BODY);

    approval.innerApprovalCallback(buildHx(ht.res, &ctx, arena.allocator()), ht.req, FLEET_ID);

    try expectSignatureRefused(&ht, "Invalid signature");
}

// ── the payload parser, past a genuine signature ─────────────────────────

/// Sign `body` for now and deliver it. Everything past the gate is the parser,
/// so these prove the signature check is not what refused the request.
fn deliverSigned(ht: *httpz.testing.Testing, ctx: *common.Context, alloc: std.mem.Allocator, body: ?[]const u8) !void {
    var ts_buf: [24]u8 = undefined;
    const ts = try std.fmt.bufPrint(&ts_buf, "{d}", .{nowSeconds()});
    var sig_buf: [SIG_PREFIX.len + HmacSha256.mac_length * 2]u8 = undefined;
    ht.header(HDR_TIMESTAMP, ts);
    ht.header(HDR_SIGNATURE, sign(&sig_buf, SIGNING_SECRET, ts, body orelse ""));
    if (body) |b| ht.body(b);
    approval.innerApprovalCallback(buildHx(ht.res, ctx, alloc), ht.req, FLEET_ID);
}

fn expectParseRefused(ht: *httpz.testing.Testing, detail: []const u8) !void {
    const json = try ht.getJson();
    try testing.expectEqualStrings(ec.ERR_APPROVAL_PARSE_FAILED, json.object.get(K_ERROR_CODE).?.string);
    try testing.expectEqualStrings(detail, json.object.get(K_DETAIL).?.string);
}

test "should refuse a signed callback that carries no body" {
    var arena = std.heap.ArenaAllocator.init(testing.allocator);
    defer arena.deinit();
    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    var ctx = buildCtx(SIGNING_SECRET);

    try deliverSigned(&ht, &ctx, arena.allocator(), null);

    try expectParseRefused(&ht, ec.MSG_APPROVAL_INVALID_BODY);
}

test "should refuse a signed callback whose body is not JSON" {
    var arena = std.heap.ArenaAllocator.init(testing.allocator);
    defer arena.deinit();
    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    var ctx = buildCtx(SIGNING_SECRET);

    try deliverSigned(&ht, &ctx, arena.allocator(), "not json");

    try expectParseRefused(&ht, ec.MSG_APPROVAL_INVALID_BODY);
}

test "should refuse a signed callback with an empty action id" {
    var arena = std.heap.ArenaAllocator.init(testing.allocator);
    defer arena.deinit();
    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    var ctx = buildCtx(SIGNING_SECRET);

    try deliverSigned(&ht, &ctx, arena.allocator(),
        \\{"action_id":"","decision":"approve"}
    );

    try expectParseRefused(&ht, ec.MSG_APPROVAL_INVALID_BODY);
}

test "should refuse a decision that is neither approve nor deny" {
    var arena = std.heap.ArenaAllocator.init(testing.allocator);
    defer arena.deinit();
    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    var ctx = buildCtx(SIGNING_SECRET);

    // A distinct message from the malformed-body one: the payload was readable,
    // the verb was not one this gate accepts.
    try deliverSigned(&ht, &ctx, arena.allocator(),
        \\{"action_id":"act_001","decision":"maybe"}
    );

    try expectParseRefused(&ht, ec.MSG_APPROVAL_INVALID_DECISION);
}

// NOT TESTED HERE — the resolve path past the parser. It writes Redis and
// Postgres through `approval_gate.resolve`, so the accepted-callback arms
// (resolved, already_resolved, not_found) belong to the integration suite. What
// is provable without them is that nothing reaches that path unsigned, stale,
// or malformed.
